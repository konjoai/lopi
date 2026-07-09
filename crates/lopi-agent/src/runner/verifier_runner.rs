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
    /// passed, or is not configured, or encountered a non-fatal error).
    /// Returns `false` when the verifier rejected the output; the caller
    /// must roll back and retry. Fix hints are already appended to
    /// `self.task.constraints` when `false` is returned.
    pub(super) async fn run_verifier_pass(&mut self, attempt: u8, test_errors: &[String]) -> bool {
        let Some(client) = self.api_client.clone() else {
            return true;
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
        let verdict = match VerifierAgent::new(client)
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
    use super::verifier_error_proceeds;

    #[test]
    fn verifier_error_is_fail_closed_by_default() {
        // The one thing an evaluator can't do: pass when it errors.
        assert!(
            !verifier_error_proceeds(false),
            "a verifier error must NOT proceed to commit by default"
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
