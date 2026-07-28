//! Sprint E, Part 4 — runaway detection.
//!
//! "The incident that motivated this was not a slow drain, it was one
//! session that would not stop." Three independent detectors, any of
//! which trips a pause:
//!
//! 1. [`RunawayDetectors::check_burn_rate`] — tokens/minute vs the rolling
//!    p90 for the same (repo, stage), sustained over a window
//!    ([`sustained_breach`]).
//! 2. [`RunawayDetectors::check_cost_per_progress`] — spend since the last
//!    gate pass vs a multiple of the stage's p90. "This is the one that
//!    would have caught my incident."
//! 3. [`RunawayDetectors::check_hard_ceiling`] — an absolute, unconditional
//!    per-session cost cap.

use lopi_core::Money;

/// Which detector tripped, with the evidence that goes straight onto
/// `AgentEvent::RunawayPaused` — no second query needed to build the
/// operator-facing decision prompt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RunawayVerdict {
    /// Detector #1 — sustained burn-rate breach.
    BurnRate {
        /// Observed tokens/minute for this session.
        observed_tokens_per_min: f64,
        /// The rolling p90 baseline for this (repo, stage).
        baseline_p90_tokens_per_min: f64,
    },
    /// Detector #2 — spend since the last gate pass exceeds a multiple of
    /// the stage's p90 without producing a pass. The brief's "this is the
    /// one that would have caught my incident."
    CostPerProgress {
        /// Spend accumulated since the last gate pass (or task start).
        spend_since_gate: Money,
        /// `stage_p90 * cost_per_progress_multiplier`.
        threshold: Money,
    },
    /// Detector #3 — unconditional absolute per-session ceiling.
    HardCeiling {
        /// Total spend this session.
        spend: Money,
        /// The configured hard ceiling.
        ceiling: Money,
    },
}

impl RunawayVerdict {
    /// Stable tag matching `AgentEvent::RunawayPaused::detector`.
    #[must_use]
    pub const fn detector_name(self) -> &'static str {
        match self {
            Self::BurnRate { .. } => "burn_rate",
            Self::CostPerProgress { .. } => "cost_per_progress",
            Self::HardCeiling { .. } => "hard_ceiling",
        }
    }
}

/// Config-driven runaway detectors. Cheap to construct — pure config
/// values, no I/O.
#[derive(Debug, Clone, Copy)]
pub struct RunawayDetectors {
    hard_session_ceiling: Money,
    cost_per_progress_multiplier: f64,
}

impl RunawayDetectors {
    /// Build from the resolved `[economics]` config values.
    #[must_use]
    pub const fn new(hard_session_ceiling: Money, cost_per_progress_multiplier: f64) -> Self {
        Self {
            hard_session_ceiling,
            cost_per_progress_multiplier,
        }
    }

    /// Detector #1 — a single burn-rate sample breach (pair with
    /// [`sustained_breach`] to require this hold over a config window
    /// before acting on it).
    #[must_use]
    pub fn check_burn_rate(
        &self,
        observed_tokens_per_min: f64,
        baseline_p90_tokens_per_min: f64,
    ) -> Option<RunawayVerdict> {
        (baseline_p90_tokens_per_min > 0.0 && observed_tokens_per_min > baseline_p90_tokens_per_min)
            .then_some(RunawayVerdict::BurnRate {
                observed_tokens_per_min,
                baseline_p90_tokens_per_min,
            })
    }

    /// Detector #2 — spend since the last gate pass vs `stage_p90 *
    /// cost_per_progress_multiplier`.
    #[must_use]
    pub fn check_cost_per_progress(
        &self,
        spend_since_gate: Money,
        stage_p90: Money,
    ) -> Option<RunawayVerdict> {
        let threshold = Money::from_usd(stage_p90.to_usd() * self.cost_per_progress_multiplier);
        (spend_since_gate > threshold).then_some(RunawayVerdict::CostPerProgress {
            spend_since_gate,
            threshold,
        })
    }

    /// Detector #3 — unconditional.
    #[must_use]
    pub fn check_hard_ceiling(&self, spend: Money) -> Option<RunawayVerdict> {
        (spend > self.hard_session_ceiling).then_some(RunawayVerdict::HardCeiling {
            spend,
            ceiling: self.hard_session_ceiling,
        })
    }

    /// Run all three in priority order (burn rate, cost-per-progress, hard
    /// ceiling) and return the first that trips. Detector #2 is checked
    /// before #3 deliberately: a looping session that hasn't yet blown the
    /// absolute ceiling should still be caught by the progress signal —
    /// that ordering is exactly what the runaway drill (`LEDGER.md`)
    /// verifies.
    #[must_use]
    pub fn check_all(
        &self,
        observed_tokens_per_min: f64,
        baseline_p90_tokens_per_min: f64,
        spend_since_gate: Money,
        stage_p90: Money,
        total_spend: Money,
    ) -> Option<RunawayVerdict> {
        self.check_burn_rate(observed_tokens_per_min, baseline_p90_tokens_per_min)
            .or_else(|| self.check_cost_per_progress(spend_since_gate, stage_p90))
            .or_else(|| self.check_hard_ceiling(total_spend))
    }
}

/// `true` once the last `required_consecutive` samples in `recent_breaches`
/// are all breaches — "sustained breach over a config window," not a
/// single noisy spike. `recent_breaches` is a rolling window the caller
/// maintains (oldest first); fewer samples than `required_consecutive`
/// never trips, since there isn't yet enough evidence of a sustained trend.
#[must_use]
pub fn sustained_breach(recent_breaches: &[bool], required_consecutive: usize) -> bool {
    required_consecutive > 0
        && recent_breaches.len() >= required_consecutive
        && recent_breaches
            .iter()
            .rev()
            .take(required_consecutive)
            .all(|&b| b)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn detectors() -> RunawayDetectors {
        RunawayDetectors::new(Money::from_usd(20.0), 3.0)
    }

    #[test]
    fn burn_rate_trips_when_over_baseline() {
        let d = detectors();
        let v = d.check_burn_rate(500.0, 200.0).expect("should trip");
        assert_eq!(v.detector_name(), "burn_rate");
    }

    #[test]
    fn burn_rate_does_not_trip_at_or_under_baseline() {
        let d = detectors();
        assert!(d.check_burn_rate(150.0, 200.0).is_none());
        assert!(d.check_burn_rate(200.0, 200.0).is_none());
    }

    #[test]
    fn burn_rate_ignores_zero_baseline_cold_start() {
        // No history yet (baseline 0) must never itself be a "breach" —
        // that would trip on every session's very first sample.
        let d = detectors();
        assert!(d.check_burn_rate(50.0, 0.0).is_none());
    }

    #[test]
    fn cost_per_progress_trips_past_the_multiplier() {
        let d = detectors();
        let stage_p90 = Money::from_usd(2.0);
        // threshold = 2.0 * 3.0 = 6.0
        assert!(d
            .check_cost_per_progress(Money::from_usd(6.01), stage_p90)
            .is_some());
        assert!(d
            .check_cost_per_progress(Money::from_usd(6.0), stage_p90)
            .is_none());
    }

    #[test]
    fn hard_ceiling_is_unconditional() {
        let d = detectors();
        assert!(d.check_hard_ceiling(Money::from_usd(20.01)).is_some());
        assert!(d.check_hard_ceiling(Money::from_usd(20.0)).is_none());
    }

    #[test]
    fn check_all_prefers_cost_per_progress_over_hard_ceiling() {
        // The runaway drill's key claim: detector #2 trips before #3 for a
        // looping session that hasn't yet blown the absolute ceiling.
        let d = detectors();
        let verdict = d.check_all(
            0.0,                   // burn rate under baseline
            1000.0,                // baseline
            Money::from_usd(10.0), // spend since gate — over 3x p90
            Money::from_usd(2.0),  // stage p90 -> threshold 6.0
            Money::from_usd(10.0), // total spend — under the 20.0 hard ceiling
        );
        assert_eq!(
            verdict,
            Some(RunawayVerdict::CostPerProgress {
                spend_since_gate: Money::from_usd(10.0),
                threshold: Money::from_usd(6.0),
            })
        );
    }

    #[test]
    fn check_all_falls_through_to_hard_ceiling_when_nothing_else_trips() {
        let d = detectors();
        let verdict = d.check_all(
            0.0,
            1000.0,
            Money::from_usd(1.0),
            Money::from_usd(2.0),
            Money::from_usd(25.0),
        );
        assert_eq!(
            verdict,
            Some(RunawayVerdict::HardCeiling {
                spend: Money::from_usd(25.0),
                ceiling: Money::from_usd(20.0),
            })
        );
    }

    #[test]
    fn check_all_is_none_when_nothing_trips() {
        let d = detectors();
        assert_eq!(
            d.check_all(
                0.0,
                1000.0,
                Money::from_usd(1.0),
                Money::from_usd(2.0),
                Money::from_usd(5.0),
            ),
            None
        );
    }

    #[test]
    fn sustained_breach_requires_the_full_window() {
        assert!(!sustained_breach(&[true, true], 3));
        assert!(!sustained_breach(&[true, false, true], 3));
        assert!(sustained_breach(&[false, true, true, true], 3));
    }

    #[test]
    fn sustained_breach_of_zero_window_never_trips() {
        assert!(!sustained_breach(&[true, true, true], 0));
    }
}
