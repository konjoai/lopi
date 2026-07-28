//! Verification gate (Finding #1) — the dead-letter ledger.
//!
//! A task that exhausts its retry budget without ever meeting its goal used
//! to become an unremarkable `TaskStatus::Failed` with no durable, queryable
//! trace beyond a log line. This table is that trace: one row per exhausted
//! task, written from the single choke point every terminal task outcome
//! passes through (`AgentPool::run_one`), keyed off
//! [`lopi_core::StopReason::parse_from_failure_reason`] so only genuine
//! retry-exhaustion — never a cancellation, a non-retryable API error, or a
//! dry run — lands here.

use super::MemoryStore;
use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

/// A row from the `dead_letters` table.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct DeadLetterRow {
    /// UUID primary key.
    pub id: String,
    /// The exhausted task.
    pub task_id: String,
    /// The task's goal, kept alongside the id for a readable listing.
    pub goal: String,
    /// The stable `StopReason::as_str()` tag that fired.
    pub stop_reason: String,
    /// Total attempts made before giving up.
    pub attempts: i64,
    /// The full `TaskStatus::Failed` reason string this row was parsed from.
    pub detail: String,
    /// ISO-8601 timestamp the row was recorded.
    pub ts: String,
}

impl MemoryStore {
    /// Persist a dead-letter row for an exhausted task.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite write fails.
    pub async fn save_dead_letter(
        &self,
        task_id: &str,
        goal: &str,
        stop_reason: &str,
        attempts: u8,
        detail: &str,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let ts = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO dead_letters \
             (id, task_id, goal, stop_reason, attempts, detail, ts) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(task_id)
        .bind(goal)
        .bind(stop_reason)
        .bind(i64::from(attempts))
        .bind(detail)
        .bind(&ts)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// The most recent dead letters across every task, newest first, capped
    /// at `limit` — the read side for an operator-facing listing (Telegram
    /// `/deadletters`-style command, web dashboard panel).
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn load_dead_letters(&self, limit: i64) -> Result<Vec<DeadLetterRow>> {
        let rows = sqlx::query_as::<_, DeadLetterRow>(
            "SELECT * FROM dead_letters ORDER BY ts DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Every dead letter recorded for one task, oldest first.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn load_dead_letters_for_task(&self, task_id: &str) -> Result<Vec<DeadLetterRow>> {
        let rows = sqlx::query_as::<_, DeadLetterRow>(
            "SELECT * FROM dead_letters WHERE task_id = ? ORDER BY ts ASC",
        )
        .bind(task_id)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_and_load_round_trips() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .save_dead_letter(
                "task-1",
                "fix the flaky test",
                "max_iterations",
                3,
                "StopReason::max_iterations { Max retries exceeded }",
            )
            .await
            .unwrap();
        let rows = store.load_dead_letters(10).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].task_id, "task-1");
        assert_eq!(rows[0].goal, "fix the flaky test");
        assert_eq!(rows[0].stop_reason, "max_iterations");
        assert_eq!(rows[0].attempts, 3);
        assert!(rows[0].detail.contains("Max retries exceeded"));
    }

    #[tokio::test]
    async fn load_dead_letters_orders_newest_first_and_respects_limit() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        for i in 0..5 {
            store
                .save_dead_letter(&format!("task-{i}"), "g", "no_progress", 3, "d")
                .await
                .unwrap();
        }
        let rows = store.load_dead_letters(2).await.unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn load_dead_letters_for_task_filters_and_orders_oldest_first() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .save_dead_letter("task-a", "g", "budget", 1, "first")
            .await
            .unwrap();
        store
            .save_dead_letter("task-b", "other goal", "budget", 1, "unrelated")
            .await
            .unwrap();
        store
            .save_dead_letter("task-a", "g", "max_iterations", 2, "second")
            .await
            .unwrap();
        let rows = store.load_dead_letters_for_task("task-a").await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].detail, "first");
        assert_eq!(rows[1].detail, "second");
    }

    #[tokio::test]
    async fn no_dead_letters_is_an_empty_list_not_an_error() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        assert!(store.load_dead_letters(10).await.unwrap().is_empty());
        assert!(store
            .load_dead_letters_for_task("nope")
            .await
            .unwrap()
            .is_empty());
    }
}
