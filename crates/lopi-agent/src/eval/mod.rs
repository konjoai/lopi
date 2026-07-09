//! Eval-Execution-1 (A1) — the tiered eval executor (cross-cutting seam #2).
//!
//! Promotes the working Konjo Verifier from a finalize-gate double-check into a
//! tiered executor that scores a loop against an explicit
//! [`Acceptance`](lopi_core::Acceptance) goal object. Four tiers sit behind one
//! pluggable [`TierEvaluator`] interface:
//!
//! | Tier | Impl | Kind |
//! |------|------|------|
//! | `ExecutionOk` | [`ExecutionOkEval`] | deterministic |
//! | `ShellTest`   | [`ShellTestEval`]   | deterministic |
//! | `Judge`       | [`JudgeEval`]       | separate-model judgment |
//! | `Suite`       | [`SuiteEval`]       | quality suite (KCQF) |
//!
//! (tiers are [`lopi_core::EvalTier`] variants.) The [`TieredEvaluator`] runs an
//! acceptance's checks in tier order and short-circuits on the first *required*
//! failure at a cheap tier before paying for the judge — the
//! objective-to-deterministic routing rule. Every tier is **fail-closed**: an
//! error yields [`Verdict::Error`](lopi_core::Verdict::Error), never a silent pass.

mod judge;
mod tiers;

pub use judge::{ErroringJudge, Judge, JudgeEval, VerifierJudge};
pub use tiers::{output_shows_failure, ExecutionOkEval, ShellTestEval, SuiteEval};

use async_trait::async_trait;
use lopi_core::acceptance::{Acceptance, AcceptanceCheck, CheckSpec, EvalTier};
use lopi_core::{CheckResult, EvalOutcome, Verdict};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The complete signal handed to every tier evaluator.
///
/// Input-completeness (probe caveat #1) lives here: the judge grades blind if
/// this is truncated, so the **full** diff and raw metric readings are passed,
/// not a 6 KB prefix or a summary. Where a criterion is objective, prefer a
/// deterministic tier / [`MetricGate`](lopi_core::MetricGate) so gaming that is
/// visible in these inputs can't be argued around.
#[derive(Debug, Clone)]
pub struct EvalContext {
    /// The loop's natural-language goal.
    pub goal: String,
    /// The **full** uncommitted diff (never truncated for the deterministic
    /// tiers; the judge tier applies its own bound but is handed the whole).
    pub diff: String,
    /// The full test + lint output produced this attempt.
    pub test_output: String,
    /// Repo root — where shell/suite tiers run.
    pub repo_path: PathBuf,
    /// Precomputed "builds/tests/lint clean" signal from the live `Scorer`.
    /// `None` (e.g. the regression fixtures) makes [`ExecutionOkEval`] derive
    /// the verdict from `test_output` itself.
    pub execution_ok: Option<bool>,
    /// Raw metric readings a [`MetricGate`](lopi_core::MetricGate) is checked
    /// against (e.g. `{"coverage": 0.83}`).
    pub metrics: BTreeMap<String, f64>,
    /// Whether shelling out / calling the model is permitted. `false` in the
    /// offline regression suite: a tier that needs IO fails closed instead.
    pub live: bool,
}

impl EvalContext {
    /// A live context rooted at `repo_path` with a precomputed execution signal.
    #[must_use]
    pub fn live(
        goal: impl Into<String>,
        diff: impl Into<String>,
        test_output: impl Into<String>,
        repo_path: PathBuf,
        execution_ok: bool,
    ) -> Self {
        Self {
            goal: goal.into(),
            diff: diff.into(),
            test_output: test_output.into(),
            repo_path,
            execution_ok: Some(execution_ok),
            metrics: BTreeMap::new(),
            live: true,
        }
    }

    /// Attach metric readings (builder).
    #[must_use]
    pub fn with_metrics(mut self, metrics: BTreeMap<String, f64>) -> Self {
        self.metrics = metrics;
        self
    }
}

/// One evaluation tier — the pluggable interface (cross-cutting seam #2).
///
/// Each impl is testable in isolation with a fake, and the judge tier's backend
/// is itself pluggable via [`Judge`] so a fixture or an erroring judge can drive
/// the fail-closed and regression tests without a live API call.
#[async_trait]
pub trait TierEvaluator: Send + Sync {
    /// The tier this evaluator serves.
    fn tier(&self) -> EvalTier;

    /// Evaluate one check against the context, returning a fail-closed result.
    async fn evaluate(&self, ctx: &EvalContext, check: &AcceptanceCheck) -> CheckResult;
}

/// Runs an [`Acceptance`]'s checks in tier order, short-circuiting on the first
/// required failure at a cheap tier before paying for the judge.
pub struct TieredEvaluator {
    execution_ok: ExecutionOkEval,
    shell_test: ShellTestEval,
    judge: JudgeEval,
    suite: SuiteEval,
    /// Test-only invocation counter proving the routing rule (the judge is
    /// never reached once a cheaper required tier has already decided FAIL).
    judge_calls: std::sync::atomic::AtomicUsize,
}

impl TieredEvaluator {
    /// Build a tiered executor from the judge backend used for the [`JudgeEval`]
    /// tier (a [`VerifierJudge`] in production, a fixture/erroring judge in
    /// tests). The deterministic tiers carry no state.
    #[must_use]
    pub fn new(judge: Box<dyn Judge>) -> Self {
        Self {
            execution_ok: ExecutionOkEval,
            shell_test: ShellTestEval,
            judge: JudgeEval::new(judge),
            suite: SuiteEval,
            judge_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Dispatch one check to its tier's evaluator.
    async fn evaluate_check(&self, ctx: &EvalContext, check: &AcceptanceCheck) -> CheckResult {
        match check.spec {
            CheckSpec::ExecutionOk => self.execution_ok.evaluate(ctx, check).await,
            CheckSpec::Shell { .. } => self.shell_test.evaluate(ctx, check).await,
            CheckSpec::Judge { .. } => {
                self.judge_calls
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.judge.evaluate(ctx, check).await
            }
            CheckSpec::Suite { .. } => self.suite.evaluate(ctx, check).await,
        }
    }

    /// Score `acceptance` against `ctx`, producing the one [`EvalOutcome`].
    ///
    /// Checks run cheapest-tier-first. As soon as a **required** check fails or
    /// errors, evaluation short-circuits: no more expensive tier runs, so an
    /// objective failure the deterministic floor already caught never spends a
    /// judge call. Non-required checks never short-circuit — they always run to
    /// feed the score and critique.
    pub async fn evaluate(&self, ctx: &EvalContext, acceptance: &Acceptance) -> EvalOutcome {
        if acceptance.is_empty() {
            return EvalOutcome::vacuous_pass();
        }
        let mut results = Vec::new();
        for check in acceptance.ordered() {
            let result = self.evaluate_check(ctx, &check).await;
            let short_circuit = check.required && !result.verdict.is_passing();
            results.push(result);
            if short_circuit {
                break;
            }
        }
        EvalOutcome::aggregate(results)
    }

    /// How many times the judge tier was invoked — proves the routing rule in
    /// tests (an objective failure decided by a cheaper tier leaves this at 0).
    #[must_use]
    pub fn judge_call_count(&self) -> usize {
        self.judge_calls.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Whether a [`Verdict`] should let the loop proceed to finalize. The single
/// call site's fail-closed check — only [`Verdict::Pass`] proceeds.
#[must_use]
pub fn verdict_allows_finalize(verdict: Verdict) -> bool {
    verdict.is_passing()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use lopi_core::acceptance::CheckSpec;
    use lopi_core::Rubric;

    fn ctx(execution_ok: bool, test_output: &str) -> EvalContext {
        EvalContext {
            goal: "g".into(),
            diff: "diff".into(),
            test_output: test_output.into(),
            repo_path: PathBuf::from("."),
            execution_ok: Some(execution_ok),
            metrics: BTreeMap::new(),
            live: false,
        }
    }

    fn judge_check() -> AcceptanceCheck {
        AcceptanceCheck::new(CheckSpec::Judge {
            rubric: Rubric::default(),
            metric: None,
        })
    }

    #[tokio::test]
    async fn empty_acceptance_is_a_vacuous_pass() {
        let ev = TieredEvaluator::new(Box::new(ErroringJudge::new("unused")));
        let outcome = ev.evaluate(&ctx(true, ""), &Acceptance::empty()).await;
        assert!(outcome.is_passing());
        assert_eq!(ev.judge_call_count(), 0);
    }

    #[tokio::test]
    async fn required_cheap_failure_short_circuits_before_the_judge() {
        // execution_ok FAIL at the cheapest tier must stop before the judge —
        // the objective-to-deterministic routing rule.
        let ev = TieredEvaluator::new(Box::new(ErroringJudge::new("must not run")));
        let acc = Acceptance::new(vec![
            AcceptanceCheck::new(CheckSpec::ExecutionOk),
            judge_check(),
        ]);
        let outcome = ev.evaluate(&ctx(false, "test result: FAILED"), &acc).await;
        assert_eq!(outcome.verdict, Verdict::Fail);
        assert_eq!(ev.judge_call_count(), 0, "judge must not be reached");
    }

    #[tokio::test]
    async fn passing_cheap_tier_still_reaches_the_judge() {
        let ev = TieredEvaluator::new(Box::new(ErroringJudge::new("boom")));
        let acc = Acceptance::new(vec![
            AcceptanceCheck::new(CheckSpec::ExecutionOk),
            judge_check(),
        ]);
        // execution ok passes → judge runs → judge errors → fail-closed Error.
        let outcome = ev.evaluate(&ctx(true, "ok"), &acc).await;
        assert_eq!(ev.judge_call_count(), 1);
        assert_eq!(outcome.verdict, Verdict::Error);
        assert!(!outcome.is_passing());
    }

    #[tokio::test]
    async fn non_required_failure_does_not_short_circuit() {
        let ev = TieredEvaluator::new(Box::new(ErroringJudge::new("x")));
        let soft_exec = AcceptanceCheck::new(CheckSpec::ExecutionOk).soft();
        let acc = Acceptance::new(vec![soft_exec, judge_check()]);
        let outcome = ev.evaluate(&ctx(false, "FAILED"), &acc).await;
        // Soft exec failed but did not stop the judge from running.
        assert_eq!(ev.judge_call_count(), 1);
        assert_eq!(outcome.verdict, Verdict::Error);
    }

    #[test]
    fn verdict_allows_finalize_only_on_pass() {
        assert!(verdict_allows_finalize(Verdict::Pass));
        assert!(!verdict_allows_finalize(Verdict::Fail));
        assert!(!verdict_allows_finalize(Verdict::Error));
    }
}
