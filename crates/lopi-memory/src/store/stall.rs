//! AVO-Supervisor-2 (Feature 2) — durable persistence for the stall/thrash
//! detector's live status. Split out of `mod.rs` (which was already at the
//! 500-line CI gate) rather than folded into that file's core task-CRUD
//! block, same rationale as `branch.rs`/`task_repo.rs`.

use super::MemoryStore;
use anyhow::Result;
use lopi_core::TaskId;

impl MemoryStore {
    /// Record that the stall detector fired for this task on `attempt`,
    /// tagged with a short machine `reason` (`"plateau"` / `"thrash"` /
    /// `"plateau+thrash"`). Called from `AgentRunner::persist_stuck` the
    /// moment `TaskStatus::Stuck` is set; a later attempt's stall simply
    /// overwrites the earlier one, mirroring `set_task_branch`.
    ///
    /// # Errors
    /// Returns `Err` if the database update fails.
    pub async fn set_task_stuck(&self, id: &TaskId, attempt: u8, reason: &str) -> Result<()> {
        sqlx::query("UPDATE tasks SET stuck_at = ?1, stuck_reason = ?2 WHERE id = ?3")
            .bind(i64::from(attempt))
            .bind(reason)
            .bind(id.0.to_string())
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Clear a task's stall marker back to `NULL` — called on a goal-met
    /// success so a task that recovered from a stall doesn't carry a stale
    /// badge into its `Success` row.
    ///
    /// # Errors
    /// Returns `Err` if the database update fails.
    pub async fn clear_task_stuck(&self, id: &TaskId) -> Result<()> {
        sqlx::query("UPDATE tasks SET stuck_at = NULL, stuck_reason = NULL WHERE id = ?1")
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
    async fn set_task_stuck_round_trips_through_load_history() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "running").await.unwrap();

        store.set_task_stuck(&task.id, 3, "thrash").await.unwrap();

        let rows = store.load_history(10).await.unwrap();
        let row = rows.iter().find(|r| r.id == task.id.0.to_string()).unwrap();
        assert_eq!(row.stuck_at, Some(3));
        assert_eq!(row.stuck_reason.as_deref(), Some("thrash"));
    }

    #[tokio::test]
    async fn stuck_is_none_until_set() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "queued").await.unwrap();

        let row = store.get_task(&task.id).await.unwrap().unwrap();
        assert!(row.stuck_at.is_none());
        assert!(row.stuck_reason.is_none());
    }

    #[tokio::test]
    async fn clear_task_stuck_resets_both_columns() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "running").await.unwrap();
        store.set_task_stuck(&task.id, 2, "plateau").await.unwrap();

        store.clear_task_stuck(&task.id).await.unwrap();

        let row = store.get_task(&task.id).await.unwrap().unwrap();
        assert!(row.stuck_at.is_none());
        assert!(row.stuck_reason.is_none());
    }

    #[tokio::test]
    async fn set_task_stuck_on_unknown_task_is_a_silent_no_op() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let ghost = TaskId::new();
        assert!(store.set_task_stuck(&ghost, 1, "plateau").await.is_ok());
    }
}
