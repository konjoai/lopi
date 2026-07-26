//! Sprint S — Konjo Verifier integration inside the agent runner.
//! Verifier as Explicit Gate — the model/effort actually used to grade is
//! resolved from the task's `verifier_model` / `verifier_effort` (falling
//! back to a model that differs from the worker's) instead of the old
//! hardcoded Opus constant.
use super::AgentRunner;
use crate::claude::select_model;
use crate::verifier::{get_repo_diff, resolve_rubric, resolve_verifier, VerifierAgent};
use lopi_core::AgentEvent;
use tracing::warn;

impl AgentRunner {
    /// Run the Konjo Verifier second-score pass.
    ///
    /// Returns `true` when the runner should proceed to commit (verifier
    /// passed). Returns `false` when the verifier rejected the output, or
    /// could not evaluate it at all (Sprint F1 Phase 4 — requested-but-
    /// unavailable is now fail-closed, same as a configured backend that
    /// errors; see `handle_verifier_error` and `LEDGER.md`); the caller
    /// must roll back and retry. Fix hints are already appended to
    /// `self.task.constraints` when `false` is returned.
    ///
    /// Backend selection (Sprint F1 Phase 1) — not a config flag: an
    /// `AnthropicClient` when one is configured (`with_api`, currently never
    /// wired in production), the `claude` CLI otherwise. The CLI path is the
    /// default because, before this sprint, it was the only path that could
    /// ever actually run.
    pub(super) async fn run_verifier_pass(&mut self, attempt: u8, test_errors: &[String]) -> bool {
        let verifier = match self.api_client.clone() {
            Some(client) => VerifierAgent::new(client),
            None => VerifierAgent::new_cli(self.repo_path.clone()),
        };
        let plan = self.last_plan.clone().unwrap_or_default();
        let rubric = resolve_rubric(self.task.rubric.clone(), &self.repo_path).await;
        let diff = get_repo_diff(&self.repo_path).await;
        let test_output = test_errors.join("\n");

        // `attempt` here is the 1-based finalize attempt; `select_model` wants
        // the 0-based attempt whose model this grading pass must not repeat.
        let worker_model = select_model(&self.task, attempt.saturating_sub(1));
        let (model, effort) = resolve_verifier(
            &worker_model,
            self.task.verifier_model.as_deref(),
            self.task.verifier_effort.as_deref(),
        );

        self.log(format!(
            "🔬 verifier: grading output against rubric ({model})…"
        ));
        let verdict = match verifier
            .verify(
                &self.task.goal,
                &plan,
                &diff,
                &test_output,
                &rubric,
                &model,
                effort.as_deref(),
            )
            .await
        {
            Ok(v) => v,
            // Phase 0 (A1) — fail-closed. A gate that could not be evaluated is
            // NOT a pass: a verifier API/parse error blocks finalize (returns
            // `false` ⇒ roll back + retry) unless the operator has explicitly
            // opted this loop into the legacy fail-open behavior.
            Err(e) => return self.handle_verifier_error(attempt, &model, &e).await,
        };

        self.log(format!(
            "🔬 verifier: passed={} confidence={:.0}% gaps={}",
            verdict.passed,
            verdict.confidence * 100.0,
            verdict.gaps.len()
        ));

        persist_and_emit(self, attempt, &verdict, &model).await;

        if verdict.passed {
            return true;
        }

        self.log(format!(
            "🔬 verifier rejected — {} gap(s); appending fix hints for next attempt",
            verdict.gaps.len()
        ));
        for hint in &verdict.fix_hints {
            if !self.task.constraints.contains(hint) {
                self.task.constraints.push(hint.clone());
            }
        }
        false
    }

    /// Fail-closed handling of a verifier API/parse error (Phase 0, A1).
    ///
    /// Records a not-passing ERROR verdict (so the trace and score history show
    /// the gate could not be evaluated, never a silent pass) and returns
    /// whether the runner may still proceed — `true` only when the operator has
    /// opted this loop into fail-open via `task.verifier_fail_open`.
    async fn handle_verifier_error(
        &mut self,
        attempt: u8,
        model: &str,
        err: &anyhow::Error,
    ) -> bool {
        let proceed = verifier_error_proceeds(self.task.verifier_fail_open);
        let verdict = lopi_core::VerifierVerdict {
            passed: false,
            gaps: vec![format!("verifier could not evaluate the output: {err}")],
            fix_hints: vec![
                "the verifier errored; re-run so the output can be graded before finalize".into(),
            ],
            confidence: 0.0,
        };
        persist_and_emit(self, attempt, &verdict, model).await;
        if proceed {
            warn!("verifier error (fail-open opt-in, proceeding): {err}");
            return true;
        }
        warn!("verifier error (fail-closed, blocking finalize): {err}");
        self.log(
            "🔬 verifier errored — fail-closed: blocking finalize and retrying (set verifier_fail_open to override)".to_string(),
        );
        false
    }

    /// Sprint F1 Phase 2 — test/integration seam. Drives exactly the
    /// verifier pass `finalize.rs` runs internally on a passing attempt,
    /// without running the full plan/implement/test loop. Exists so a
    /// regression test built through the real pool-construction seam
    /// (`lopi_orchestrator::pool::run_loop::build_runner`) can prove a
    /// runner with no API client actually **executes** a verifier pass —
    /// asserting on the emitted event or the persisted `verifier_verdicts`
    /// row, not on `verifier_enabled()` — which is exactly the gap that let
    /// the verifier return `true` unconditionally for its entire existence
    /// while every existing test only ever checked the bool.
    pub async fn run_verifier_pass_for_test(&mut self, test_errors: &[String]) -> bool {
        self.run_verifier_pass(1, test_errors).await
    }
}

/// Whether a verifier error should let the loop proceed to commit.
///
/// The fail-closed default (`fail_open == false`) returns `false`: a gate that
/// could not be evaluated is treated as not-passing, so an unverifiable change
/// never lands. Only an explicit operator opt-in (`fail_open == true`) restores
/// the legacy "proceed on error" behavior.
#[must_use]
pub fn verifier_error_proceeds(fail_open: bool) -> bool {
    fail_open
}

async fn persist_and_emit(
    runner: &AgentRunner,
    attempt: u8,
    verdict: &lopi_core::VerifierVerdict,
    model: &str,
) {
    if let Some(store) = &runner.store {
        if let Err(e) = store
            .save_verifier_verdict(&runner.task.id.to_string(), attempt, verdict, model)
            .await
        {
            warn!("verifier verdict persist failed: {e}");
        }
    }
    runner.bus.send(AgentEvent::VerifierVerdict {
        task_id: runner.id(),
        passed: verdict.passed,
        gaps: verdict.gaps.clone(),
        fix_hints: verdict.fix_hints.clone(),
        confidence: verdict.confidence,
    });
}

#[cfg(test)]
mod tests {
    use super::{verifier_error_proceeds, AgentRunner};

    #[test]
    fn verifier_error_is_fail_closed_by_default() {
        // The one thing an evaluator can't do: pass when it errors.
        assert!(
            !verifier_error_proceeds(false),
            "a verifier error must NOT proceed to commit by default"
        );
    }

    /// Sprint F1 Phase 4 — "requested but unavailable" must fail closed, the
    /// same as a configured backend that errors. Before this sprint the
    /// no-client branch returned `true` unconditionally (a silent pass) —
    /// that branch no longer exists; this proves the replacement is
    /// fail-closed without depending on a live `claude` binary or network:
    /// pointing `repo_path` at a directory that cannot exist makes the CLI
    /// spawn's `current_dir` fail deterministically, in any environment,
    /// which is exactly the "no backend available" case the brief describes.
    #[tokio::test]
    async fn requested_but_unavailable_verifier_fails_closed() {
        let task = lopi_core::Task::new("prove the gate can't silently pass");
        let (mut runner, _bus) = AgentRunner::standalone(
            task,
            std::path::PathBuf::from("/nonexistent/path/that/cannot/possibly/exist/lopi-f1-kt"),
        );
        let passed = runner.run_verifier_pass_for_test(&[]).await;
        assert!(
            !passed,
            "a verifier that could not even be spawned must block finalize, not pass it"
        );
    }

    /// The one escape hatch from the above is explicit operator opt-in —
    /// unchanged from the pre-existing `verifier_fail_open` semantics, now
    /// reachable from the "unavailable" branch too (it used to only apply to
    /// a *configured* backend that errored).
    #[tokio::test]
    async fn requested_but_unavailable_verifier_honors_explicit_fail_open() {
        let mut task = lopi_core::Task::new("operator explicitly accepts the risk");
        task.verifier_fail_open = true;
        let (mut runner, _bus) = AgentRunner::standalone(
            task,
            std::path::PathBuf::from("/nonexistent/path/that/cannot/possibly/exist/lopi-f1-kt"),
        );
        let passed = runner.run_verifier_pass_for_test(&[]).await;
        assert!(
            passed,
            "an explicit verifier_fail_open opt-in must still let the unavailable case proceed"
        );
    }

    #[test]
    fn verifier_error_proceeds_only_on_explicit_opt_in() {
        assert!(
            verifier_error_proceeds(true),
            "fail-open is available only as a deliberate operator override"
        );
    }
}
