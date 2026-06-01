//! Sprint S — Konjo Verifier integration inside the agent runner.
use super::AgentRunner;
use crate::verifier::{default_rubric, get_repo_diff, VerifierAgent};
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
        let rubric = self.task.rubric.clone().unwrap_or_else(default_rubric);
        let diff = get_repo_diff(&self.repo_path).await;
        let test_output = test_errors.join("\n");

        self.log("🔬 verifier: grading output against rubric…");
        let verdict = match VerifierAgent::new(client)
            .verify(&self.task.goal, &plan, &diff, &test_output, &rubric)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                warn!("verifier error (non-fatal, proceeding): {e}");
                return true;
            }
        };

        self.log(format!(
            "🔬 verifier: passed={} confidence={:.0}% gaps={}",
            verdict.passed,
            verdict.confidence * 100.0,
            verdict.gaps.len()
        ));

        persist_and_emit(self, attempt, &verdict).await;

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
}

async fn persist_and_emit(runner: &AgentRunner, attempt: u8, verdict: &lopi_core::VerifierVerdict) {
    if let Some(store) = &runner.store {
        if let Err(e) = store
            .save_verifier_verdict(
                &runner.task.id.to_string(),
                attempt,
                verdict,
                crate::claude::MODEL_OPUS,
            )
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
