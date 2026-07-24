//! macOS-Web-Parity-5 — durable persistence for a running task's effective
//! repo. Mirrors `branch.rs` (MCPB-App-1) exactly: same write-site timing
//! (unresolved until dequeue, so persisted the moment `TaskStarted` fires,
//! not at initial `save_task`), same "later attempt overwrites" semantics.

use super::MemoryStore;
use anyhow::Result;
use lopi_core::TaskId;

impl MemoryStore {
    /// Persist the effective repo (task-level override, or the pool default)
    /// an attempt is running against — the only structured, queryable
    /// source of "which repo is this task on" while it's in flight or after
    /// it finishes. Called from `AgentRunner::persist_repo` the moment
    /// `TaskStarted` fires; a later attempt's repo simply overwrites the
    /// earlier one (matches `set_task_branch`).
    ///
    /// # Errors
    /// Returns `Err` if the database update fails.
    pub async fn set_task_repo(&self, id: &TaskId, repo: &str) -> Result<()> {
        sqlx::query("UPDATE tasks SET repo = ?1 WHERE id = ?2")
            .bind(repo)
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
    async fn set_task_repo_round_trips_through_load_history() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "running").await.unwrap();

        store
            .set_task_repo(&task.id, "/Users/dev/lopi")
            .await
            .unwrap();

        let rows = store.load_history(10).await.unwrap();
        let row = rows.iter().find(|r| r.id == task.id.0.to_string()).unwrap();
        assert_eq!(row.repo.as_deref(), Some("/Users/dev/lopi"));
    }

    #[tokio::test]
    async fn repo_is_none_until_set() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "queued").await.unwrap();

        let row = store.get_task(&task.id).await.unwrap().unwrap();
        assert!(row.repo.is_none());
    }

    #[tokio::test]
    async fn set_task_repo_on_unknown_task_is_a_silent_no_op() {
        // UPDATE against a nonexistent id affects zero rows, not an error —
        // mirrors `set_task_branch`'s own behavior for the same case.
        let store = MemoryStore::open_in_memory().await.unwrap();
        let ghost = TaskId::new();
        assert!(store.set_task_repo(&ghost, "/tmp/ghost").await.is_ok());
    }

    #[tokio::test]
    async fn set_task_repo_overwrites_a_later_attempts_value() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "running").await.unwrap();

        store.set_task_repo(&task.id, "/repo/a").await.unwrap();
        store.set_task_repo(&task.id, "/repo/b").await.unwrap();

        let row = store.get_task(&task.id).await.unwrap().unwrap();
        assert_eq!(row.repo.as_deref(), Some("/repo/b"));
    }
}
