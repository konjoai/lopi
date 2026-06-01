//! Dead-letter queue for tasks that exhausted their retry budget.
//!
//! The orchestrator pool already gives a `Task` up to `task.max_retries`
//! attempts. When the last attempt still ends in `TaskStatus::Failed` (or
//! the runner itself errored), the task previously vanished — there was
//! no record of *why* the work stopped flowing. The DLQ closes that gap:
//! the pool calls [`MemoryStore::push_dead_letter`] on the terminal
//! failure path, and an operator can later inspect, manually re-enqueue
//! ([`MemoryStore::take_dead_letter`]) or permanently discard
//! ([`MemoryStore::delete_dead_letter`]).
//!
//! Read-side queries use the `idx_dlq_dead_at` index so the newest
//! entries come back first without a sort over the full table.

use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::TaskId;
use serde::{Deserialize, Serialize};
use sqlx::Row as _;
use uuid::Uuid;

use super::MemoryStore;

/// One row from the dead-letter queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterRow {
    /// Row-level UUID — primary key.
    pub id: String,
    /// Owning `TaskId` (stringified UUID).
    pub task_id: String,
    /// Goal text, copied at submit time so the original task can be
    /// reconstructed even if `tasks` is pruned.
    pub goal: String,
    /// Repo the task targeted, if any.
    pub repo_path: Option<String>,
    /// How many attempts the runner made before giving up.
    pub total_attempts: u8,
    /// Final failure reason (usually from `TaskStatus::Failed.reason`).
    pub last_error: Option<String>,
    /// First-known failure timestamp (ISO-8601).
    pub first_failed_at: String,
    /// Wall-clock time the task landed in the DLQ.
    pub dead_at: String,
    /// Originating surface — `cli`, `api`, `webhook`, `telegram`, etc.
    pub source: String,
}

/// Input to [`MemoryStore::push_dead_letter`]. Mirrors `DeadLetterRow`
/// minus the generated columns (`id`, `first_failed_at`, `dead_at`).
#[derive(Debug, Clone)]
pub struct DeadLetterInput {
    /// Identifier of the task that was moved to the dead-letter queue.
    pub task_id: TaskId,
    /// Original task goal text.
    pub goal: String,
    /// Repository path the task was targeting, if known.
    pub repo_path: Option<String>,
    /// Total number of attempts made before the task was declared dead.
    pub total_attempts: u8,
    /// Last error message recorded before giving up, if any.
    pub last_error: Option<String>,
    /// Source that originally submitted the task (e.g. `"webhook"`, `"api"`).
    pub source: String,
}

impl DeadLetterInput {
    /// Minimal builder — leaves the runtime fields blank so callers can
    /// fill them in.
    #[must_use]
    pub fn new(task_id: TaskId, goal: impl Into<String>) -> Self {
        Self {
            task_id,
            goal: goal.into(),
            repo_path: None,
            total_attempts: 0,
            last_error: None,
            source: "unknown".into(),
        }
    }
}

impl MemoryStore {
    /// Persist a `DeadLetterRow` and return the new row id.
    ///
    /// `first_failed_at` and `dead_at` are stamped server-side so the
    /// caller does not need to thread a clock through.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite insert fails.
    pub async fn push_dead_letter(&self, input: &DeadLetterInput) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let task_id = input.task_id.0.to_string();
        sqlx::query(
            "INSERT INTO dead_letter_queue
               (id, task_id, goal, repo_path, total_attempts, last_error,
                first_failed_at, dead_at, source)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&task_id)
        .bind(&input.goal)
        .bind(&input.repo_path)
        .bind(i64::from(input.total_attempts))
        .bind(&input.last_error)
        .bind(&now)
        .bind(&now)
        .bind(&input.source)
        .execute(&self.write_pool)
        .await
        .context("inserting dead_letter_queue row")?;
        Ok(id)
    }

    /// Read the most-recent `limit` entries, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn list_dead_letters(&self, limit: i64) -> Result<Vec<DeadLetterRow>> {
        let rows = sqlx::query(
            "SELECT id, task_id, goal, repo_path, total_attempts, last_error,
                    first_failed_at, dead_at, source
             FROM dead_letter_queue
             ORDER BY dead_at DESC, id DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await
        .context("listing dead_letter_queue")?;
        Ok(rows.into_iter().map(dlq_from_row).collect())
    }

    /// Fetch a single DLQ row by its primary key.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn get_dead_letter(&self, id: &str) -> Result<Option<DeadLetterRow>> {
        let row = sqlx::query(
            "SELECT id, task_id, goal, repo_path, total_attempts, last_error,
                    first_failed_at, dead_at, source
             FROM dead_letter_queue
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.read_pool)
        .await
        .context("fetching dead_letter_queue row")?;
        Ok(row.map(dlq_from_row))
    }

    /// Atomically remove and return a DLQ row — for `POST /tasks/:id/retry`.
    /// Returns `Ok(None)` when no row matches.
    ///
    /// # Errors
    /// Returns `Err` if either the read or the delete fails.
    pub async fn take_dead_letter(&self, id: &str) -> Result<Option<DeadLetterRow>> {
        // Single-writer pool means this read-then-delete pair is effectively
        // serialised — no concurrent caller can race with us between the two
        // statements.
        let row = self.get_dead_letter(id).await?;
        if row.is_some() {
            sqlx::query("DELETE FROM dead_letter_queue WHERE id = ?")
                .bind(id)
                .execute(&self.write_pool)
                .await
                .context("deleting dead_letter_queue row")?;
        }
        Ok(row)
    }

    /// Permanently discard a DLQ row.
    /// Returns `true` when a row was removed.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite delete fails.
    pub async fn delete_dead_letter(&self, id: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM dead_letter_queue WHERE id = ?")
            .bind(id)
            .execute(&self.write_pool)
            .await
            .context("deleting dead_letter_queue row")?;
        Ok(res.rows_affected() > 0)
    }

    /// Count entries — feeds `/metrics` and stats panels.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn count_dead_letters(&self) -> Result<u64> {
        let row = sqlx::query("SELECT COUNT(*) as c FROM dead_letter_queue")
            .fetch_one(&self.read_pool)
            .await
            .context("counting dead_letter_queue")?;
        let c: i64 = row.get("c");
        Ok(u64::try_from(c).unwrap_or(0))
    }
}

fn dlq_from_row(row: sqlx::sqlite::SqliteRow) -> DeadLetterRow {
    DeadLetterRow {
        id: row.get("id"),
        task_id: row.get("task_id"),
        goal: row.get("goal"),
        repo_path: row.get("repo_path"),
        total_attempts: u8::try_from(row.get::<i64, _>("total_attempts")).unwrap_or(u8::MAX),
        last_error: row.get("last_error"),
        first_failed_at: row.get("first_failed_at"),
        dead_at: row.get("dead_at"),
        source: row.get("source"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn input(goal: &str) -> DeadLetterInput {
        let mut i = DeadLetterInput::new(TaskId::new(), goal);
        i.repo_path = Some("/tmp/repo".into());
        i.total_attempts = 3;
        i.last_error = Some("tests failed after 3 attempts".into());
        i.source = "cli".into();
        i
    }

    #[tokio::test]
    async fn push_round_trips_through_get() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let inp = input("fix the flaky test");
        let id = store.push_dead_letter(&inp).await.unwrap();
        let row = store.get_dead_letter(&id).await.unwrap().unwrap();
        assert_eq!(row.id, id);
        assert_eq!(row.goal, "fix the flaky test");
        assert_eq!(row.total_attempts, 3);
        assert_eq!(row.source, "cli");
        assert_eq!(row.repo_path.as_deref(), Some("/tmp/repo"));
        assert_eq!(
            row.last_error.as_deref(),
            Some("tests failed after 3 attempts")
        );
    }

    #[tokio::test]
    async fn list_returns_newest_first() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        for g in ["one", "two", "three"] {
            store.push_dead_letter(&input(g)).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        let rows = store.list_dead_letters(10).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].goal, "three");
        assert_eq!(rows[2].goal, "one");
    }

    #[tokio::test]
    async fn list_respects_limit() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        for i in 0..5 {
            store
                .push_dead_letter(&input(&format!("g{i}")))
                .await
                .unwrap();
        }
        let rows = store.list_dead_letters(2).await.unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn take_removes_the_row_and_returns_it() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let id = store.push_dead_letter(&input("retry me")).await.unwrap();
        assert_eq!(store.count_dead_letters().await.unwrap(), 1);
        let row = store.take_dead_letter(&id).await.unwrap().unwrap();
        assert_eq!(row.goal, "retry me");
        assert_eq!(store.count_dead_letters().await.unwrap(), 0);
        // A second take is a clean None — no error.
        assert!(store.take_dead_letter(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_returns_true_only_on_hit() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let id = store.push_dead_letter(&input("hello")).await.unwrap();
        assert!(store.delete_dead_letter(&id).await.unwrap());
        assert!(!store.delete_dead_letter(&id).await.unwrap());
        assert!(!store.delete_dead_letter("does-not-exist").await.unwrap());
    }

    #[tokio::test]
    async fn count_is_zero_on_a_fresh_store() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        assert_eq!(store.count_dead_letters().await.unwrap(), 0);
    }
}
