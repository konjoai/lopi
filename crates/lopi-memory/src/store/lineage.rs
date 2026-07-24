//! Sprint Successor-1 — task lineage reads. Split out of `store/mod.rs`
//! purely to keep that file under the 500-line CI file-size gate (same
//! rationale as `dag.rs`/`task_logs.rs`'s earlier splits); `get_task` moved
//! here alongside the new `lineage_chain` since the latter is built directly
//! on top of the former.

use super::{MemoryStore, TaskRow};
use anyhow::Result;
use lopi_core::TaskId;

impl MemoryStore {
    /// Fetch a single task row by id.
    ///
    /// Stack-Chain-1 — used by `ChainScheduleManager`'s boot-time resume to
    /// tell whether a step's task actually reached a terminal state before
    /// the process restarted (in which case the chain advances) or was still
    /// in flight and lost with the old process's in-memory queue (in which
    /// case the step must be resubmitted).
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn get_task(&self, id: &TaskId) -> Result<Option<TaskRow>> {
        let row = sqlx::query_as::<_, TaskRow>(
            "SELECT id, goal, status, created_at, completed_at, client_ref, branch, repo, \
             parent_task, chain_depth FROM tasks WHERE id = ?1",
        )
        .bind(id.0.to_string())
        .fetch_optional(&self.read_pool)
        .await?;
        Ok(row)
    }

    /// Sprint Successor-1 — a task's ancestor chain: itself, then its
    /// parent, grandparent, and so on, stopping at the root (a task with no
    /// `parent_task`) or after `max_depth` ancestor hops, whichever comes
    /// first. A bounded walk up the parent pointers — not a full recursive
    /// descendant tree, which is out of scope this sprint.
    ///
    /// # Errors
    /// Returns `Err` if a database query fails partway through the walk.
    pub async fn lineage_chain(&self, task_id: &TaskId, max_depth: u8) -> Result<Vec<TaskRow>> {
        let mut chain = Vec::new();
        let mut current = Some(*task_id);
        for _ in 0..=max_depth {
            let Some(id) = current else { break };
            let Some(row) = self.get_task(&id).await? else {
                break;
            };
            current = row
                .parent_task
                .as_deref()
                .and_then(|p| uuid::Uuid::parse_str(p).ok())
                .map(TaskId);
            let reached_root = current.is_none();
            chain.push(row);
            if reached_root {
                break;
            }
        }
        Ok(chain)
    }
}
