//! Per-attempt lifecycle plumbing: event emission, cancellation checks, and
//! context-window bookkeeping shared by every phase of the run loop.
//!
//! Split out of `runner/mod.rs` (which owns the `AgentRunner` struct and its
//! builder methods) purely to keep that file under the 500-line gate — no
//! logic changed in the move.

use super::AgentRunner;
use crate::dag::{NodeKind, NodeStatus};
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
        self.record_dag_transition(&s);
        self.emit_turn_metrics(activity);
        self.bus.send(AgentEvent::StatusChanged {
            task_id: self.id(),
            status: s,
            attempt,
        });
    }

    /// Best-effort persist of the attempt's branch into the `tasks.branch`
    /// column the moment `TaskStarted` fires (MCPB-App-1 KT-B1) — the only
    /// structured, queryable source of "which branch is this task on" while
    /// it's still in flight. Same fire-and-forget shape as
    /// `record_dag_transition`: errors are logged, never fatal to the run.
    pub(super) fn persist_branch(&self, branch: &str) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let task_id = self.id();
        let branch = branch.to_string();
        tokio::spawn(async move {
            if let Err(e) = store.set_task_branch(&task_id, &branch).await {
                tracing::warn!(error = %e, "failed to persist task branch");
            }
        });
    }

    /// Best-effort mirror of `TaskStatus` into the `agent_dag_nodes` table so
    /// `GET /api/agents/:id/dag` and `lopi replay` reflect real progress
    /// instead of an always-empty graph. This only *records* the DAG — the
    /// runner does not (yet) drive execution from it for partial restart;
    /// that integration is tracked separately. Persistence errors are
    /// logged, never fatal to the run.
    fn record_dag_transition(&self, s: &TaskStatus) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let transitions: Vec<(NodeKind, NodeStatus)> = match s {
            TaskStatus::Planning => vec![(NodeKind::Plan, NodeStatus::Running)],
            TaskStatus::Implementing => vec![
                (NodeKind::Plan, NodeStatus::Done),
                (NodeKind::Implement, NodeStatus::Running),
            ],
            TaskStatus::Testing => vec![
                (NodeKind::Implement, NodeStatus::Done),
                (NodeKind::Test, NodeStatus::Running),
            ],
            TaskStatus::Scoring => vec![
                (NodeKind::Test, NodeStatus::Done),
                (NodeKind::Score, NodeStatus::Running),
            ],
            TaskStatus::Success { .. } => vec![(NodeKind::Score, NodeStatus::Done)],
            TaskStatus::Failed { .. } => vec![(NodeKind::Score, NodeStatus::Failed)],
            _ => return,
        };
        let task_id = self.id().to_string();
        tokio::spawn(async move {
            for (kind, status) in transitions {
                let depends_on = kind
                    .predecessor()
                    .map_or_else(Vec::new, |p| vec![p.as_str()]);
                let depends_on_json = serde_json::to_string(&depends_on).unwrap_or_default();
                if let Err(e) = store
                    .upsert_dag_node(
                        &task_id,
                        kind.as_str(),
                        status.as_str(),
                        &depends_on_json,
                        None,
                        None,
                    )
                    .await
                {
                    tracing::warn!(error = %e, "failed to persist DAG node");
                }
            }
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::AgentRunner;
    use lopi_core::{Task, TaskStatus};
    use lopi_memory::MemoryStore;
    use std::path::PathBuf;

    /// Regression test for the "unwired DAG-node writer" finding:
    /// `status()` transitions used to only broadcast an `AgentEvent` — the
    /// `agent_dag_nodes` table (and therefore `GET /api/agents/:id/dag` /
    /// `lopi replay`) never got a single row written, no matter how far a
    /// real run progressed. Asserts a real `status()` call now persists a
    /// DAG node.
    #[tokio::test]
    async fn status_transition_persists_a_dag_node() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let (mut runner, _bus) =
            AgentRunner::standalone(Task::new("fix the bug"), PathBuf::from("."));
        runner.store = Some(store.clone());
        let task_id = runner.id().to_string();

        runner.status(TaskStatus::Planning, 1);

        // record_dag_transition fires a detached tokio::spawn; poll briefly
        // rather than assume it has already landed.
        let mut rows = Vec::new();
        for _ in 0..50 {
            rows = store.load_dag_nodes(&task_id).await.unwrap();
            if !rows.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(rows.len(), 1, "one DAG node persisted for the plan stage");
        assert_eq!(rows[0].kind, "plan");
        assert_eq!(rows[0].status, "running");
    }

    /// Two real transitions (Planning then Implementing) must leave the
    /// earlier stage marked Done, not stuck at Running forever.
    #[tokio::test]
    async fn later_transition_marks_earlier_stage_done() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let (mut runner, _bus) =
            AgentRunner::standalone(Task::new("fix the bug"), PathBuf::from("."));
        runner.store = Some(store.clone());
        let task_id = runner.id().to_string();

        runner.status(TaskStatus::Planning, 1);
        runner.status(TaskStatus::Implementing, 1);

        let mut rows = Vec::new();
        for _ in 0..50 {
            rows = store.load_dag_nodes(&task_id).await.unwrap();
            if rows.len() >= 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let plan = rows.iter().find(|r| r.kind == "plan").unwrap();
        let implement = rows.iter().find(|r| r.kind == "implement").unwrap();
        assert_eq!(plan.status, "done");
        assert_eq!(implement.status, "running");
    }
}
