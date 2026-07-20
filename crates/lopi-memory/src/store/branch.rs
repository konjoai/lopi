//! MCPB-App-1 (KT-B1) — durable persistence for a running task's git branch.
//!
//! Split out of `mod.rs` (which was already at the 500-line CI gate) rather
//! than folded into that file's core task-CRUD block.

use super::MemoryStore;
use anyhow::Result;
use lopi_core::TaskId;

impl MemoryStore {
    /// Persist the git branch an attempt is running on — the only structured,
    /// queryable source of "which branch is this task on" (MCPB-App-1 KT-B1).
    /// Called from `AgentRunner::persist_branch` the moment `TaskStarted`
    /// fires; a later attempt's branch simply overwrites the earlier one.
    ///
    /// # Errors
    /// Returns `Err` if the database update fails.
    pub async fn set_task_branch(&self, id: &TaskId, branch: &str) -> Result<()> {
        sqlx::query("UPDATE tasks SET branch = ?1 WHERE id = ?2")
            .bind(branch)
            .bind(id.0.to_string())
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::MemoryStore;
    use lopi_core::{Task, TaskId};

    #[tokio::test]
    async fn set_task_branch_round_trips_through_load_history() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "running").await.unwrap();

        store
            .set_task_branch(&task.id, "lopi/abc-attempt-1")
            .await
            .unwrap();

        let rows = store.load_history(10).await.unwrap();
        let row = rows.iter().find(|r| r.id == task.id.0.to_string()).unwrap();
        assert_eq!(row.branch.as_deref(), Some("lopi/abc-attempt-1"));
    }

    #[tokio::test]
    async fn branch_is_none_until_set() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "queued").await.unwrap();

        let row = store.get_task(&task.id).await.unwrap().unwrap();
        assert!(row.branch.is_none());
    }

    #[tokio::test]
    async fn set_task_branch_on_unknown_task_is_a_silent_no_op() {
        // UPDATE against a nonexistent id affects zero rows, not an error —
        // mirrors `mark_running`/`mark_completed`'s existing behavior for
        // the same case.
        let store = MemoryStore::open_in_memory().await.unwrap();
        let ghost = TaskId::new();
        assert!(store
            .set_task_branch(&ghost, "lopi/ghost-attempt-1")
            .await
            .is_ok());
    }
}
