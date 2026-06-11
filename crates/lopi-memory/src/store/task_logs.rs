//! Per-task log ring buffer.
//!
//! Mirrors `AgentEvent::LogLine` into SQLite so the web UI's
//! per-task SSE stream has a historical tail and operators can
//! retroactively inspect a finished task's output. Capped to the
//! most recent `MAX_PER_TASK` rows via [`MemoryStore::prune_task_logs`],
//! invoked from the broadcast bridge after every insert.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row as _;

use super::MemoryStore;

/// Maximum log lines kept per task. The bridge prunes after each insert
/// so the table cannot grow without bound on a busy fleet.
pub const MAX_PER_TASK: i64 = 1_000;

/// One row from `task_logs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLogRow {
    /// Monotonic cursor — primary key.
    pub id: i64,
    /// Owning task UUID (stringified).
    pub task_id: String,
    /// ISO-8601 wall clock from the original `LogLine` event.
    pub ts: String,
    /// Lowercase log level — `info` / `warn` / `error` / `debug`.
    pub level: String,
    /// The log line itself, free-form text.
    pub line: String,
}

impl MemoryStore {
    /// Persist one log line for a task.
    ///
    /// Callers should invoke [`Self::prune_task_logs`] after a batch of
    /// inserts to keep the per-task tail bounded.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite insert fails.
    pub async fn record_task_log(
        &self,
        task_id: &str,
        ts: DateTime<Utc>,
        level: &str,
        line: &str,
    ) -> Result<i64> {
        let res =
            sqlx::query("INSERT INTO task_logs (task_id, ts, level, line) VALUES (?, ?, ?, ?)")
                .bind(task_id)
                .bind(ts.to_rfc3339())
                .bind(level)
                .bind(line)
                .execute(&self.write_pool)
                .await
                .context("inserting task_logs row")?;
        Ok(res.last_insert_rowid())
    }

    /// Read the most-recent `limit` log lines for `task_id`, oldest first.
    ///
    /// Returns at most `limit` rows, clamped internally to `[1, 5000]` so
    /// a runaway request cannot exhaust the read pool.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn load_task_logs(&self, task_id: &str, limit: i64) -> Result<Vec<TaskLogRow>> {
        let limit = limit.clamp(1, 5_000);
        // Pull the newest `limit` rows by descending id, then reverse so
        // the caller sees them in chronological order.
        let rows = sqlx::query(
            "SELECT id, task_id, ts, level, line
             FROM task_logs
             WHERE task_id = ?
             ORDER BY id DESC
             LIMIT ?",
        )
        .bind(task_id)
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await
        .context("loading task_logs")?;
        let mut out: Vec<TaskLogRow> = rows.into_iter().map(row_to_log).collect();
        out.reverse();
        Ok(out)
    }

    /// Read the most-recent `limit` log lines across *all* tasks, oldest
    /// first. Powers the global Logs tab in the web dashboard.
    ///
    /// Returns at most `limit` rows, clamped internally to `[1, 5000]` so
    /// a runaway request cannot exhaust the read pool.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn load_recent_task_logs(&self, limit: i64) -> Result<Vec<TaskLogRow>> {
        let limit = limit.clamp(1, 5_000);
        let rows = sqlx::query(
            "SELECT id, task_id, ts, level, line
             FROM task_logs
             ORDER BY id DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await
        .context("loading recent task_logs")?;
        let mut out: Vec<TaskLogRow> = rows.into_iter().map(row_to_log).collect();
        out.reverse();
        Ok(out)
    }

    /// Keep only the most-recent `MAX_PER_TASK` rows for `task_id`.
    /// Idempotent and cheap — called from the broadcast bridge after
    /// every insert for the affected task.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite delete fails.
    pub async fn prune_task_logs(&self, task_id: &str) -> Result<u64> {
        // Find the cutoff `id`: the (MAX_PER_TASK + 1)-th newest row,
        // and delete every older row in one statement.
        let row = sqlx::query(
            "SELECT id FROM task_logs
             WHERE task_id = ?
             ORDER BY id DESC
             LIMIT 1 OFFSET ?",
        )
        .bind(task_id)
        .bind(MAX_PER_TASK)
        .fetch_optional(&self.read_pool)
        .await
        .context("computing task_logs prune cutoff")?;
        let Some(row) = row else { return Ok(0) };
        let cutoff: i64 = row.get("id");
        let res = sqlx::query("DELETE FROM task_logs WHERE task_id = ? AND id <= ?")
            .bind(task_id)
            .bind(cutoff)
            .execute(&self.write_pool)
            .await
            .context("pruning task_logs")?;
        Ok(res.rows_affected())
    }

    /// Total log lines across all tasks — for `/metrics` exposition.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn count_task_logs(&self) -> Result<u64> {
        let row = sqlx::query("SELECT COUNT(*) as c FROM task_logs")
            .fetch_one(&self.read_pool)
            .await
            .context("counting task_logs")?;
        let c: i64 = row.get("c");
        Ok(u64::try_from(c).unwrap_or(0))
    }
}

fn row_to_log(row: sqlx::sqlite::SqliteRow) -> TaskLogRow {
    TaskLogRow {
        id: row.get("id"),
        task_id: row.get("task_id"),
        ts: row.get("ts"),
        level: row.get("level"),
        line: row.get("line"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn record_and_load_round_trip() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tid = "task-alpha";
        let now = Utc::now();
        store
            .record_task_log(tid, now, "info", "hello")
            .await
            .unwrap();
        store
            .record_task_log(tid, now, "warn", "almost done")
            .await
            .unwrap();
        let rows = store.load_task_logs(tid, 10).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].line, "hello");
        assert_eq!(rows[1].level, "warn");
    }

    #[tokio::test]
    async fn load_returns_oldest_first() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tid = "task-beta";
        let now = Utc::now();
        for i in 0..5 {
            store
                .record_task_log(tid, now, "info", &format!("line {i}"))
                .await
                .unwrap();
        }
        let rows = store.load_task_logs(tid, 10).await.unwrap();
        assert_eq!(rows.len(), 5);
        // First row inserted comes back first.
        assert_eq!(rows[0].line, "line 0");
        assert_eq!(rows[4].line, "line 4");
    }

    #[tokio::test]
    async fn load_respects_limit_returning_newest_window() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tid = "task-gamma";
        let now = Utc::now();
        for i in 0..10 {
            store
                .record_task_log(tid, now, "info", &format!("line {i}"))
                .await
                .unwrap();
        }
        // limit=3 should return the 3 most-recent (lines 7, 8, 9) in
        // chronological order.
        let rows = store.load_task_logs(tid, 3).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].line, "line 7");
        assert_eq!(rows[2].line, "line 9");
    }

    #[tokio::test]
    async fn prune_caps_per_task_at_max_per_task() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tid = "task-prune";
        let now = Utc::now();
        // Insert MAX_PER_TASK + 5 → prune drops 5 oldest.
        let total = MAX_PER_TASK + 5;
        for i in 0..total {
            store
                .record_task_log(tid, now, "info", &format!("line {i}"))
                .await
                .unwrap();
        }
        let deleted = store.prune_task_logs(tid).await.unwrap();
        assert_eq!(deleted, 5);
        let rows = store.load_task_logs(tid, MAX_PER_TASK + 10).await.unwrap();
        assert_eq!(rows.len() as i64, MAX_PER_TASK);
        // The 5 oldest lines are gone; the first remaining line is "line 5".
        assert_eq!(rows[0].line, "line 5");
    }

    #[tokio::test]
    async fn prune_below_max_is_noop() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tid = "task-small";
        store
            .record_task_log(tid, Utc::now(), "info", "only one")
            .await
            .unwrap();
        let deleted = store.prune_task_logs(tid).await.unwrap();
        assert_eq!(deleted, 0);
        let rows = store.load_task_logs(tid, 10).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn recent_spans_all_tasks_oldest_first() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let now = Utc::now();
        store.record_task_log("t-a", now, "info", "a1").await.unwrap();
        store.record_task_log("t-b", now, "warn", "b1").await.unwrap();
        store.record_task_log("t-a", now, "info", "a2").await.unwrap();
        let rows = store.load_recent_task_logs(10).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].line, "a1");
        assert_eq!(rows[1].task_id, "t-b");
        assert_eq!(rows[2].line, "a2");
    }

    #[tokio::test]
    async fn recent_respects_limit_returning_newest_window() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let now = Utc::now();
        for i in 0..8 {
            store
                .record_task_log("t-x", now, "info", &format!("line {i}"))
                .await
                .unwrap();
        }
        let rows = store.load_recent_task_logs(3).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].line, "line 5");
        assert_eq!(rows[2].line, "line 7");
    }

    #[tokio::test]
    async fn recent_on_empty_store_returns_empty() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let rows = store.load_recent_task_logs(10).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn load_for_unknown_task_returns_empty() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let rows = store.load_task_logs("nope", 10).await.unwrap();
        assert!(rows.is_empty());
    }
}
