//! Cost governor + circuit breakers — hierarchical budget enforcement.
//!
//! Three nested scopes — fleet (whole `lopi sail`), agent (one running runner),
//! task (one task across its attempts) — each with its own [`BudgetLimit`] and
//! a private [`CircuitBreaker`] (`Closed → Open → HalfOpen`).
//!
//! Before any billable call (planner, scorer, post-mortem), code calls
//! [`BudgetGovernor::check`]. The governor walks the three breakers
//! innermost-first and returns the tightest enclosing scope that refuses, so
//! the runner can attribute the failure correctly.
//!
//! After the call returns, callers report the outcome via
//! [`BudgetGovernor::record_success`] (with the actual cost in USD),
//! [`BudgetGovernor::record_failure`], or [`BudgetGovernor::record_cost_only`]
//! (for successful free calls). Costs feed each breaker's rolling 1-hour
//! window; failures bump the consecutive-failure counters.
//!
//! Pair the governor with `lopi_core::AgentEvent::BudgetExceeded`: the moment
//! `check` would reject, the runner emits that event before propagating the
//! error so the UI can flag the breach before the next turn fires.
//!
//! Lives in `lopi-ratelimit` (tier 2) because [`CircuitBreaker`] lives here.
//! The scope label ([`BudgetScope`]) is re-exported from `lopi-core` (tier 1)
//! so events can carry it without an upward dependency.
//!
//! [`CircuitBreaker`]: crate::CircuitBreaker
//! [`BudgetScope`]: lopi_core::BudgetScope

use crate::{BreakerError, BreakerState, CircuitBreaker};
use lopi_core::BudgetScope;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Cap for a single scope. `usd_per_hour` becomes the rolling 1-hour cost
/// limit inside the underlying [`CircuitBreaker`]; `max_consecutive_failures`
/// is the breaker's failure threshold before it trips Open.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BudgetLimit {
    /// Maximum USD that can be burned in a rolling 1-hour window.
    pub usd_per_hour: f64,
    /// Consecutive failed calls before the breaker opens.
    pub max_consecutive_failures: u32,
    /// How long the breaker stays Open before transitioning to HalfOpen.
    pub open_duration_secs: u64,
}

impl Default for BudgetLimit {
    fn default() -> Self {
        Self {
            usd_per_hour: 25.0,
            max_consecutive_failures: 5,
            open_duration_secs: 60,
        }
    }
}

/// Three-tier budget — defaults are conservative caps for a single-developer
/// `lopi sail` instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub fleet: BudgetLimit,
    pub agent: BudgetLimit,
    pub task: BudgetLimit,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            fleet: BudgetLimit {
                usd_per_hour: 25.0,
                ..BudgetLimit::default()
            },
            agent: BudgetLimit {
                usd_per_hour: 5.0,
                ..BudgetLimit::default()
            },
            task: BudgetLimit {
                usd_per_hour: 1.5,
                ..BudgetLimit::default()
            },
        }
    }
}

/// Reason a `check` was rejected.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum BudgetError {
    /// The scope's hourly cost cap is reached — no further billable calls
    /// allowed in this window.
    #[error("budget exceeded at {scope:?}: ${limit_usd:.4}/hr cap reached")]
    Exceeded { scope: BudgetScope, limit_usd: f64 },
    /// The scope's breaker tripped Open after too many consecutive failures.
    #[error("circuit breaker open at {scope:?}")]
    BreakerOpen { scope: BudgetScope },
}

/// Hierarchical governor wrapping three [`CircuitBreaker`]s.
///
/// `Arc<CircuitBreaker>` per scope so a single `BudgetGovernor` can be shared
/// across the pool's agents without holding a Mutex. `CircuitBreaker` does
/// not implement `Debug`, so neither does this — call `config()` instead.
///
/// [`CircuitBreaker`]: crate::CircuitBreaker
#[derive(Clone)]
pub struct BudgetGovernor {
    fleet: Arc<CircuitBreaker>,
    agent: Arc<CircuitBreaker>,
    task: Arc<CircuitBreaker>,
    config: BudgetConfig,
}

impl BudgetGovernor {
    /// Build a governor from a config — one breaker per scope.
    #[must_use]
    pub fn new(config: BudgetConfig) -> Self {
        let fleet = Arc::new(CircuitBreaker::new(
            config.fleet.max_consecutive_failures,
            Duration::from_secs(config.fleet.open_duration_secs),
            config.fleet.usd_per_hour,
        ));
        let agent = Arc::new(CircuitBreaker::new(
            config.agent.max_consecutive_failures,
            Duration::from_secs(config.agent.open_duration_secs),
            config.agent.usd_per_hour,
        ));
        let task = Arc::new(CircuitBreaker::new(
            config.task.max_consecutive_failures,
            Duration::from_secs(config.task.open_duration_secs),
            config.task.usd_per_hour,
        ));
        Self {
            fleet,
            agent,
            task,
            config,
        }
    }

    /// Snapshot of the active config — useful for inclusion in
    /// `AgentEvent::BudgetExceeded` payloads.
    #[must_use]
    pub fn config(&self) -> &BudgetConfig {
        &self.config
    }

    /// Check whether the next billable call is allowed across all three
    /// scopes. Returns the **innermost** scope that refuses.
    ///
    /// # Errors
    /// Returns [`BudgetError::Exceeded`] if any breaker has hit its hourly
    /// cap, or [`BudgetError::BreakerOpen`] if any breaker tripped from
    /// consecutive failures.
    pub async fn check(&self) -> Result<(), BudgetError> {
        // Innermost-first so the most specific scope wins.
        map_check(
            self.task.check().await,
            BudgetScope::Task,
            self.config.task.usd_per_hour,
        )?;
        map_check(
            self.agent.check().await,
            BudgetScope::Agent,
            self.config.agent.usd_per_hour,
        )?;
        map_check(
            self.fleet.check().await,
            BudgetScope::Fleet,
            self.config.fleet.usd_per_hour,
        )?;
        Ok(())
    }

    /// Report a successful billable call — resets failure counters and
    /// records the actual cost against every scope's hourly window.
    pub async fn record_success(&self, actual_cost_usd: f64) {
        self.fleet.record_success().await;
        self.agent.record_success().await;
        self.task.record_success().await;
        self.record_cost_only(actual_cost_usd).await;
    }

    /// Record cost without touching the failure counters (e.g. a free call
    /// that still counted against a quota).
    pub async fn record_cost_only(&self, usd: f64) {
        if usd > 0.0 {
            self.fleet.record_cost(usd).await;
            self.agent.record_cost(usd).await;
            self.task.record_cost(usd).await;
        }
    }

    /// Report a failed call — bumps consecutive-failure counters; on
    /// `max_consecutive_failures` the breaker trips Open.
    pub async fn record_failure(&self) {
        self.fleet.record_failure().await;
        self.agent.record_failure().await;
        self.task.record_failure().await;
    }

    /// Current state of each breaker — useful for `/metrics` exposition.
    pub async fn states(&self) -> BudgetStates {
        BudgetStates {
            fleet: self.fleet.state().await,
            agent: self.agent.state().await,
            task: self.task.state().await,
        }
    }
}

fn map_check(
    result: Result<(), BreakerError>,
    scope: BudgetScope,
    limit_usd: f64,
) -> Result<(), BudgetError> {
    match result {
        Ok(()) => Ok(()),
        Err(BreakerError::Open) => Err(BudgetError::BreakerOpen { scope }),
        Err(BreakerError::CostCapExceeded { cap: _ }) => {
            Err(BudgetError::Exceeded { scope, limit_usd })
        }
    }
}

/// Snapshot of every breaker state — driven by [`BudgetGovernor::states`].
#[derive(Debug, Clone, Copy)]
pub struct BudgetStates {
    pub fleet: BreakerState,
    pub agent: BreakerState,
    pub task: BreakerState,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tight_config() -> BudgetConfig {
        BudgetConfig {
            fleet: BudgetLimit {
                usd_per_hour: 10.0,
                max_consecutive_failures: 3,
                open_duration_secs: 30,
            },
            agent: BudgetLimit {
                usd_per_hour: 5.0,
                max_consecutive_failures: 3,
                open_duration_secs: 30,
            },
            task: BudgetLimit {
                usd_per_hour: 1.0,
                max_consecutive_failures: 3,
                open_duration_secs: 30,
            },
        }
    }

    #[tokio::test]
    async fn check_passes_when_under_all_limits() {
        let g = BudgetGovernor::new(tight_config());
        assert!(g.check().await.is_ok());
    }

    #[tokio::test]
    async fn task_scope_trips_first_on_cost() {
        // Innermost scope (task=$1/hr) is tightest, so it must trip before
        // agent ($5) or fleet ($10) when 1.5 USD lands.
        let g = BudgetGovernor::new(tight_config());
        g.record_cost_only(1.5).await;
        let err = g.check().await.expect_err("task cap should be exceeded");
        match err {
            BudgetError::Exceeded { scope, limit_usd } => {
                assert_eq!(scope, BudgetScope::Task);
                assert!((limit_usd - 1.0).abs() < f64::EPSILON);
            }
            other => panic!("expected Exceeded at task scope, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn failures_trip_breaker_after_threshold() {
        let g = BudgetGovernor::new(tight_config());
        for _ in 0..3 {
            g.record_failure().await;
        }
        let err = g
            .check()
            .await
            .expect_err("breaker should be open after 3 failures");
        match err {
            BudgetError::BreakerOpen { scope } => {
                assert_eq!(scope, BudgetScope::Task);
            }
            other => panic!("expected BreakerOpen, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn record_success_resets_failure_count() {
        let g = BudgetGovernor::new(tight_config());
        g.record_failure().await;
        g.record_failure().await;
        assert!(g.check().await.is_ok());
        g.record_success(0.01).await;
        g.record_failure().await;
        g.record_failure().await;
        assert!(g.check().await.is_ok());
    }

    #[tokio::test]
    async fn states_snapshot_starts_closed() {
        let g = BudgetGovernor::new(BudgetConfig::default());
        let s = g.states().await;
        assert_eq!(s.fleet, BreakerState::Closed);
        assert_eq!(s.agent, BreakerState::Closed);
        assert_eq!(s.task, BreakerState::Closed);
    }

    #[test]
    fn default_config_has_conservative_caps() {
        let c = BudgetConfig::default();
        assert!(
            c.fleet.usd_per_hour >= c.agent.usd_per_hour,
            "fleet must be a superset cap of agent"
        );
        assert!(
            c.agent.usd_per_hour >= c.task.usd_per_hour,
            "agent must be a superset cap of task"
        );
    }
}
