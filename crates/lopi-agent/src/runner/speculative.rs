//! Speculative planning mode — apply plan steps as they stream from claude,
//! instead of waiting for the full plan before implementing.

use super::AgentRunner;
use crate::claude::ClaudeCode;
use crate::claude_model::ClaudeOutput;
use chrono::Utc;
use lopi_context::Phase;
use lopi_core::{TaskStatus, TurnMetrics};
use lopi_git::GitManager;
use std::sync::atomic::Ordering;
use uuid::Uuid;

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
        model: &str,
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
            match claude.implement_step(&self.task, &step).await {
                Ok(out) => self.record_speculative_usage(&out, model, attempt).await,
                Err(e) => self.warn(format!("speculative step failed: {e}")),
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

    /// Feed a completed speculative `implement_step` call's real usage into
    /// the runner's token counter — the same `tokens_used` the progress
    /// gate's budget check reads (`progress.rs::budget_exceeded`) — and
    /// persist a `turn_metrics` row, mirroring `stream.rs::persist_turn` for
    /// the streamed path. Without this, `--speculative` runs spent real
    /// tokens/dollars per applied step that never tripped the budget gate
    /// and never showed up on any cost dashboard.
    async fn record_speculative_usage(&self, out: &ClaudeOutput, model: &str, attempt: u8) {
        let usage = out.usage.unwrap_or_default();
        let spent = usage.input_tokens + usage.output_tokens;
        if spent > 0 {
            self.tokens_used.fetch_add(spent, Ordering::Relaxed);
        }

        let Some(store) = &self.store else { return };
        if out.usage.is_none() && out.cost_usd.unwrap_or(0.0) == 0.0 {
            return;
        }
        let metrics = TurnMetrics {
            turn_id: Uuid::new_v4(),
            task_id: self.task.id,
            session_id: self.session_id,
            model: model.to_string(),
            attempt_number: attempt,
            input_tokens: saturating_u32(usage.input_tokens),
            output_tokens: saturating_u32(usage.output_tokens),
            cache_read_input_tokens: saturating_u32(usage.cache_read_tokens),
            cache_write_input_tokens: saturating_u32(usage.cache_write_tokens),
            ttft_ms: 0,
            turn_latency_ms: out.duration_ms.unwrap_or(0),
            tool_execution_ms: 0,
            context_tokens: 0,
            context_pressure: self.context.token_pressure(),
            evictions_this_turn: 0,
            tool_calls: 0,
            tools_parallel: false,
            estimated_cost_usd: out.cost_usd.unwrap_or(0.0),
            timestamp: Utc::now(),
        };
        if let Err(e) = store.save_turn_metrics(&metrics).await {
            tracing::warn!(error = %e, "failed to persist speculative turn metrics");
        }
    }
}

/// Saturating `u64` → `u32`, matching `stream.rs`'s `SaturatingU32` cast for
/// `turn_metrics`' `u32` token columns.
fn saturating_u32(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

impl AgentRunner {
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
