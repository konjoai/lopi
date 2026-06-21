//! Loop-health projections — Phase 16.3 (Loop Engineering observability).
//!
//! Read-only aggregations over data the agent loop already persists
//! (`attempts`, `turn_metrics`, `verifier_verdicts`) projected into the shapes
//! the Loop Health dashboard renders: per-attempt score series, outcome
//! distribution, token/cost burn, and verifier pass rate. No new write paths —
//! this is pure observability over existing truth.

use anyhow::Result;

use super::MemoryStore;

/// One attempt row projected for the loop-health score/diff timelines.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoopAttemptRow {
    /// Task this attempt belongs to (groups attempts into a single run).
    pub task_id: String,
    /// 1-based attempt index within the task's retry loop.
    pub attempt_num: i64,
    /// Fraction of tests passing (0.0–1.0), if scored.
    pub test_pass_rate: Option<f64>,
    /// Lint error count at this attempt, if scored.
    pub lint_errors: Option<i64>,
    /// Net diff size in lines at this attempt, if scored.
    pub diff_lines: Option<i64>,
    /// Outcome tag: `success` / `retry` / etc.
    pub outcome: String,
    /// ISO-8601 timestamp the attempt was recorded.
    pub created_at: String,
}

/// One turn's token/cost/pressure sample for the burn charts.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoopTurnRow {
    /// Estimated USD cost of the turn.
    pub estimated_cost_usd: f64,
    /// Input (prompt) tokens consumed.
    pub input_tokens: i64,
    /// Output (completion) tokens produced.
    pub output_tokens: i64,
    /// Context-window pressure 0.0–1.0 at the turn.
    pub context_pressure: f64,
    /// ISO-8601 timestamp of the turn.
    pub timestamp: String,
}

impl MemoryStore {
    /// Most recent attempts across all tasks, newest first.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn recent_loop_attempts(&self, limit: i64) -> Result<Vec<LoopAttemptRow>> {
        let rows = sqlx::query_as::<_, LoopAttemptRow>(
            "SELECT task_id, attempt_num, score_test_pass_rate AS test_pass_rate, \
             score_lint_errors AS lint_errors, score_diff_lines AS diff_lines, \
             outcome, created_at \
             FROM attempts ORDER BY created_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Attempt counts grouped by outcome tag (`success`, `retry`, …).
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn loop_outcome_counts(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT outcome, COUNT(*) FROM attempts GROUP BY outcome ORDER BY COUNT(*) DESC",
        )
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Most recent turn metrics, newest first, for the cost/token/pressure burn charts.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn recent_turn_metrics(&self, limit: i64) -> Result<Vec<LoopTurnRow>> {
        let rows = sqlx::query_as::<_, LoopTurnRow>(
            "SELECT estimated_cost_usd, input_tokens, output_tokens, \
             context_pressure, timestamp \
             FROM turn_metrics ORDER BY timestamp DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Verifier pass rate as `(passed, total)` across all recorded verdicts.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn verifier_pass_rate(&self) -> Result<(i64, i64)> {
        let row = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COALESCE(SUM(passed), 0), COUNT(*) FROM verifier_verdicts",
        )
        .fetch_one(&self.read_pool)
        .await?;
        Ok(row)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::store::MemoryStore;
    use lopi_core::{Attempt, Score, Task, TaskId};

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    /// Seed a task row so attempts satisfy the `attempts.task_id` foreign key.
    async fn seed_task(s: &MemoryStore) -> TaskId {
        let task = Task::new("loop health fixture");
        s.save_task(&task, "queued").await.unwrap();
        task.id
    }

    fn attempt(task: TaskId, n: u8, pass: f32, outcome: &str) -> Attempt {
        let mut a = Attempt::new(task, n, format!("br-{n}"));
        a.score = Some(Score {
            test_pass_rate: pass,
            lint_errors: 0,
            diff_lines: 10,
            errors: vec![],
        });
        a.outcome = outcome.into();
        a
    }

    #[tokio::test]
    async fn recent_attempts_are_newest_first_and_limited() {
        let s = store().await;
        let task = seed_task(&s).await;
        for n in 1..=3u8 {
            s.save_attempt(&attempt(task, n, 0.5, "retry"))
                .await
                .unwrap();
        }
        let rows = s.recent_loop_attempts(2).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].task_id, task.to_string());
        assert_eq!(rows[0].test_pass_rate, Some(0.5));
    }

    #[tokio::test]
    async fn outcome_counts_group_and_order() {
        let s = store().await;
        let task = seed_task(&s).await;
        s.save_attempt(&attempt(task, 1, 0.4, "retry"))
            .await
            .unwrap();
        s.save_attempt(&attempt(task, 2, 0.6, "retry"))
            .await
            .unwrap();
        s.save_attempt(&attempt(task, 3, 1.0, "success"))
            .await
            .unwrap();
        let counts = s.loop_outcome_counts().await.unwrap();
        // "retry" (2) should sort before "success" (1).
        assert_eq!(counts[0], ("retry".to_string(), 2));
        assert_eq!(counts[1], ("success".to_string(), 1));
    }

    #[tokio::test]
    async fn verifier_pass_rate_handles_empty() {
        let s = store().await;
        assert_eq!(s.verifier_pass_rate().await.unwrap(), (0, 0));
    }

    #[tokio::test]
    async fn turn_metrics_empty_is_ok() {
        let s = store().await;
        assert!(s.recent_turn_metrics(10).await.unwrap().is_empty());
    }
}
