//! Sprint F4 Phase 4 — durable persistence for a running attempt's CLI
//! session id. Mirrors `branch.rs` (MCPB-App-1) / `task_repo.rs`
//! (macOS-Web-Parity-5) exactly: written the moment the id is minted
//! (`AgentRunner::persist_cli_session`, called before the plan phase's first
//! spawn), same "later attempt overwrites" semantics.

use super::MemoryStore;
use anyhow::Result;
use lopi_core::TaskId;

impl MemoryStore {
    /// Persist the CLI's own resumable session id for an attempt — the join
    /// key `lopi diag`, replay, and the CLI's own transcripts
    /// (`transcript_import.rs`) can share. Called from
    /// `AgentRunner::persist_cli_session` the moment the id is minted; a
    /// later attempt's id simply overwrites the earlier one (matches
    /// `set_task_branch`/`set_task_repo`).
    ///
    /// # Errors
    /// Returns `Err` if the database update fails.
    pub async fn set_task_cli_session_id(&self, id: &TaskId, cli_session_id: &str) -> Result<()> {
        sqlx::query("UPDATE tasks SET cli_session_id = ?1 WHERE id = ?2")
            .bind(cli_session_id)
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
    async fn set_task_cli_session_id_round_trips_through_load_history() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "running").await.unwrap();

        store
            .set_task_cli_session_id(&task.id, "35faaa8b-8553-4b16-a67e-348c1fac42ff")
            .await
            .unwrap();

        let rows = store.load_history(10).await.unwrap();
        let row = rows.iter().find(|r| r.id == task.id.0.to_string()).unwrap();
        assert_eq!(
            row.cli_session_id.as_deref(),
            Some("35faaa8b-8553-4b16-a67e-348c1fac42ff")
        );
    }

    #[tokio::test]
    async fn cli_session_id_is_none_until_set() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "queued").await.unwrap();

        let row = store.get_task(&task.id).await.unwrap().unwrap();
        assert!(row.cli_session_id.is_none());
    }

    #[tokio::test]
    async fn set_task_cli_session_id_on_unknown_task_is_a_silent_no_op() {
        // UPDATE against a nonexistent id affects zero rows, not an error —
        // mirrors `set_task_branch`/`set_task_repo`'s own behavior.
        let store = MemoryStore::open_in_memory().await.unwrap();
        let ghost = TaskId::new();
        assert!(store
            .set_task_cli_session_id(&ghost, "ignored")
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn set_task_cli_session_id_overwrites_a_later_attempts_value() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "running").await.unwrap();

        store
            .set_task_cli_session_id(&task.id, "attempt-1-session")
            .await
            .unwrap();
        store
            .set_task_cli_session_id(&task.id, "attempt-2-session")
            .await
            .unwrap();

        let row = store.get_task(&task.id).await.unwrap().unwrap();
        assert_eq!(row.cli_session_id.as_deref(), Some("attempt-2-session"));
    }
}
