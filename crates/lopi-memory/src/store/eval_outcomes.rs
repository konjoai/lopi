//! Eval-Execution-1 (A1) — tiered-eval outcome + score-history persistence
//! (cross-cutting seam #4).
//!
//! Mirrors [`super::verifier`] but persists the whole [`EvalOutcome`] (verdict +
//! scalar score + per-check detail + critique), not just a pass bit, so all
//! three consumers can read the same evaluation: A2 reflection (`critique`), A3
//! ratchet (`score`), A3/B1 termination (`verdict` + the score trajectory).

use super::MemoryStore;
use anyhow::Result;
use chrono::Utc;
use lopi_core::EvalOutcome;
use uuid::Uuid;

/// A row from the `eval_outcomes` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvalOutcomeRow {
    /// UUID primary key.
    pub id: String,
    /// Task that was evaluated.
    pub task_id: String,
    /// Attempt number (1-based).
    pub attempt: i64,
    /// `pass` / `fail` / `error` (fail-closed: `error` is not-passing).
    pub verdict: String,
    /// Weighted scalar score in `0..1`.
    pub score: f64,
    /// JSON array of per-check results.
    pub per_check_json: String,
    /// JSON array of critique strings.
    pub critique_json: String,
    /// ISO-8601 timestamp the outcome was recorded.
    pub ts: String,
}

/// One point on a loop's score trajectory (cross-cutting seam #4 read side).
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ScorePoint {
    /// 1-based attempt index.
    pub attempt: i64,
    /// Weighted scalar score at that attempt.
    pub score: f64,
    /// The attempt's verdict string.
    pub verdict: String,
}

impl MemoryStore {
    /// Persist an eval outcome for a task attempt (the write side of seam #4).
    ///
    /// # Errors
    /// Returns `Err` if JSON serialisation or the SQLite write fails.
    pub async fn save_eval_outcome(
        &self,
        task_id: &str,
        attempt: u8,
        outcome: &EvalOutcome,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let per_check = serde_json::to_string(&outcome.per_check)?;
        let critique = serde_json::to_string(&outcome.critique)?;
        let ts = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO eval_outcomes \
             (id, task_id, attempt, verdict, score, per_check_json, critique_json, ts) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(task_id)
        .bind(i64::from(attempt))
        .bind(outcome.verdict.as_str())
        .bind(f64::from(outcome.score))
        .bind(&per_check)
        .bind(&critique)
        .bind(&ts)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Load every eval outcome for a task, oldest first.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn load_eval_outcomes(&self, task_id: &str) -> Result<Vec<EvalOutcomeRow>> {
        let rows = sqlx::query_as::<_, EvalOutcomeRow>(
            "SELECT * FROM eval_outcomes WHERE task_id = ? ORDER BY attempt ASC, ts ASC",
        )
        .bind(task_id)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// The loop's score trajectory — one point per attempt, oldest first. The
    /// queryable progress signal A3's ratchet/no-progress and B1's stack
    /// termination read (seam #4). Previously the raw score rows existed but no
    /// query surfaced the trajectory.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn score_trajectory(&self, task_id: &str) -> Result<Vec<ScorePoint>> {
        let rows = sqlx::query_as::<_, ScorePoint>(
            "SELECT attempt, score, verdict FROM eval_outcomes \
             WHERE task_id = ? ORDER BY attempt ASC, ts ASC",
        )
        .bind(task_id)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use lopi_core::acceptance::EvalTier;
    use lopi_core::{CheckResult, EvalOutcome};

    fn outcome(verdict_fail: bool, score: f32) -> EvalOutcome {
        let check = if verdict_fail {
            CheckResult::fail(
                EvalTier::Judge,
                1.0,
                true,
                vec!["gap".into()],
                vec!["fix it".into()],
            )
        } else {
            CheckResult::pass(EvalTier::ExecutionOk, 1.0, true)
        };
        let mut o = EvalOutcome::aggregate(vec![check]);
        o.score = score;
        o
    }

    #[tokio::test]
    async fn save_and_load_round_trips_the_whole_outcome() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let o = outcome(true, 0.3);
        store.save_eval_outcome("task-1", 1, &o).await.unwrap();
        let rows = store.load_eval_outcomes("task-1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].verdict, "fail");
        assert_eq!(rows[0].attempt, 1);
        assert!((rows[0].score - 0.3).abs() < 1e-6);
        // The per-check + critique detail survives the round-trip.
        let per_check: Vec<CheckResult> = serde_json::from_str(&rows[0].per_check_json).unwrap();
        assert_eq!(per_check.len(), 1);
        let critique: Vec<String> = serde_json::from_str(&rows[0].critique_json).unwrap();
        assert!(critique.contains(&"fix it".to_string()));
    }

    #[tokio::test]
    async fn score_trajectory_is_ordered_by_attempt() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .save_eval_outcome("t", 1, &outcome(true, 0.2))
            .await
            .unwrap();
        store
            .save_eval_outcome("t", 2, &outcome(true, 0.5))
            .await
            .unwrap();
        store
            .save_eval_outcome("t", 3, &outcome(false, 1.0))
            .await
            .unwrap();
        let traj = store.score_trajectory("t").await.unwrap();
        assert_eq!(traj.len(), 3);
        assert_eq!(traj[0].attempt, 1);
        assert!((traj[0].score - 0.2).abs() < 1e-6);
        assert_eq!(traj[2].verdict, "pass");
        // The trajectory is monotonically improving here — exactly the signal
        // A3's ratchet/no-progress will read.
        assert!(traj[0].score < traj[1].score && traj[1].score < traj[2].score);
    }

    #[tokio::test]
    async fn empty_task_has_no_trajectory() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        assert!(store.score_trajectory("nope").await.unwrap().is_empty());
        assert!(store.load_eval_outcomes("nope").await.unwrap().is_empty());
    }
}
