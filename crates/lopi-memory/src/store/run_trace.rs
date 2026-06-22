//! Per-run drill-down projections — Phase 16.4 (Loop Engineering observability).
//!
//! The single-run counterpart to [`loop_health`](super::loop_health): given a
//! task (one autonomous "run"), reconstruct its attempt-by-attempt trace from
//! the `attempts`, `turn_metrics`, and `verifier_verdicts` tables. `recent_runs`
//! lists runs for the picker; `run_attempts` + `run_turn_aggregates` (joined with
//! `load_verifier_verdicts`) build the detail timeline. Read-only — no new writes.

use anyhow::Result;

use super::MemoryStore;

/// One run (task) summarised for the run picker.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoopRunRow {
    /// Task id (the run identifier).
    pub task_id: String,
    /// Human-readable goal.
    pub goal: String,
    /// Current task status string.
    pub status: String,
    /// Number of attempts recorded for the run.
    pub attempts: i64,
    /// Best test-pass-rate across the run's attempts, if any were scored.
    pub best_score: Option<f64>,
    /// Outcome tag of the latest attempt (`success` / `retry` / `stalled` / …).
    pub final_outcome: String,
    /// ISO-8601 timestamp of the most recent attempt.
    pub last_at: String,
}

/// One attempt within a run's trace.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunAttemptRow {
    /// 1-based attempt index.
    pub attempt_num: i64,
    /// Fraction of tests passing (0.0–1.0), if scored.
    pub test_pass_rate: Option<f64>,
    /// Lint error count, if scored.
    pub lint_errors: Option<i64>,
    /// Net diff size in lines, if scored.
    pub diff_lines: Option<i64>,
    /// Outcome tag for the attempt.
    pub outcome: String,
    /// JSON array of error strings captured during scoring, if any.
    pub errors: Option<String>,
    /// ISO-8601 timestamp the attempt was recorded.
    pub created_at: String,
}

/// Token/cost totals for one attempt, summed over its turns.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunTurnAgg {
    /// Attempt number the turns belong to.
    pub attempt_number: i64,
    /// Total input + output tokens across the attempt's turns.
    pub tokens: i64,
    /// Total estimated USD cost across the attempt's turns.
    pub cost_usd: f64,
}

impl MemoryStore {
    /// Recent runs (tasks with at least one attempt), newest activity first.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn recent_runs(&self, limit: i64) -> Result<Vec<LoopRunRow>> {
        let rows = sqlx::query_as::<_, LoopRunRow>(
            "SELECT a.task_id AS task_id, t.goal AS goal, t.status AS status, \
             COUNT(*) AS attempts, MAX(a.score_test_pass_rate) AS best_score, \
             (SELECT outcome FROM attempts a2 WHERE a2.task_id = a.task_id \
              ORDER BY a2.attempt_num DESC LIMIT 1) AS final_outcome, \
             MAX(a.created_at) AS last_at \
             FROM attempts a JOIN tasks t ON t.id = a.task_id \
             GROUP BY a.task_id, t.goal, t.status \
             ORDER BY last_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// All attempts for a run, in attempt order (oldest first).
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn run_attempts(&self, task_id: &str) -> Result<Vec<RunAttemptRow>> {
        let rows = sqlx::query_as::<_, RunAttemptRow>(
            "SELECT attempt_num, score_test_pass_rate AS test_pass_rate, \
             score_lint_errors AS lint_errors, score_diff_lines AS diff_lines, \
             outcome, errors, created_at \
             FROM attempts WHERE task_id = ?1 ORDER BY attempt_num ASC",
        )
        .bind(task_id)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Per-attempt token/cost totals for a run.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn run_turn_aggregates(&self, task_id: &str) -> Result<Vec<RunTurnAgg>> {
        let rows = sqlx::query_as::<_, RunTurnAgg>(
            "SELECT attempt_number, \
             COALESCE(SUM(input_tokens + output_tokens), 0) AS tokens, \
             COALESCE(SUM(estimated_cost_usd), 0) AS cost_usd \
             FROM turn_metrics WHERE task_id = ?1 GROUP BY attempt_number",
        )
        .bind(task_id)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Goal + status for a run, or `None` if the task is unknown.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn run_task_meta(&self, task_id: &str) -> Result<Option<(String, String)>> {
        let row =
            sqlx::query_as::<_, (String, String)>("SELECT goal, status FROM tasks WHERE id = ?1")
                .bind(task_id)
                .fetch_optional(&self.read_pool)
                .await?;
        Ok(row)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::store::MemoryStore;
    use chrono::Utc;
    use lopi_core::{Attempt, Score, Task, TaskId, TurnMetrics};
    use uuid::Uuid;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    /// A turn-metrics row for `task`/`attempt_no` with the given token + cost figures.
    fn turn(task: TaskId, attempt_no: u8, input: u32, cost: f64) -> TurnMetrics {
        TurnMetrics {
            turn_id: Uuid::new_v4(),
            task_id: task,
            session_id: Uuid::new_v4(),
            model: "claude-sonnet-4-6".into(),
            attempt_number: attempt_no,
            input_tokens: input,
            output_tokens: 50,
            cache_read_input_tokens: 0,
            cache_write_input_tokens: 0,
            ttft_ms: 0,
            turn_latency_ms: 0,
            tool_execution_ms: 0,
            context_tokens: 0,
            context_pressure: 0.2,
            evictions_this_turn: 0,
            tool_calls: 0,
            tools_parallel: false,
            estimated_cost_usd: cost,
            timestamp: Utc::now(),
        }
    }

    async fn seed_task(s: &MemoryStore, goal: &str) -> TaskId {
        let task = Task::new(goal);
        s.save_task(&task, "queued").await.unwrap();
        task.id
    }

    fn attempt(task: TaskId, n: u8, pass: f32, outcome: &str) -> Attempt {
        let mut a = Attempt::new(task, n, format!("br-{n}"));
        a.score = Some(Score {
            test_pass_rate: pass,
            lint_errors: 1,
            diff_lines: 20,
            errors: vec!["boom".into()],
        });
        a.outcome = outcome.into();
        a
    }

    #[tokio::test]
    async fn recent_runs_summarise_attempts() {
        let s = store().await;
        let task = seed_task(&s, "tighten retry backoff").await;
        s.save_attempt(&attempt(task, 1, 0.4, "retry"))
            .await
            .unwrap();
        s.save_attempt(&attempt(task, 2, 0.9, "success"))
            .await
            .unwrap();

        let runs = s.recent_runs(10).await.unwrap();
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!(r.task_id, task.to_string());
        assert_eq!(r.goal, "tighten retry backoff");
        assert_eq!(r.attempts, 2);
        // f32 0.9 widens to f64 imprecisely — compare with tolerance.
        assert!((r.best_score.unwrap() - 0.9).abs() < 1e-6);
        // final_outcome is the latest attempt's outcome.
        assert_eq!(r.final_outcome, "success");
    }

    #[tokio::test]
    async fn run_attempts_are_ordered_and_carry_errors() {
        let s = store().await;
        let task = seed_task(&s, "fix flaky scorer").await;
        s.save_attempt(&attempt(task, 2, 0.6, "retry"))
            .await
            .unwrap();
        s.save_attempt(&attempt(task, 1, 0.3, "retry"))
            .await
            .unwrap();

        let rows = s.run_attempts(&task.to_string()).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].attempt_num, 1);
        assert_eq!(rows[1].attempt_num, 2);
        assert!(rows[0].errors.as_deref().unwrap().contains("boom"));
    }

    #[tokio::test]
    async fn turn_aggregates_sum_tokens_and_cost_per_attempt() {
        let s = store().await;
        let task = seed_task(&s, "measure burn").await;
        for (i, attempt_no) in [1u8, 1, 2].iter().enumerate() {
            s.save_turn_metrics(&turn(task, *attempt_no, 100 + i as u32, 0.01))
                .await
                .unwrap();
        }
        let aggs = s.run_turn_aggregates(&task.to_string()).await.unwrap();
        let a1 = aggs.iter().find(|a| a.attempt_number == 1).unwrap();
        // attempt 1 has two turns: tokens (100+50)+(101+50)=301, cost 0.02.
        assert_eq!(a1.tokens, 301);
        assert!((a1.cost_usd - 0.02).abs() < 1e-9);
    }

    #[tokio::test]
    async fn task_meta_returns_goal_and_status() {
        let s = store().await;
        let task = seed_task(&s, "anchor the intent").await;
        let (goal, status) = s.run_task_meta(&task.to_string()).await.unwrap().unwrap();
        assert_eq!(goal, "anchor the intent");
        assert_eq!(status, "queued");
        assert!(s.run_task_meta("nope").await.unwrap().is_none());
    }
}
