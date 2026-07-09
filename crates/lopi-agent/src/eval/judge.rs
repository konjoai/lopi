//! Eval-Execution-1 (A1) — the judge tier (cross-cutting seam #2, judge half).
//!
//! [`JudgeEval`] is the tier-2 evaluator. It does **not** reimplement judging —
//! it delegates to a pluggable [`Judge`], whose production impl [`VerifierJudge`]
//! wraps the existing, probe-validated [`VerifierAgent`](crate::verifier) verbatim
//! (maker/checker isolation, separate model, `{passed,gaps,fix_hints,confidence}`
//! schema). The trait is what makes the tier testable offline: [`ErroringJudge`]
//! drives the fail-closed test and the regression suite injects a fixture judge.
//!
//! **Fail-closed:** any judge error ⇒ [`Verdict::Error`](lopi_core::Verdict::Error), never a silent pass.
//! **Input-completeness:** the full diff rides in [`EvalContext::diff`]; an
//! objective [`MetricGate`](lopi_core::MetricGate) layered on the judge check is
//! evaluated from raw readings, and a missing reading errors (can't verify ⇒
//! don't pass) rather than being argued around by the model.

use super::{EvalContext, TierEvaluator};
use crate::api_client::AnthropicClient;
use crate::verifier::VerifierAgent;
use async_trait::async_trait;
use lopi_core::acceptance::{AcceptanceCheck, CheckSpec, EvalTier, MetricGate};
use lopi_core::{CheckResult, Rubric, VerifierVerdict};
use std::sync::Arc;

/// The pluggable judgment backend behind [`JudgeEval`].
#[async_trait]
pub trait Judge: Send + Sync {
    /// Grade the context's artifact against `rubric`, returning the verifier
    /// verdict or an error the tier turns into a fail-closed [`Verdict::Error`](lopi_core::Verdict::Error).
    ///
    /// # Errors
    /// Returns `Err` when the underlying model call or its parse fails.
    async fn judge(&self, ctx: &EvalContext, rubric: &Rubric) -> anyhow::Result<VerifierVerdict>;
}

/// Production judge — the existing [`VerifierAgent`] reused verbatim.
pub struct VerifierJudge {
    client: Arc<AnthropicClient>,
    model: String,
    effort: Option<String>,
}

impl VerifierJudge {
    /// Build a judge that grades with `model` (which must differ from the
    /// worker — resolve via `crate::verifier::resolve_verifier`) and an optional
    /// reasoning-effort hint.
    #[must_use]
    pub fn new(client: Arc<AnthropicClient>, model: String, effort: Option<String>) -> Self {
        Self {
            client,
            model,
            effort,
        }
    }
}

#[async_trait]
impl Judge for VerifierJudge {
    async fn judge(&self, ctx: &EvalContext, rubric: &Rubric) -> anyhow::Result<VerifierVerdict> {
        // Reuse the verifier verbatim: isolated maker/checker, no plan leaked.
        // The full diff is handed in (input-completeness); the verifier applies
        // its own documented 6 KB bound internally — the honest judgment ceiling.
        VerifierAgent::new(self.client.clone())
            .verify(
                &ctx.goal,
                "",
                &ctx.diff,
                &ctx.test_output,
                rubric,
                &self.model,
                self.effort.as_deref(),
            )
            .await
    }
}

/// A judge that always errors — drives the fail-closed test and stands in for a
/// down API in offline tests.
pub struct ErroringJudge {
    reason: String,
}

impl ErroringJudge {
    /// Build an erroring judge carrying `reason`.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl Judge for ErroringJudge {
    async fn judge(&self, _ctx: &EvalContext, _rubric: &Rubric) -> anyhow::Result<VerifierVerdict> {
        Err(anyhow::anyhow!(self.reason.clone()))
    }
}

/// Tier 2 — the judge tier, delegating to a [`Judge`] and applying any objective
/// [`MetricGate`] on top of the judgment.
pub struct JudgeEval {
    judge: Box<dyn Judge>,
}

impl JudgeEval {
    /// Wrap a judge backend.
    #[must_use]
    pub fn new(judge: Box<dyn Judge>) -> Self {
        Self { judge }
    }
}

#[async_trait]
impl TierEvaluator for JudgeEval {
    fn tier(&self) -> EvalTier {
        EvalTier::Judge
    }

    async fn evaluate(&self, ctx: &EvalContext, check: &AcceptanceCheck) -> CheckResult {
        let CheckSpec::Judge { rubric, metric } = &check.spec else {
            return CheckResult::error(
                EvalTier::Judge,
                check.weight,
                check.required,
                "judge tier received a non-judge check spec",
            );
        };
        // The metric gate is objective — check it first so a failing metric
        // can't be argued around, and a missing reading fails closed before we
        // even spend a judge call.
        if let Some(gate) = metric {
            if let Some(gated) = gate_result(gate, ctx, check) {
                return gated;
            }
        }
        match self.judge.judge(ctx, rubric).await {
            Ok(verdict) => judge_verdict_result(&verdict, check),
            Err(e) => CheckResult::error(
                EvalTier::Judge,
                check.weight,
                check.required,
                format!("judge error: {e}"),
            ),
        }
    }
}

/// Apply a metric gate against the context's readings. Returns `Some(fail/error)`
/// when the gate is not satisfied (short-circuiting the judge), or `None` when
/// the gate passes and the judge should still run.
fn gate_result(
    gate: &MetricGate,
    ctx: &EvalContext,
    check: &AcceptanceCheck,
) -> Option<CheckResult> {
    match ctx.metrics.get(&gate.name) {
        None => Some(CheckResult::error(
            EvalTier::Judge,
            check.weight,
            check.required,
            format!(
                "metric `{}` has no reading — cannot verify the gate",
                gate.name
            ),
        )),
        Some(&reading) if gate.satisfied_by(reading) => None,
        Some(&reading) => Some(CheckResult::fail(
            EvalTier::Judge,
            check.weight,
            check.required,
            vec![format!(
                "metric `{}` = {reading} failed the gate ({:?} {})",
                gate.name, gate.op, gate.threshold
            )],
            vec![format!("raise `{}` to satisfy the gate", gate.name)],
        )),
    }
}

/// Map a verifier verdict onto a judge-tier [`CheckResult`].
fn judge_verdict_result(verdict: &VerifierVerdict, check: &AcceptanceCheck) -> CheckResult {
    let confidence = verdict.confidence as f32;
    if verdict.passed {
        CheckResult {
            confidence,
            ..CheckResult::pass(EvalTier::Judge, check.weight, check.required)
        }
    } else {
        CheckResult {
            confidence,
            ..CheckResult::fail(
                EvalTier::Judge,
                check.weight,
                check.required,
                verdict.gaps.clone(),
                verdict.fix_hints.clone(),
            )
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use lopi_core::acceptance::Op;
    use lopi_core::Verdict;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// A judge that returns a fixed recorded verdict — used by the regression
    /// suite and these unit tests to drive the tier offline.
    struct FixedJudge(VerifierVerdict);

    #[async_trait]
    impl Judge for FixedJudge {
        async fn judge(
            &self,
            _ctx: &EvalContext,
            _rubric: &Rubric,
        ) -> anyhow::Result<VerifierVerdict> {
            Ok(self.0.clone())
        }
    }

    fn ctx_with(metrics: BTreeMap<String, f64>) -> EvalContext {
        EvalContext {
            goal: "g".into(),
            diff: "d".into(),
            test_output: "t".into(),
            repo_path: PathBuf::from("."),
            execution_ok: Some(true),
            metrics,
            live: false,
        }
    }

    fn judge_check(metric: Option<MetricGate>) -> AcceptanceCheck {
        AcceptanceCheck::new(CheckSpec::Judge {
            rubric: Rubric::default(),
            metric,
        })
    }

    fn passed_verdict() -> VerifierVerdict {
        VerifierVerdict {
            passed: true,
            gaps: vec![],
            fix_hints: vec![],
            confidence: 0.9,
        }
    }

    #[tokio::test]
    async fn judge_error_is_fail_closed() {
        let e = JudgeEval::new(Box::new(ErroringJudge::new("api 500")));
        let r = e
            .evaluate(&ctx_with(BTreeMap::new()), &judge_check(None))
            .await;
        assert_eq!(r.verdict, Verdict::Error);
        assert!(!r.verdict.is_passing());
    }

    #[tokio::test]
    async fn judge_pass_maps_to_pass() {
        let e = JudgeEval::new(Box::new(FixedJudge(passed_verdict())));
        let r = e
            .evaluate(&ctx_with(BTreeMap::new()), &judge_check(None))
            .await;
        assert_eq!(r.verdict, Verdict::Pass);
        assert_eq!(r.confidence, 0.9);
    }

    #[tokio::test]
    async fn judge_fail_carries_gaps_and_hints() {
        let v = VerifierVerdict {
            passed: false,
            gaps: vec!["cherry-picked best-of-5".into()],
            fix_hints: vec!["report all 5 runs".into()],
            confidence: 0.8,
        };
        let e = JudgeEval::new(Box::new(FixedJudge(v)));
        let r = e
            .evaluate(&ctx_with(BTreeMap::new()), &judge_check(None))
            .await;
        assert_eq!(r.verdict, Verdict::Fail);
        assert_eq!(r.gaps, vec!["cherry-picked best-of-5".to_string()]);
        assert_eq!(r.fix_hints, vec!["report all 5 runs".to_string()]);
    }

    #[tokio::test]
    async fn metric_gate_missing_reading_is_fail_closed() {
        let gate = MetricGate {
            name: "coverage".into(),
            op: Op::Gte,
            threshold: 0.8,
        };
        let e = JudgeEval::new(Box::new(FixedJudge(passed_verdict())));
        let r = e
            .evaluate(&ctx_with(BTreeMap::new()), &judge_check(Some(gate)))
            .await;
        assert_eq!(r.verdict, Verdict::Error, "missing reading can't verify");
    }

    #[tokio::test]
    async fn metric_gate_failing_reading_fails_before_judge() {
        let gate = MetricGate {
            name: "coverage".into(),
            op: Op::Gte,
            threshold: 0.8,
        };
        let mut m = BTreeMap::new();
        m.insert("coverage".to_string(), 0.5);
        let e = JudgeEval::new(Box::new(FixedJudge(passed_verdict())));
        let r = e.evaluate(&ctx_with(m), &judge_check(Some(gate))).await;
        assert_eq!(r.verdict, Verdict::Fail);
    }

    #[tokio::test]
    async fn metric_gate_passing_reading_lets_judge_decide() {
        let gate = MetricGate {
            name: "coverage".into(),
            op: Op::Gte,
            threshold: 0.8,
        };
        let mut m = BTreeMap::new();
        m.insert("coverage".to_string(), 0.95);
        let e = JudgeEval::new(Box::new(FixedJudge(passed_verdict())));
        let r = e.evaluate(&ctx_with(m), &judge_check(Some(gate))).await;
        assert_eq!(r.verdict, Verdict::Pass);
    }
}
