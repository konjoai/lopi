//! Speculative planning mode — apply plan steps as they stream from claude,
//! instead of waiting for the full plan before implementing.

use super::AgentRunner;
use crate::claude::ClaudeCode;
use lopi_context::Phase;
use lopi_core::TaskStatus;
use lopi_git::GitManager;

/// Control-flow signal from the speculative branch back to the run loop.
pub(super) enum SpecFlow {
    /// Planning + implementation finished; proceed to diff-scope + scoring.
    Proceed,
    /// Planning failed; the branch has been rolled back — retry the attempt.
    Retry,
}

impl AgentRunner {
    /// Drive a speculative attempt: stream the plan and implement each step as
    /// it arrives. `attempt` is the zero-based attempt index.
    pub(super) async fn implement_speculative(
        &mut self,
        claude: &ClaudeCode,
        git: &GitManager,
        attempt: u8,
    ) -> SpecFlow {
        let (plan_handle, mut step_rx) = claude.plan_streaming(&self.task);
        self.status(TaskStatus::Implementing, attempt + 1);
        self.context.transition_phase(Phase::Implementation);
        tracing::info!(
            pressure = self.context.token_pressure(),
            "context at speculative implementation"
        );
        self.log("⚡ speculative: applying steps as plan streams…");

        while let Some(step) = step_rx.recv().await {
            self.log(format!("  ↳ {step}"));
            if let Err(e) = claude.implement_step(&self.task, &step).await {
                self.warn(format!("speculative step failed: {e}"));
            }
        }

        // Wait for the plan to finish; use the accumulated text for logging.
        let plan = match plan_handle.await {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                return self
                    .speculative_plan_failed(git, attempt, &e.to_string())
                    .await
            }
            Err(e) => {
                return self
                    .speculative_plan_failed(git, attempt, &e.to_string())
                    .await
            }
        };
        self.last_plan = Some(plan.clone());
        self.log(format!(
            "✅ speculative plan+implement done ({} chars)",
            plan.len()
        ));
        SpecFlow::Proceed
    }

    /// Roll back and mark the attempt for retry after a speculative plan stream
    /// failed.
    async fn speculative_plan_failed(
        &mut self,
        git: &GitManager,
        attempt: u8,
        err: &str,
    ) -> SpecFlow {
        self.warn(format!("plan stream failed: {err}"));
        git.hard_rollback().await.ok();
        git.checkout_default().await.ok();
        self.status(
            TaskStatus::Retrying {
                attempt: attempt + 1,
            },
            attempt + 1,
        );
        SpecFlow::Retry
    }
}
