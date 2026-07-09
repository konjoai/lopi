//! Eval-Execution-1 (A1) — the one eval-result object, three consumers
//! (cross-cutting seam #3).
//!
//! [`EvalOutcome`] is the single object the tiered executor produces. It is
//! shaped so its three future consumers can read it now, even though only the
//! `verdict` is acted on this sprint:
//!
//! - **A2 reflection** reads [`EvalOutcome::critique`] (flattened gaps +
//!   fix-hints) → the next iteration's prompt.
//! - **A3 ratchet** reads [`EvalOutcome::score`] (the weighted scalar) →
//!   accept/reject vs best-so-far.
//! - **A3/B1 termination** reads [`EvalOutcome::verdict`] + the persisted score
//!   trajectory → no-progress / goal-met stop.
//!
//! Aggregation is **fail-closed**: a check that errors is never a silent pass.

use crate::acceptance::EvalTier;
use serde::{Deserialize, Serialize};

/// The pass/fail/error decision for one check or the whole outcome.
///
/// [`Verdict::Error`] is a first-class, explicit not-passing state — a gate
/// that could not be evaluated (verifier/exec error) is treated as failing,
/// never silently passed. Only [`Verdict::Pass`] is passing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The check/goal was met.
    Pass,
    /// The check/goal was evaluated and not met.
    Fail,
    /// The check/goal could not be evaluated (fail-closed → not passing).
    Error,
}

impl Verdict {
    /// The single source of truth for "does the loop treat this as passing?".
    /// Only [`Verdict::Pass`] passes; [`Verdict::Fail`] and [`Verdict::Error`]
    /// both block — the fail-closed guarantee.
    #[must_use]
    pub const fn is_passing(self) -> bool {
        matches!(self, Self::Pass)
    }

    /// The short string form persisted in the store and surfaced on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Error => "error",
        }
    }
}

/// The result of running a single [`crate::acceptance::AcceptanceCheck`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckResult {
    /// The tier this check ran at.
    pub tier: EvalTier,
    /// The check's verdict.
    pub verdict: Verdict,
    /// This check's contribution to the scalar score, in `[0, 1]`.
    pub score: f32,
    /// The check's weight (carried through from the acceptance so a reloaded
    /// outcome is self-describing).
    pub weight: f32,
    /// Whether the check was a hard gate.
    pub required: bool,
    /// Unmet-criteria descriptions (one per gap).
    pub gaps: Vec<String>,
    /// Imperative fix hints for the next attempt (A2 reads these).
    pub fix_hints: Vec<String>,
    /// Evaluator confidence in `[0, 1]` (`1.0` for deterministic tiers).
    pub confidence: f32,
}

impl CheckResult {
    /// A passing deterministic result (confidence `1.0`, no gaps).
    #[must_use]
    pub fn pass(tier: EvalTier, weight: f32, required: bool) -> Self {
        Self {
            tier,
            verdict: Verdict::Pass,
            score: 1.0,
            weight,
            required,
            gaps: Vec::new(),
            fix_hints: Vec::new(),
            confidence: 1.0,
        }
    }

    /// A failing result carrying gaps + fix hints (deterministic confidence).
    #[must_use]
    pub fn fail(
        tier: EvalTier,
        weight: f32,
        required: bool,
        gaps: Vec<String>,
        fix_hints: Vec<String>,
    ) -> Self {
        Self {
            tier,
            verdict: Verdict::Fail,
            score: 0.0,
            weight,
            required,
            gaps,
            fix_hints,
            confidence: 1.0,
        }
    }

    /// An error result — the check could not be evaluated. Fail-closed: this
    /// contributes a `0.0` score and, when required, forces the whole outcome
    /// to [`Verdict::Error`].
    #[must_use]
    pub fn error(tier: EvalTier, weight: f32, required: bool, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            tier,
            verdict: Verdict::Error,
            score: 0.0,
            weight,
            required,
            gaps: vec![reason.clone()],
            fix_hints: vec![format!("resolve the evaluation error: {reason}")],
            confidence: 0.0,
        }
    }
}

/// The one eval result — verdict + score + per-check detail + critique.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalOutcome {
    /// The aggregate pass/fail/error decision (A3/B1 termination reads this).
    pub verdict: Verdict,
    /// The weighted scalar score in `[0, 1]` (A3 ratchet reads this).
    pub score: f32,
    /// One result per check that ran (reflection + per-tier termination).
    pub per_check: Vec<CheckResult>,
    /// Flattened gaps + fix hints across failing checks (A2 reflection reads
    /// this and routes it into the next attempt's prompt).
    pub critique: Vec<String>,
}

impl EvalOutcome {
    /// Aggregate per-check results into one outcome — **fail-closed**.
    ///
    /// Verdict precedence over `required` checks: any [`Verdict::Error`] ⇒
    /// [`Verdict::Error`]; else any [`Verdict::Fail`] ⇒ [`Verdict::Fail`];
    /// else [`Verdict::Pass`]. Non-`required` checks never change the verdict —
    /// they feed the score and critique only.
    ///
    /// `score` is the weight-weighted mean of per-check scores; when the total
    /// weight is zero it falls back to `1.0` for a pass and `0.0` otherwise.
    #[must_use]
    pub fn aggregate(per_check: Vec<CheckResult>) -> Self {
        let verdict = Self::aggregate_verdict(&per_check);
        let score = Self::weighted_score(&per_check, verdict);
        let critique = Self::collect_critique(&per_check);
        Self {
            verdict,
            score,
            per_check,
            critique,
        }
    }

    /// The fail-closed verdict rule over the required checks.
    fn aggregate_verdict(per_check: &[CheckResult]) -> Verdict {
        let mut worst = Verdict::Pass;
        for c in per_check.iter().filter(|c| c.required) {
            match c.verdict {
                Verdict::Error => return Verdict::Error,
                Verdict::Fail => worst = Verdict::Fail,
                Verdict::Pass => {}
            }
        }
        worst
    }

    /// Weighted mean of per-check scores, with a verdict-derived fallback when
    /// every check has zero weight.
    fn weighted_score(per_check: &[CheckResult], verdict: Verdict) -> f32 {
        let total_weight: f32 = per_check.iter().map(|c| c.weight).sum();
        if total_weight <= 0.0 {
            return if verdict.is_passing() { 1.0 } else { 0.0 };
        }
        let weighted: f32 = per_check.iter().map(|c| c.weight * c.score).sum();
        (weighted / total_weight).clamp(0.0, 1.0)
    }

    /// Flatten gaps + fix hints from every non-passing check into the critique.
    fn collect_critique(per_check: &[CheckResult]) -> Vec<String> {
        let mut out = Vec::new();
        for c in per_check.iter().filter(|c| !c.verdict.is_passing()) {
            out.extend(c.gaps.iter().cloned());
            out.extend(c.fix_hints.iter().cloned());
        }
        out
    }

    /// An empty (vacuously passing) outcome — used when a task carries no
    /// acceptance and the legacy `score.passed()` gate decides instead.
    #[must_use]
    pub fn vacuous_pass() -> Self {
        Self {
            verdict: Verdict::Pass,
            score: 1.0,
            per_check: Vec::new(),
            critique: Vec::new(),
        }
    }

    /// Whether the loop should treat this outcome as passing (fail-closed).
    #[must_use]
    pub const fn is_passing(&self) -> bool {
        self.verdict.is_passing()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn only_pass_is_passing() {
        assert!(Verdict::Pass.is_passing());
        assert!(!Verdict::Fail.is_passing());
        assert!(!Verdict::Error.is_passing());
    }

    #[test]
    fn verdict_as_str_is_stable() {
        assert_eq!(Verdict::Pass.as_str(), "pass");
        assert_eq!(Verdict::Fail.as_str(), "fail");
        assert_eq!(Verdict::Error.as_str(), "error");
    }

    #[test]
    fn all_required_pass_aggregates_to_pass() {
        let outcome = EvalOutcome::aggregate(vec![
            CheckResult::pass(EvalTier::ExecutionOk, 1.0, true),
            CheckResult::pass(EvalTier::Judge, 1.0, true),
        ]);
        assert_eq!(outcome.verdict, Verdict::Pass);
        assert_eq!(outcome.score, 1.0);
        assert!(outcome.critique.is_empty());
    }

    #[test]
    fn a_required_fail_fails_the_outcome() {
        let outcome = EvalOutcome::aggregate(vec![
            CheckResult::pass(EvalTier::ExecutionOk, 1.0, true),
            CheckResult::fail(
                EvalTier::Judge,
                1.0,
                true,
                vec!["missing test".into()],
                vec!["add a test".into()],
            ),
        ]);
        assert_eq!(outcome.verdict, Verdict::Fail);
        assert!(outcome.critique.contains(&"missing test".to_string()));
        assert!(outcome.critique.contains(&"add a test".to_string()));
    }

    #[test]
    fn a_required_error_beats_a_fail_and_is_not_passing() {
        // Fail-closed: Error must win over Fail and must block.
        let outcome = EvalOutcome::aggregate(vec![
            CheckResult::fail(EvalTier::ShellTest, 1.0, true, vec![], vec![]),
            CheckResult::error(EvalTier::Judge, 1.0, true, "API 500"),
        ]);
        assert_eq!(outcome.verdict, Verdict::Error);
        assert!(!outcome.is_passing());
    }

    #[test]
    fn non_required_failures_never_block() {
        let outcome = EvalOutcome::aggregate(vec![
            CheckResult::pass(EvalTier::ExecutionOk, 1.0, true),
            CheckResult::fail(
                EvalTier::Judge,
                1.0,
                false,
                vec!["style nit".into()],
                vec![],
            ),
            CheckResult::error(EvalTier::Suite, 1.0, false, "suite unavailable"),
        ]);
        assert_eq!(outcome.verdict, Verdict::Pass);
        // …but their critique still surfaces for reflection.
        assert!(outcome.critique.contains(&"style nit".to_string()));
    }

    #[test]
    fn weighted_score_mixes_by_weight() {
        // 0.5-weighted 1.0 + 1.5-weighted 0.0 → 0.5/2.0 = 0.25.
        let mut a = CheckResult::pass(EvalTier::ExecutionOk, 0.5, true);
        a.score = 1.0;
        let mut b = CheckResult::pass(EvalTier::ShellTest, 1.5, true);
        b.score = 0.0;
        let outcome = EvalOutcome::aggregate(vec![a, b]);
        assert_eq!(outcome.score, 0.25);
    }

    #[test]
    fn zero_total_weight_falls_back_to_verdict() {
        let pass =
            EvalOutcome::aggregate(vec![CheckResult::pass(EvalTier::ExecutionOk, 0.0, true)]);
        assert_eq!(pass.score, 1.0);
        let fail = EvalOutcome::aggregate(vec![CheckResult::fail(
            EvalTier::ExecutionOk,
            0.0,
            true,
            vec![],
            vec![],
        )]);
        assert_eq!(fail.score, 0.0);
    }

    #[test]
    fn no_required_checks_aggregates_to_pass() {
        // Only soft checks → verdict Pass (nothing gates), score from weights.
        let outcome = EvalOutcome::aggregate(vec![CheckResult::fail(
            EvalTier::Judge,
            1.0,
            false,
            vec!["soft gap".into()],
            vec![],
        )]);
        assert_eq!(outcome.verdict, Verdict::Pass);
        assert_eq!(outcome.score, 0.0);
    }

    #[test]
    fn vacuous_pass_is_passing_and_empty() {
        let v = EvalOutcome::vacuous_pass();
        assert!(v.is_passing());
        assert!(v.per_check.is_empty());
        assert!(v.critique.is_empty());
    }

    #[test]
    fn outcome_round_trips_through_json() {
        let outcome =
            EvalOutcome::aggregate(vec![CheckResult::error(EvalTier::Judge, 1.0, true, "boom")]);
        let json = serde_json::to_string(&outcome).unwrap();
        let back: EvalOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(back, outcome);
    }
}
