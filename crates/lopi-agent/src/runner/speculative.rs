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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::claude_events::ResultUsage;
    use lopi_core::{AgentEvent, Task};
    use lopi_memory::MemoryStore;
    use std::path::PathBuf;
    use std::process::Command;

    fn blank_output(usage: Option<ResultUsage>, cost_usd: Option<f64>) -> ClaudeOutput {
        ClaudeOutput {
            kind: None,
            result: None,
            is_error: None,
            cost_usd,
            duration_ms: Some(42),
            usage,
            raw: String::new(),
        }
    }

    #[test]
    fn saturating_u32_passes_through_in_range_values() {
        assert_eq!(saturating_u32(0), 0);
        assert_eq!(saturating_u32(1_000), 1_000);
        assert_eq!(saturating_u32(u64::from(u32::MAX)), u32::MAX);
    }

    #[test]
    fn saturating_u32_clamps_values_above_u32_max() {
        assert_eq!(saturating_u32(u64::from(u32::MAX) + 1), u32::MAX);
        assert_eq!(saturating_u32(u64::MAX), u32::MAX);
    }

    /// No store attached (the `AgentRunner::standalone` default) — must not
    /// panic, and real token usage must still be metered into `tokens_used`
    /// even though there's nowhere to persist a `turn_metrics` row.
    #[tokio::test]
    async fn record_speculative_usage_without_a_store_still_meters_tokens() {
        let (runner, _bus) = AgentRunner::standalone(Task::new("fix the bug"), PathBuf::from("."));
        let usage = ResultUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        };
        let out = blank_output(Some(usage), Some(0.01));
        runner
            .record_speculative_usage(&out, crate::claude::MODEL_SONNET, 0)
            .await;
        assert_eq!(runner.tokens_used.load(Ordering::Relaxed), 150);
    }

    /// Neither usage nor a nonzero cost — the early-return branch must skip
    /// persisting a `turn_metrics` row entirely, not write a mostly-zero one.
    #[tokio::test]
    async fn record_speculative_usage_skips_persisting_when_no_usage_and_no_cost() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let (mut runner, _bus) =
            AgentRunner::standalone(Task::new("fix the bug"), PathBuf::from("."));
        runner.store = Some(store.clone());
        let out = blank_output(None, None);
        runner
            .record_speculative_usage(&out, crate::claude::MODEL_SONNET, 0)
            .await;
        assert!(store.recent_turn_metrics(10).await.unwrap().is_empty());
    }

    /// Real usage with a store attached — a `turn_metrics` row must actually
    /// land, mirroring `stream.rs::persist_turn` for the streamed path.
    #[tokio::test]
    async fn record_speculative_usage_persists_a_row_when_usage_present() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let (mut runner, _bus) =
            AgentRunner::standalone(Task::new("fix the bug"), PathBuf::from("."));
        runner.store = Some(store.clone());
        let usage = ResultUsage {
            input_tokens: 200,
            output_tokens: 80,
            cache_read_tokens: 10,
            cache_write_tokens: 5,
        };
        let out = blank_output(Some(usage), Some(0.05));
        runner
            .record_speculative_usage(&out, crate::claude::MODEL_SONNET, 2)
            .await;
        let rows = store.recent_turn_metrics(10).await.unwrap();
        assert_eq!(rows.len(), 1, "one turn_metrics row must be persisted");
    }

    fn git(repo: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path().to_path_buf();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "t@konjoai.dev"]);
        git(&repo, &["config", "user.name", "tester"]);
        std::fs::write(repo.join("file.txt"), "base\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "base"]);
        (dir, repo)
    }

    /// A speculative plan-stream failure must roll back, transition the
    /// runner to `Retrying`, and always return `SpecFlow::Retry` —
    /// regardless of whether the git rollback itself succeeds (both calls
    /// are best-effort `.ok()`).
    #[tokio::test]
    async fn speculative_plan_failed_transitions_to_retrying_and_signals_retry() {
        let (_dir, repo) = init_repo();
        let git_mgr = GitManager::new(&repo).unwrap();
        let (mut runner, bus) = AgentRunner::standalone(Task::new("fix the bug"), repo);
        let mut rx = bus.subscribe();

        let flow = runner
            .speculative_plan_failed(&git_mgr, 1, "stream disconnected")
            .await;

        assert!(matches!(flow, SpecFlow::Retry));
        let mut saw_retrying = false;
        while let Ok(ev) = rx.try_recv() {
            if let AgentEvent::StatusChanged {
                status: TaskStatus::Retrying { attempt: 2 },
                ..
            } = ev
            {
                saw_retrying = true;
            }
        }
        assert!(
            saw_retrying,
            "must broadcast a Retrying{{attempt: 2}} status transition"
        );
    }
}
