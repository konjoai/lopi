//! Sprint P3a (review-pipeline plan, Phase 3 prerequisite) — durable
//! persistence for a running attempt's readonly-Planner output. Mirrors
//! `cli_session.rs` exactly: written the moment a schema-valid
//! `PlanArtifact` is produced (`AgentRunner::persist_plan_artifact`, called
//! before the Executor spawns), same "later attempt overwrites" semantics.

use super::MemoryStore;
use anyhow::Result;
use lopi_core::TaskId;

impl MemoryStore {
    /// Persist the readonly Planner's JSON-serialized `PlanArtifact` for an
    /// attempt. Called from `AgentRunner::persist_plan_artifact` immediately
    /// after a successful Planner call, before the Executor spawns, so a
    /// crashed Executor still leaves the plan on record (`abort_attempt`'s
    /// git operations never touch this store). A later attempt's artifact
    /// simply overwrites the earlier one (matches `set_task_cli_session_id`).
    ///
    /// # Errors
    /// Returns `Err` if the database update fails.
    pub async fn set_task_plan_artifact(
        &self,
        id: &TaskId,
        plan_artifact_json: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE tasks SET plan_artifact = ?1 WHERE id = ?2")
            .bind(plan_artifact_json)
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
    async fn set_task_plan_artifact_round_trips_through_load_history() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "running").await.unwrap();

        store
            .set_task_plan_artifact(&task.id, r#"{"goal":"do a thing"}"#)
            .await
            .unwrap();

        let rows = store.load_history(10).await.unwrap();
        let row = rows.iter().find(|r| r.id == task.id.0.to_string()).unwrap();
        assert_eq!(
            row.plan_artifact.as_deref(),
            Some(r#"{"goal":"do a thing"}"#)
        );
    }

    #[tokio::test]
    async fn plan_artifact_is_none_until_set() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "queued").await.unwrap();

        let row = store.get_task(&task.id).await.unwrap().unwrap();
        assert!(row.plan_artifact.is_none());
    }

    #[tokio::test]
    async fn set_task_plan_artifact_on_unknown_task_is_a_silent_no_op() {
        // UPDATE against a nonexistent id affects zero rows, not an error --
        // mirrors set_task_cli_session_id's own behavior.
        let store = MemoryStore::open_in_memory().await.unwrap();
        let ghost = TaskId::new();
        assert!(store
            .set_task_plan_artifact(&ghost, "ignored")
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn set_task_plan_artifact_overwrites_a_later_attempts_value() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do a thing");
        store.save_task(&task, "running").await.unwrap();

        store
            .set_task_plan_artifact(&task.id, r#"{"goal":"attempt 1"}"#)
            .await
            .unwrap();
        store
            .set_task_plan_artifact(&task.id, r#"{"goal":"attempt 2"}"#)
            .await
            .unwrap();

        let row = store.get_task(&task.id).await.unwrap().unwrap();
        assert_eq!(
            row.plan_artifact.as_deref(),
            Some(r#"{"goal":"attempt 2"}"#)
        );
    }
}
