//! Agent checkpoints — durable snapshots of an agent's lifecycle state for
//! crash-resume.
//!
//! The runner calls [`MemoryStore::save_checkpoint`] before any action that
//! can fail (plan / implement / score / PR). On crash or `lopi sail`
//! restart, `lopi resume --agent-id <uuid>` calls
//! [`MemoryStore::latest_checkpoint`] to recover the last known state.
//!
//! `state` is a lowercase snake_case string (e.g. `planning` /
//! `implementing` / `testing` / `scoring` / `done` / `errored`) so the
//! SQLite check constraint is human-readable in queries.

use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::TaskId;
use serde::{Deserialize, Serialize};
use sqlx::Row as _;
use uuid::Uuid;

use super::MemoryStore;

/// Persisted snapshot of one agent's progress on a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRow {
    /// Random UUID — primary key. Lets a task have many checkpoints in order.
    pub id: String,
    /// Owning task — joinable against the `tasks` table.
    pub task_id: String,
    /// Attempt number at the time of the snapshot.
    pub attempt: u8,
    /// Lowercase lifecycle-state discriminant: `planning` / `implementing`
    /// / `testing` / `scoring` / `done` / `errored` / `idle`.
    pub state: String,
    /// Most recent plan text (truncated upstream if huge).
    pub last_plan: Option<String>,
    /// Most recent score, serialized as JSON.
    pub last_score: Option<String>,
    /// Filesystem path the agent was working in.
    pub repo_path: Option<String>,
    /// Cheap hash of the accumulated session context — lets resume detect
    /// context drift without storing the full transcript.
    pub context_hash: Option<String>,
    /// ISO-8601 timestamp.
    pub created_at: String,
}

/// Builder for a new checkpoint — all fields except `task_id` and `state`
/// default to `None`/`0`.
#[derive(Debug, Clone)]
pub struct CheckpointInput {
    /// Identifier of the task this checkpoint belongs to.
    pub task_id: TaskId,
    /// Attempt number at the time the checkpoint was saved.
    pub attempt: u8,
    /// Serialised runner state label (e.g. `"planning"`, `"testing"`).
    pub state: String,
    /// JSON-encoded plan produced on this attempt, if any.
    pub last_plan: Option<String>,
    /// JSON-encoded score produced on this attempt, if any.
    pub last_score: Option<String>,
    /// Filesystem path of the repository being worked on.
    pub repo_path: Option<String>,
    /// Hash of the context window at checkpoint time for cache keying.
    pub context_hash: Option<String>,
}

impl CheckpointInput {
    /// Minimal constructor.
    #[must_use]
    pub fn new(task_id: TaskId, attempt: u8, state: impl Into<String>) -> Self {
        Self {
            task_id,
            attempt,
            state: state.into(),
            last_plan: None,
            last_score: None,
            repo_path: None,
            context_hash: None,
        }
    }
}

impl MemoryStore {
    /// Persist a fresh checkpoint row. Returns the generated `id` (UUID v4).
    ///
    /// # Errors
    /// Returns `Err` if the underlying SQLite insert fails.
    pub async fn save_checkpoint(&self, input: &CheckpointInput) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let task_id = input.task_id.0.to_string();
        sqlx::query(
            "INSERT INTO agent_checkpoints
               (id, task_id, attempt, state, last_plan, last_score,
                repo_path, context_hash, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&task_id)
        .bind(i64::from(input.attempt))
        .bind(&input.state)
        .bind(&input.last_plan)
        .bind(&input.last_score)
        .bind(&input.repo_path)
        .bind(&input.context_hash)
        .bind(&now)
        .execute(&self.write_pool)
        .await
        .context("inserting agent_checkpoints row")?;
        Ok(id)
    }

    /// Return the most-recent checkpoint for a task, or `None` if no
    /// checkpoints exist.
    ///
    /// # Errors
    /// Returns `Err` if the underlying SQLite query fails.
    pub async fn latest_checkpoint(&self, task_id: &TaskId) -> Result<Option<CheckpointRow>> {
        let tid = task_id.0.to_string();
        let row = sqlx::query(
            "SELECT id, task_id, attempt, state, last_plan, last_score,
                    repo_path, context_hash, created_at
             FROM agent_checkpoints
             WHERE task_id = ?
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
        )
        .bind(&tid)
        .fetch_optional(&self.read_pool)
        .await
        .context("querying latest agent_checkpoint")?;
        Ok(row.map(checkpoint_from_row))
    }

    /// Return all checkpoints for a task, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the underlying SQLite query fails.
    pub async fn list_checkpoints(
        &self,
        task_id: &TaskId,
        limit: i64,
    ) -> Result<Vec<CheckpointRow>> {
        let tid = task_id.0.to_string();
        let rows = sqlx::query(
            "SELECT id, task_id, attempt, state, last_plan, last_score,
                    repo_path, context_hash, created_at
             FROM agent_checkpoints
             WHERE task_id = ?
             ORDER BY created_at DESC, id DESC
             LIMIT ?",
        )
        .bind(&tid)
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await
        .context("listing agent_checkpoints")?;
        Ok(rows.into_iter().map(checkpoint_from_row).collect())
    }
}

fn checkpoint_from_row(row: sqlx::sqlite::SqliteRow) -> CheckpointRow {
    CheckpointRow {
        id: row.get("id"),
        task_id: row.get("task_id"),
        attempt: u8::try_from(row.get::<i64, _>("attempt")).unwrap_or(u8::MAX),
        state: row.get("state"),
        last_plan: row.get("last_plan"),
        last_score: row.get("last_score"),
        repo_path: row.get("repo_path"),
        context_hash: row.get("context_hash"),
        created_at: row.get("created_at"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_and_load_round_trip() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task_id = TaskId::new();
        let mut input = CheckpointInput::new(task_id, 1, "planning");
        input.last_plan = Some("# Plan\n- step 1\n- step 2".into());
        input.repo_path = Some("/tmp/repo".into());
        input.context_hash = Some("abc123".into());

        let id = store.save_checkpoint(&input).await.unwrap();
        assert_eq!(id.len(), 36, "uuid v4 is 36 chars");

        let loaded = store.latest_checkpoint(&task_id).await.unwrap().unwrap();
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.attempt, 1);
        assert_eq!(loaded.state, "planning");
        assert_eq!(
            loaded.last_plan.as_deref(),
            Some("# Plan\n- step 1\n- step 2")
        );
        assert_eq!(loaded.repo_path.as_deref(), Some("/tmp/repo"));
    }

    #[tokio::test]
    async fn latest_returns_none_for_unknown_task() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task_id = TaskId::new();
        assert!(store.latest_checkpoint(&task_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn latest_returns_newest_when_multiple_exist() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task_id = TaskId::new();
        store
            .save_checkpoint(&CheckpointInput::new(task_id, 1, "planning"))
            .await
            .unwrap();
        // Force a 1ms wait so created_at strictly increases.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let mut second = CheckpointInput::new(task_id, 2, "implementing");
        second.last_plan = Some("second pass".into());
        store.save_checkpoint(&second).await.unwrap();

        let latest = store.latest_checkpoint(&task_id).await.unwrap().unwrap();
        assert_eq!(latest.attempt, 2);
        assert_eq!(latest.state, "implementing");
    }

    #[tokio::test]
    async fn list_returns_all_newest_first() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task_id = TaskId::new();
        for i in 0..3 {
            store
                .save_checkpoint(&CheckpointInput::new(task_id, i, "planning"))
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        let rows = store.list_checkpoints(&task_id, 10).await.unwrap();
        assert_eq!(rows.len(), 3);
        // Newest first → attempt counts down.
        assert_eq!(rows[0].attempt, 2);
        assert_eq!(rows[2].attempt, 0);
    }
}
