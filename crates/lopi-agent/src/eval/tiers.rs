//! Eval-Execution-1 (A1) — the deterministic + suite tier evaluators.
//!
//! [`ExecutionOkEval`] and [`ShellTestEval`] are the cheap, objective,
//! un-gameable floor; [`SuiteEval`] wraps a named quality suite (KCQF). All
//! three are fail-closed: an error yields [`Verdict::Error`](lopi_core::Verdict::Error), never a pass.

use super::{EvalContext, TierEvaluator};
use async_trait::async_trait;
use lopi_core::acceptance::{AcceptanceCheck, CheckSpec, EvalTier};
use lopi_core::loop_config::run_guard_command;
use lopi_core::CheckResult;

/// Failure markers a test/lint transcript is scanned for when no precomputed
/// execution signal is available (the offline regression fixtures). Case
/// matters — these are the exact tokens cargo/npm/rustc emit on failure.
const FAILURE_MARKERS: &[&str] = &[
    "test result: FAILED",
    "FAILED",
    "error[",
    "error:",
    "panicked",
    "failures:",
];

/// Whether a test/lint transcript shows a failure. Deterministic and pure so
/// the objective floor is unit-testable without a live runner.
#[must_use]
pub fn output_shows_failure(output: &str) -> bool {
    FAILURE_MARKERS.iter().any(|m| output.contains(m))
}

/// Tier 0 — the code builds/tests/lints clean. Honors the live `Scorer`'s
/// precomputed signal when present ([`EvalContext::execution_ok`]); otherwise
/// derives the verdict deterministically from the recorded test output.
pub struct ExecutionOkEval;

#[async_trait]
impl TierEvaluator for ExecutionOkEval {
    fn tier(&self) -> EvalTier {
        EvalTier::ExecutionOk
    }

    async fn evaluate(&self, ctx: &EvalContext, check: &AcceptanceCheck) -> CheckResult {
        let ok = ctx
            .execution_ok
            .unwrap_or_else(|| !output_shows_failure(&ctx.test_output));
        if ok {
            CheckResult::pass(EvalTier::ExecutionOk, check.weight, check.required)
        } else {
            CheckResult::fail(
                EvalTier::ExecutionOk,
                check.weight,
                check.required,
                vec!["execution/tests/lint did not pass cleanly".into()],
                vec!["make the build, tests, and linter all pass before re-scoring".into()],
            )
        }
    }
}

/// Tier 1 — a shell command exits `0`. Reuses `run_guard_command` (the same
/// `sh -c` guardrail primitive `gate`/`until` use). Fail-closed: a shell that
/// cannot be spawned, or a context that forbids IO, yields [`Verdict::Error`](lopi_core::Verdict::Error).
pub struct ShellTestEval;

#[async_trait]
impl TierEvaluator for ShellTestEval {
    fn tier(&self) -> EvalTier {
        EvalTier::ShellTest
    }

    async fn evaluate(&self, ctx: &EvalContext, check: &AcceptanceCheck) -> CheckResult {
        let CheckSpec::Shell { cmd } = &check.spec else {
            return CheckResult::error(
                EvalTier::ShellTest,
                check.weight,
                check.required,
                "shell tier received a non-shell check spec",
            );
        };
        if !ctx.live {
            return CheckResult::error(
                EvalTier::ShellTest,
                check.weight,
                check.required,
                "shell check cannot run in an offline context",
            );
        }
        match run_guard_command(cmd, &ctx.repo_path).await {
            Ok(true) => CheckResult::pass(EvalTier::ShellTest, check.weight, check.required),
            Ok(false) => CheckResult::fail(
                EvalTier::ShellTest,
                check.weight,
                check.required,
                vec![format!("shell check failed: `{cmd}` exited non-zero")],
                vec![format!("make `{cmd}` exit 0")],
            ),
            Err(e) => CheckResult::error(
                EvalTier::ShellTest,
                check.weight,
                check.required,
                format!("shell check `{cmd}` could not be spawned: {e}"),
            ),
        }
    }
}

/// Tier 3 — a named quality suite (KCQF). A thin v1 wrapper: it runs the suite
/// as a shell invocation in the repo (`konjo <name>` by convention) and gates
/// on exit `0`. Fail-closed like the shell tier.
pub struct SuiteEval;

#[async_trait]
impl TierEvaluator for SuiteEval {
    fn tier(&self) -> EvalTier {
        EvalTier::Suite
    }

    async fn evaluate(&self, ctx: &EvalContext, check: &AcceptanceCheck) -> CheckResult {
        let CheckSpec::Suite { name } = &check.spec else {
            return CheckResult::error(
                EvalTier::Suite,
                check.weight,
                check.required,
                "suite tier received a non-suite check spec",
            );
        };
        if !ctx.live {
            return CheckResult::error(
                EvalTier::Suite,
                check.weight,
                check.required,
                "suite check cannot run in an offline context",
            );
        }
        let cmd = format!("konjo {name}");
        match run_guard_command(&cmd, &ctx.repo_path).await {
            Ok(true) => CheckResult::pass(EvalTier::Suite, check.weight, check.required),
            Ok(false) => CheckResult::fail(
                EvalTier::Suite,
                check.weight,
                check.required,
                vec![format!("quality suite `{name}` did not pass")],
                vec![format!("resolve the `{name}` suite failures")],
            ),
            Err(e) => CheckResult::error(
                EvalTier::Suite,
                check.weight,
                check.required,
                format!("suite `{name}` could not be spawned: {e}"),
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use lopi_core::acceptance::CheckSpec;
    use lopi_core::Verdict;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn offline_ctx(test_output: &str, execution_ok: Option<bool>) -> EvalContext {
        EvalContext {
            goal: "g".into(),
            diff: String::new(),
            test_output: test_output.into(),
            repo_path: PathBuf::from("."),
            execution_ok,
            metrics: BTreeMap::new(),
            live: false,
        }
    }

    #[test]
    fn failure_markers_detected() {
        assert!(output_shows_failure(
            "test result: FAILED. 1 passed; 1 failed"
        ));
        assert!(output_shows_failure("error[E0369]: no impl"));
        assert!(output_shows_failure("thread 'x' panicked at ..."));
        assert!(!output_shows_failure("test result: ok. 3 passed; 0 failed"));
        assert!(!output_shows_failure(""));
    }

    #[tokio::test]
    async fn execution_ok_uses_precomputed_signal_first() {
        let e = ExecutionOkEval;
        let check = AcceptanceCheck::new(CheckSpec::ExecutionOk);
        // Precomputed true wins even if the (empty) output would also pass.
        let r = e.evaluate(&offline_ctx("", Some(true)), &check).await;
        assert_eq!(r.verdict, Verdict::Pass);
        // Precomputed false fails even though output has no failure marker.
        let r = e
            .evaluate(&offline_ctx("all good", Some(false)), &check)
            .await;
        assert_eq!(r.verdict, Verdict::Fail);
        assert!(!r.fix_hints.is_empty());
    }

    #[tokio::test]
    async fn execution_ok_derives_from_output_when_no_signal() {
        let e = ExecutionOkEval;
        let check = AcceptanceCheck::new(CheckSpec::ExecutionOk);
        let pass = e
            .evaluate(&offline_ctx("test result: ok. 2 passed", None), &check)
            .await;
        assert_eq!(pass.verdict, Verdict::Pass);
        let fail = e
            .evaluate(&offline_ctx("test result: FAILED", None), &check)
            .await;
        assert_eq!(fail.verdict, Verdict::Fail);
    }

    #[tokio::test]
    async fn shell_tier_errors_when_offline() {
        let e = ShellTestEval;
        let check = AcceptanceCheck::new(CheckSpec::Shell { cmd: "true".into() });
        let r = e.evaluate(&offline_ctx("", None), &check).await;
        assert_eq!(r.verdict, Verdict::Error);
    }

    #[tokio::test]
    async fn shell_tier_passes_and_fails_live() {
        let e = ShellTestEval;
        let mut ctx = offline_ctx("", None);
        ctx.live = true;
        let pass = e
            .evaluate(
                &ctx,
                &AcceptanceCheck::new(CheckSpec::Shell { cmd: "true".into() }),
            )
            .await;
        assert_eq!(pass.verdict, Verdict::Pass);
        let fail = e
            .evaluate(
                &ctx,
                &AcceptanceCheck::new(CheckSpec::Shell {
                    cmd: "exit 1".into(),
                }),
            )
            .await;
        assert_eq!(fail.verdict, Verdict::Fail);
    }

    #[tokio::test]
    async fn suite_tier_errors_when_offline() {
        let e = SuiteEval;
        let check = AcceptanceCheck::new(CheckSpec::Suite {
            name: "kcqf".into(),
        });
        let r = e.evaluate(&offline_ctx("", None), &check).await;
        assert_eq!(r.verdict, Verdict::Error);
    }

    #[tokio::test]
    async fn wrong_spec_is_fail_closed() {
        // A shell evaluator handed an execution-ok spec must Error, not pass.
        let r = ShellTestEval
            .evaluate(
                &offline_ctx("", None),
                &AcceptanceCheck::new(CheckSpec::ExecutionOk),
            )
            .await;
        assert_eq!(r.verdict, Verdict::Error);
    }
}
