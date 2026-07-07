//! Per-attempt lifecycle plumbing: event emission, cancellation checks, and
//! context-window bookkeeping shared by every phase of the run loop.
//!
//! Split out of `runner/mod.rs` (which owns the `AgentRunner` struct and its
//! builder methods) purely to keep that file under the 500-line gate — no
//! logic changed in the move.

use super::AgentRunner;
use lopi_context::{ContentBlock, Phase, PinPolicy, Role, TaggedMessage};
use lopi_core::{AgentEvent, Score, TaskId, TaskStatus};
use tokio::sync::oneshot;
use uuid::Uuid;

impl AgentRunner {
    pub(super) fn id(&self) -> TaskId {
        self.task.id
    }

    pub(super) fn log(&self, msg: impl Into<String>) {
        self.bus.send(AgentEvent::info(self.id(), msg));
    }

    pub(super) fn warn(&self, msg: impl Into<String>) {
        self.bus.send(AgentEvent::warn(self.id(), msg));
    }

    /// Broadcast a `StatusChanged` event and a `TurnMetrics` heartbeat.
    pub(super) fn status(&self, s: TaskStatus, attempt: u8) {
        let activity = match &s {
            TaskStatus::Planning => 0.45_f32,
            TaskStatus::AwaitingPlanApproval { .. } => 0.05_f32,
            TaskStatus::Implementing => 0.85_f32,
            TaskStatus::Testing => 0.55_f32,
            TaskStatus::Scoring => 0.30_f32,
            TaskStatus::Retrying { .. } => 0.40_f32,
            TaskStatus::Success { .. }
            | TaskStatus::Failed { .. }
            | TaskStatus::RolledBack
            | TaskStatus::Conflict { .. } => 0.0_f32,
            TaskStatus::Queued => 0.10_f32,
        };
        self.emit_turn_metrics(activity);
        self.bus.send(AgentEvent::StatusChanged {
            task_id: self.id(),
            status: s,
            attempt,
        });
    }

    /// Emit terminal bookkeeping for a finalized attempt and return its status.
    ///
    /// A genuine success pins the conclusion and marks an OTel `complete` span;
    /// a [`TaskStatus::Conflict`] (rebase collision) skips that — it is not a
    /// success — but both broadcast the status so the dashboards reflect reality.
    pub(super) fn conclude_finalized(
        &mut self,
        status: TaskStatus,
        score: &Score,
        attempt: u8,
    ) -> TaskStatus {
        if !matches!(status, TaskStatus::Conflict { .. }) {
            self.context.pin_conclusion(
                format!(
                    "Sprint succeeded — pass={:.0}% diff={}L",
                    score.test_pass_rate * 100.0,
                    score.diff_lines
                ),
                Phase::Conclusion,
            );
            tracing::info!(
                pressure = self.context.token_pressure(),
                "context at conclusion"
            );
            // OTel GenAI-aligned task-completion boundary span.
            let _ = tracing::info_span!(
                "lopi.agent.task.complete",
                task_id = %self.id(),
                outcome = "success",
                attempts = attempt,
            )
            .entered();
        }
        self.status(status.clone(), attempt);
        status
    }

    pub(super) fn emit_turn_metrics(&self, activity: f32) {
        let pressure = self.context.token_pressure();
        self.bus.send(AgentEvent::TurnMetrics {
            task_id: self.id(),
            pressure,
            activity,
            tokens_per_sec: 0.0,
            cost_usd: 0.0,
        });
    }

    pub(super) fn check_cancel(&mut self) -> bool {
        // Check the structured CancellationToken first (pool JoinSet teardown path).
        if self.cancel_token.is_cancelled() {
            self.log("⛔ cancelled via token");
            return true;
        }
        // Then check the legacy oneshot cancel channel (web API / CLI path).
        // A Closed channel means the sender was dropped (standalone/CLI path with no
        // active canceller) — that is NOT a cancellation, so we discard the receiver
        // and continue. Only an explicit Ok(()) send is a real cancel.
        if let Some(mut rx) = self.cancel_rx.take() {
            match rx.try_recv() {
                Ok(()) => {
                    self.log("⛔ cancelled by user");
                    return true;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.cancel_rx = Some(rx);
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    // Sender dropped (no active canceller) — proceed normally.
                }
            }
        }
        false
    }

    /// Pin the task goal as a Boot-phase turn so it's always visible across evictions.
    pub(super) fn boot_context(&mut self) {
        let content = vec![ContentBlock::Text(format!("Task goal: {}", self.task.goal))];
        let msg = TaggedMessage {
            id: Uuid::new_v4(),
            role: Role::User,
            content,
            tokens: 0,
            pin: PinPolicy::Always,
            phase: Phase::Boot,
            evict_after: None,
            tool_pair_id: None,
            is_conclusion: false,
        };
        self.context.push(msg).ok();
    }
}
