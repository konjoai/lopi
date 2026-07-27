//! Eval-Execution-1 (A1) — hook the tiered eval executor into the run loop.
//!
//! After a loop run passes the heuristic scorer, [`AgentRunner::finalize`] scores
//! it against its explicit [`Acceptance`](lopi_core::Acceptance) goal (when one
//! is set) *before* the autonomy-level verifier gate. The evaluation is
//! **fail-closed**: a non-passing [`EvalOutcome`] routes its critique into the
//! next attempt's constraints and rejects the finalize, exactly like the
//! verifier path. It is **additive** — a task with no acceptance is untouched,
//! and the existing verifier critique-routing still fires — and it **persists**
//! the outcome plus a score-history entry (cross-cutting seam #4).

use super::AgentRunner;
use crate::claude::select_model;
use crate::eval::{CliVerifierJudge, EvalContext, Judge, TieredEvaluator, VerifierJudge};
use crate::verifier::{get_repo_diff, resolve_verifier};
use lopi_core::Score;
use tracing::warn;

impl AgentRunner {
    /// Score a passing attempt against its [`Acceptance`](lopi_core::Acceptance).
    ///
    /// Returns `true` when the runner may proceed to the verifier/commit path —
    /// either because no acceptance is set (backward-compatible) or because the
    /// tiered outcome passed. Returns `false` when the outcome is not passing
    /// (fail-closed); the caller rolls back and retries. On `false`, the
    /// outcome's critique has already been appended to `self.task.constraints`
    /// for the next attempt, and the outcome + score-history entry are persisted.
    pub(super) async fn evaluate_acceptance_gate(&mut self, score: &Score, attempt: u8) -> bool {
        let Some(acceptance) = self.task.acceptance.clone() else {
            return true;
        };
        if acceptance.is_empty() {
            return true;
        }

        let ctx = self.build_eval_context(score).await;
        let judge = self.build_judge(attempt);
        let outcome = TieredEvaluator::new(judge)
            .evaluate(&ctx, &acceptance)
            .await;

        self.log(format!(
            "🎯 eval: verdict={} score={:.3} ({} check(s))",
            outcome.verdict.as_str(),
            outcome.score,
            outcome.per_check.len(),
        ));

        if let Some(store) = &self.store {
            if let Err(e) = store
                .save_eval_outcome(&self.task.id.to_string(), attempt, &outcome)
                .await
            {
                warn!("eval outcome persist failed: {e}");
            }
        }

        if outcome.is_passing() {
            return true;
        }

        // Fail-closed: route the critique into the next attempt, exactly like
        // the verifier's fix-hint injection, and reject the finalize.
        self.log(format!(
            "🎯 eval rejected — {} critique item(s); appending for next attempt",
            outcome.critique.len(),
        ));
        for item in &outcome.critique {
            if !self.task.constraints.contains(item) {
                self.task.constraints.push(item.clone());
            }
        }
        // A2 (reflection) — capture the durable learning *now*, before the
        // caller (`finalize`) rolls the attempt back. A rejected attempt still
        // yields its lesson: you learned what does not work. No-op unless
        // cross-run reflection is enabled.
        self.capture_learning(&outcome.critique, "eval_rejected")
            .await;
        false
    }

    /// Assemble the full evaluation signal (input-completeness): the **full**
    /// diff (not a truncated prefix), the scorer's test/lint output, and the
    /// precomputed execution-ok signal.
    async fn build_eval_context(&self, score: &Score) -> EvalContext {
        let diff = get_repo_diff(&self.repo_path).await;
        let test_output = score.errors.join("\n");
        EvalContext::live(
            self.task.goal.clone(),
            diff,
            test_output,
            self.repo_path.clone(),
            score.passed(),
            !lopi_core::is_untrusted_source(&self.task.source),
        )
    }

    /// Build the judge backend for the judge tier. Reuses the verifier's
    /// model-resolution ("never grade your own homework") and its Sprint F1
    /// backend selection: an `AnthropicClient` when one is configured, the
    /// `claude` CLI otherwise — which, before this sprint, was every
    /// production deployment (`with_api` never wired), and is why this used
    /// to fall back to an always-failing `ErroringJudge` instead of a
    /// working default. See `LEDGER.md`.
    fn build_judge(&self, attempt: u8) -> Box<dyn Judge> {
        let worker_model = select_model(&self.task, attempt.saturating_sub(1));
        let (model, effort) = resolve_verifier(
            &worker_model,
            self.task.verifier_model.as_deref(),
            self.task.verifier_effort.as_deref(),
        );
        match self.api_client.clone() {
            Some(client) => Box::new(VerifierJudge::new(client, model, effort)),
            None => Box::new(CliVerifierJudge::new(self.repo_path.clone(), model, effort)),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopi_core::acceptance::{Acceptance, AcceptanceCheck, CheckSpec};
    use lopi_core::{Score, Task};
    use std::path::PathBuf;

    #[tokio::test]
    async fn no_acceptance_proceeds_unchanged() {
        let task = Task::new("no goal object");
        let (mut runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));
        let score = Score::new(1.0, 0, 10);
        assert!(
            runner.evaluate_acceptance_gate(&score, 1).await,
            "a task with no acceptance must proceed exactly as before"
        );
    }

    #[tokio::test]
    async fn empty_acceptance_proceeds() {
        let mut task = Task::new("empty goal object");
        task.acceptance = Some(Acceptance::empty());
        let (mut runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));
        let score = Score::new(1.0, 0, 10);
        assert!(runner.evaluate_acceptance_gate(&score, 1).await);
    }

    #[tokio::test]
    async fn passing_execution_ok_acceptance_proceeds() {
        // A deterministic execution-ok check with a clean score passes without
        // needing an API client.
        let mut task = Task::new("deterministic goal");
        task.acceptance = Some(Acceptance::new(vec![AcceptanceCheck::new(
            CheckSpec::ExecutionOk,
        )]));
        let (mut runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));
        let score = Score::new(1.0, 0, 10); // passed() == true
        assert!(runner.evaluate_acceptance_gate(&score, 1).await);
    }

    /// Sprint F1 Phase 3 — before this sprint, "no API client" meant the
    /// judge tier could never run at all (`.with_api()` production-unwired),
    /// so this used to be documented as "must proceed, not retry" — the
    /// least-bad option when the only alternative was retrying to
    /// exhaustion against a guaranteed-failing `ErroringJudge`. Now that a
    /// CLI backend exists, "no client" is no longer a wiring gap: a real
    /// grading attempt is made (via `build_judge`'s CLI branch) and its
    /// outcome — pass, fail, or CLI-error — is persisted like any other
    /// evaluated attempt, exactly like `run_verifier_pass`'s
    /// `verifier_verdicts` row. This asserts the attempt happened (a row
    /// exists) rather than asserting its pass/fail content, so the test
    /// stays deterministic whether or not a live `claude` binary/subscription
    /// is available in the environment running it.
    #[tokio::test]
    async fn judge_acceptance_without_api_client_now_attempts_the_cli_backend() {
        let mut task = Task::new("needs a judge");
        task.acceptance = Some(Acceptance::new(vec![AcceptanceCheck::new(
            CheckSpec::Judge {
                rubric: lopi_core::Rubric::default(),
                metric: None,
            },
        )]));
        let task_id = task.id.to_string();
        let store = lopi_memory::MemoryStore::open_in_memory()
            .await
            .expect("open in-memory store");
        let (mut runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));
        runner.store = Some(store.clone());
        let score = Score::new(1.0, 0, 10);

        runner.evaluate_acceptance_gate(&score, 1).await;

        let outcomes = store
            .load_eval_outcomes(&task_id)
            .await
            .expect("load eval outcomes");
        assert!(
            !outcomes.is_empty(),
            "no API client must still ATTEMPT a real judge-tier evaluation (via the CLI \
             backend) and persist its outcome, not silently skip the gate"
        );
    }
}
