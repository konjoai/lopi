//! Sprint E (Finding #10) — the economics layer: predicts spend before
//! committing it, degrades in stages instead of dying, and reports unit
//! economics. See `LEDGER.md`'s Sprint E entry for why this is built fresh
//! here rather than extending `lopi_ratelimit::BudgetGovernor` (unwired
//! dead code — never call it from here).
//!
//! Split per file per the brief's constraint (`budget/{pool,estimate,
//! reserve,ladder,detect,report}.rs`):
//! - [`reserve`] — the single-writer reservation ledger (Part 2).
//! - [`pool`] — the active [`lopi_core::Pool`] + runway (Part 1/5).
//! - [`estimate`] — historical median/p90 cost estimation (Part 2).
//! - [`ladder`] — the degradation ladder + handoff writer (Part 3).
//! - [`detect`] — runaway detectors (Part 4).
//! - [`report`] — unit economics (Part 5).

pub mod detect;
pub mod estimate;
pub mod ladder;
pub mod pool;
pub mod report;
pub mod reserve;

use detect::RunawayDetectors;
use estimate::CostEstimator;
use ladder::Ladder;
use lopi_core::{BudgetTier, EconomicsConfig, Money};
use lopi_memory::MemoryStore;
use pool::PoolState;
use reserve::ReservationId;
use std::time::Duration;

/// The two pipeline stages every task's admission estimate is built from —
/// "plan" and "implement" are the two dominant, always-billed stages
/// (verify/score ride the same session's context, not a separate billed
/// call in the common CLI path). A task's admission reservation is this
/// sum for one attempt — it deliberately does not multiply by
/// `max_retries`, since most tasks succeed well under their retry ceiling
/// and reserving worst-case-times-retries against every admission would
/// starve the pool. Documented as a known simplification in `LEDGER.md`.
const ADMISSION_STAGES: [&str; 2] = ["plan", "implement"];

/// Why an admission was declined — enough to build the brief's `declined:
/// p90 estimate $4.20, headroom $2.10 — this task fits at effort=medium
/// ($1.80)` message without a second query.
#[derive(Debug, Clone, PartialEq)]
pub struct AdmissionDecline {
    /// The p90 estimate that didn't fit.
    pub p90: Money,
    /// Headroom that was available.
    pub headroom: Money,
    /// A concrete alternative that *would* fit right now, if one was found
    /// among lower effort levels.
    pub alternative: Option<String>,
}

impl AdmissionDecline {
    /// Render the brief's exact message shape.
    #[must_use]
    pub fn message(&self) -> String {
        let alt = self
            .alternative
            .clone()
            .unwrap_or_else(|| "no configuration fits right now".to_string());
        format!(
            "declined: p90 estimate {} , headroom {} — {alt}",
            self.p90, self.headroom
        )
    }
}

/// The economics layer's live facade — one per running `lopi sail`
/// process, held by `AgentPool` behind `Option` (the layer is entirely
/// inactive when `lopi.toml` has no `[economics]` pool configured).
/// Composes [`pool::PoolState`] (the resource + reservation ledger),
/// [`ladder::Ladder`] (current degradation tier), [`estimate::CostEstimator`]
/// (historical p90), and [`detect::RunawayDetectors`].
pub struct Economics {
    /// The active pool + its reservation ledger.
    pub pool: PoolState,
    /// The current degradation tier.
    pub ladder: Ladder,
    estimator: CostEstimator,
    /// Runaway detectors, exposed for callers that drive their own
    /// monitoring loop (`AgentPool`'s runaway check).
    pub detectors: RunawayDetectors,
    thresholds: lopi_core::LadderThresholds,
    reservation_ttl: Duration,
}

impl Economics {
    /// Build the facade from config + a store handle, or `None` if no
    /// pool is configured — the deliberate opt-in the brief calls for.
    #[must_use]
    pub fn new(cfg: &EconomicsConfig, store: MemoryStore) -> Option<Self> {
        let pool_cfg = cfg.pool.clone()?;
        Some(Self {
            pool: PoolState::new(pool_cfg),
            ladder: Ladder::new(),
            estimator: CostEstimator::new(
                store,
                cfg.cold_start_sample_min,
                cfg.cold_start_default_cost,
            ),
            detectors: RunawayDetectors::new(cfg.hard_session_ceiling, cfg.cost_per_progress_multiplier),
            thresholds: cfg.ladder,
            reservation_ttl: Duration::from_secs(cfg.reservation_ttl_secs),
        })
    }

    /// Re-derive the ladder tier from current headroom. Returns
    /// `Some((from, to))` only on a genuine transition — callers use this
    /// to decide whether to emit `AgentEvent::BudgetTier`.
    pub async fn recheck_ladder(&self) -> Option<(BudgetTier, BudgetTier)> {
        let ratio = self.pool.headroom_ratio().await;
        self.ladder.recheck(ratio, &self.thresholds)
    }

    /// Estimate and reserve the p90 cost of one attempt (`plan` +
    /// `implement`) for `(repo, model, effort)`. On success, returns the
    /// reservation id and the amount actually reserved (their sum) —
    /// callers must [`reconcile`](Self::reconcile) or
    /// [`release`](Self::release) it once the task finishes or is
    /// abandoned.
    ///
    /// # Errors
    /// Returns [`AdmissionDecline`] when the estimate doesn't fit current
    /// headroom, with a computed alternative when a lower effort level
    /// would.
    pub async fn try_admit(
        &self,
        repo: Option<&str>,
        model: &str,
        effort: Option<&str>,
    ) -> Result<(ReservationId, Money), AdmissionDecline> {
        let p90 = self.task_p90(repo, model, effort).await;
        match self.pool.try_reserve(p90, self.reservation_ttl).await {
            Ok(id) => Ok((id, p90)),
            Err(decline) => {
                let alternative = self.find_fitting_effort(repo, model, decline.headroom).await;
                Err(AdmissionDecline {
                    p90,
                    headroom: decline.headroom,
                    alternative,
                })
            }
        }
    }

    /// Reconcile a reservation against its actual cost.
    pub async fn reconcile(&self, id: ReservationId, actual: Money) {
        self.pool.reconcile(id, actual).await;
    }

    /// Release a reservation without recording spend (task never started,
    /// or was declined after a speculative reserve).
    pub async fn release(&self, id: ReservationId) {
        self.pool.release(id).await;
    }

    async fn task_p90(&self, repo: Option<&str>, model: &str, effort: Option<&str>) -> Money {
        let mut total = Money::ZERO;
        for stage in ADMISSION_STAGES {
            let est = self
                .estimator
                .estimate(repo, stage, model, effort)
                .await
                .unwrap_or(estimate::CostEstimate {
                    median: Money::ZERO,
                    p90: Money::ZERO,
                    sample_size: 0,
                    cold_start: true,
                });
            total += est.p90;
        }
        total
    }

    /// Try each effort level from `high` down to `low` and report the
    /// first whose p90 estimate fits `headroom` — the brief's "say what
    /// would fit" requirement. `None` if nothing fits.
    async fn find_fitting_effort(&self, repo: Option<&str>, model: &str, headroom: Money) -> Option<String> {
        for level in ["low", "medium", "high"] {
            let p90 = self.task_p90(repo, model, Some(level)).await;
            if p90 <= headroom {
                return Some(format!("fits at effort={level} ({p90})"));
            }
        }
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use lopi_core::Pool;

    fn cfg(usd: f64) -> EconomicsConfig {
        let mut c = EconomicsConfig::default();
        c.pool = Some(Pool::AgentSdkCredits {
            monthly_allotment: Money::from_usd(usd),
            resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date"),
        });
        c
    }

    #[tokio::test]
    async fn no_pool_configured_disables_the_layer() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        assert!(Economics::new(&EconomicsConfig::default(), store).is_none());
    }

    #[tokio::test]
    async fn cold_start_admission_uses_configured_default_and_reserves_it() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let econ = Economics::new(&cfg(100.0), store).expect("pool configured");
        // Cold start default is $1.00/stage * 2 stages = $2.00.
        let (id, reserved) = econ
            .try_admit(None, "claude-sonnet-5", None)
            .await
            .expect("must fit under a $100 ceiling");
        assert_eq!(reserved, Money::from_usd(2.0));
        assert_eq!(econ.pool.headroom().await, Money::from_usd(98.0));
        econ.release(id).await;
        assert_eq!(econ.pool.headroom().await, Money::from_usd(100.0));
    }

    #[tokio::test]
    async fn admission_declines_with_message_when_pool_is_too_thin() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        // $1.00 ceiling; cold-start estimate is $2.00 — must not fit.
        let econ = Economics::new(&cfg(1.0), store).expect("pool configured");
        let decline = econ
            .try_admit(None, "claude-sonnet-5", None)
            .await
            .expect_err("must not fit a $1 ceiling");
        assert_eq!(decline.p90, Money::from_usd(2.0));
        assert_eq!(decline.headroom, Money::from_usd(1.0));
        assert!(decline.message().starts_with("declined:"));
    }

    #[tokio::test]
    async fn recheck_ladder_reports_transitions_as_headroom_shrinks() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let econ = Economics::new(&cfg(10.0), store).expect("pool configured");
        assert_eq!(econ.recheck_ladder().await, None, "starts at Full, ratio 1.0");
        let (id, _) = econ
            .try_admit(None, "claude-sonnet-5", None)
            .await
            .expect("fits");
        // Reserve most of the pool to push headroom ratio down.
        let _big = econ
            .pool
            .try_reserve(Money::from_usd(7.0), Duration::from_secs(60))
            .await
            .expect("fits");
        let transition = econ.recheck_ladder().await;
        assert!(transition.is_some(), "headroom dropped, ladder must move");
        econ.release(id).await;
    }
}
