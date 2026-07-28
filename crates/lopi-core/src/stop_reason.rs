//! Progress-Gating (A3) — the reason a loop stopped.
//!
//! A3's termination paths are *specific*, not a generic "stopped": a loop halts
//! because its goal was met, because it ran out of budget, because it stopped
//! making progress, or because it hit its hard iteration ceiling. Recording
//! *which* is what lets an operator (and B1's stack sequencer) tell "done" apart
//! from "gave up", and drives the precedence when more than one condition trips
//! in the same iteration.
//!
//! **Precedence** (highest first): [`GoalMet`](StopReason::GoalMet) >
//! [`Budget`](StopReason::Budget) > [`NoProgress`](StopReason::NoProgress) >
//! [`MaxIterations`](StopReason::MaxIterations). A goal that is met is a success
//! however much budget was spent; a budget cap is a hard resource ceiling that
//! outranks the softer no-progress heuristic; and the fixed iteration cap is the
//! last-resort backstop.

use serde::{Deserialize, Serialize};

/// Why a progress-gated loop terminated. Ordered so `>=`/`max` selects the
/// higher-precedence reason when two conditions trip together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The hard iteration ceiling (`max_iterations`) was reached — the
    /// last-resort backstop, lowest precedence.
    MaxIterations,
    /// The loop stopped gaining for `no_progress_limit` consecutive rounds.
    NoProgress,
    /// The metered token budget for this loop was exhausted.
    Budget,
    /// The loop's acceptance goal was satisfied — the success terminal,
    /// highest precedence.
    GoalMet,
}

impl StopReason {
    /// Precedence rank, higher wins. Mirrors the enum's declaration order so
    /// [`Ord`]-based `max` and this rank agree.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::MaxIterations => 0,
            Self::NoProgress => 1,
            Self::Budget => 2,
            Self::GoalMet => 3,
        }
    }

    /// The higher-precedence of two reasons — the one that "wins" when both
    /// conditions trip in the same iteration.
    #[must_use]
    pub fn precede(self, other: Self) -> Self {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }

    /// Stable wire/log string — persisted on the run and surfaced to the UI.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MaxIterations => "max_iterations",
            Self::NoProgress => "no_progress",
            Self::Budget => "budget",
            Self::GoalMet => "goal_met",
        }
    }

    /// Whether this reason represents a *successful* termination (the goal was
    /// met) as opposed to a resource/progress cutoff.
    #[must_use]
    pub const fn is_success(self) -> bool {
        matches!(self, Self::GoalMet)
    }

    /// Parse the `StopReason` back out of a `TaskStatus::Failed { reason }`
    /// string built with the `"StopReason::{tag} {{ ... }}"` convention
    /// (`runner/progress.rs::record_progress_stop`, `runner/run_loop.rs`'s
    /// `MaxIterations` backstop). Verification gate (Finding #1) — the
    /// single choke point every terminal task outcome passes through
    /// (`AgentPool::run_one`) uses this to detect "did this task exhaust its
    /// retry budget without meeting its goal" without a second, parallel
    /// signal threaded through the runner/pool boundary: the reason string
    /// already carries the answer, once.
    ///
    /// `GoalMet` never round-trips through this — a goal-met run terminates
    /// as `TaskStatus::Success`, never `Failed`, so a reason string can only
    /// ever encode one of the three non-success reasons. Any string that
    /// doesn't match the `"StopReason::{tag} "` prefix (a cancellation, a
    /// dry-run, a non-retryable API error, a plan/implement spawn failure)
    /// returns `None` — those are not retry exhaustion and must not be
    /// dead-lettered as if they were.
    #[must_use]
    pub fn parse_from_failure_reason(reason: &str) -> Option<Self> {
        let tag = reason.strip_prefix("StopReason::")?;
        let tag = tag.split(' ').next()?;
        [
            Self::MaxIterations,
            Self::NoProgress,
            Self::Budget,
            Self::GoalMet,
        ]
        .into_iter()
        .find(|r| r.as_str() == tag)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn precedence_is_goal_budget_noprogress_maxiter() {
        // The pre-registered precedence: goal-met > budget > no-progress > max-iter.
        assert!(StopReason::GoalMet.rank() > StopReason::Budget.rank());
        assert!(StopReason::Budget.rank() > StopReason::NoProgress.rank());
        assert!(StopReason::NoProgress.rank() > StopReason::MaxIterations.rank());
    }

    #[test]
    fn precede_picks_the_higher_precedence_reason() {
        assert_eq!(
            StopReason::NoProgress.precede(StopReason::Budget),
            StopReason::Budget
        );
        assert_eq!(
            StopReason::Budget.precede(StopReason::NoProgress),
            StopReason::Budget
        );
        assert_eq!(
            StopReason::MaxIterations.precede(StopReason::GoalMet),
            StopReason::GoalMet
        );
        // Same reason is idempotent.
        assert_eq!(
            StopReason::Budget.precede(StopReason::Budget),
            StopReason::Budget
        );
    }

    #[test]
    fn only_goal_met_is_success() {
        assert!(StopReason::GoalMet.is_success());
        for r in [
            StopReason::Budget,
            StopReason::NoProgress,
            StopReason::MaxIterations,
        ] {
            assert!(!r.is_success());
        }
    }

    #[test]
    fn strings_are_stable() {
        assert_eq!(StopReason::GoalMet.as_str(), "goal_met");
        assert_eq!(StopReason::Budget.as_str(), "budget");
        assert_eq!(StopReason::NoProgress.as_str(), "no_progress");
        assert_eq!(StopReason::MaxIterations.as_str(), "max_iterations");
    }

    // ── parse_from_failure_reason (verification gate — dead-letter) ─────────

    #[test]
    fn parses_every_non_success_reason_out_of_its_own_string() {
        for r in [
            StopReason::MaxIterations,
            StopReason::NoProgress,
            StopReason::Budget,
        ] {
            let encoded = format!("StopReason::{} {{ some detail here }}", r.as_str());
            assert_eq!(StopReason::parse_from_failure_reason(&encoded), Some(r));
        }
    }

    #[test]
    fn parses_the_bare_max_iterations_backstop_string() {
        // The exact string `run_loop.rs`'s bottom-of-loop backstop builds.
        let encoded = "StopReason::max_iterations { Max retries exceeded }";
        assert_eq!(
            StopReason::parse_from_failure_reason(encoded),
            Some(StopReason::MaxIterations)
        );
    }

    #[test]
    fn unrelated_failure_reasons_do_not_parse() {
        for reason in [
            "Cancelled",
            "dry-run complete",
            "TurnLimitExceeded { limit: 25, task_id: t }",
            "NonRetryableApiError: insufficient credits",
        ] {
            assert_eq!(StopReason::parse_from_failure_reason(reason), None);
        }
    }

    #[test]
    fn empty_and_malformed_strings_do_not_parse() {
        assert_eq!(StopReason::parse_from_failure_reason(""), None);
        assert_eq!(StopReason::parse_from_failure_reason("StopReason::"), None);
        assert_eq!(
            StopReason::parse_from_failure_reason("StopReason::not_a_real_tag { x }"),
            None
        );
    }

    #[test]
    fn round_trips_through_json() {
        for r in [
            StopReason::GoalMet,
            StopReason::Budget,
            StopReason::NoProgress,
            StopReason::MaxIterations,
        ] {
            let json = serde_json::to_string(&r).unwrap();
            let back: StopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(back, r);
        }
    }
}
