use super::{MemoryStore, TaskRow};
use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::{Attempt, Task, TaskId};

impl MemoryStore {
    /// Save or upsert a task record.
    ///
    /// # Errors
    /// Returns `Err` if serialisation or the database write fails.
    pub async fn save_task(&self, task: &Task, status: &str) -> Result<()> {
        let source = serde_json::to_string(&task.source)?;
        sqlx::query(
            "INSERT INTO tasks (id, goal, status, created_at, source) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(id) DO UPDATE SET status = excluded.status",
        )
        .bind(task.id.0.to_string())
        .bind(&task.goal)
        .bind(status)
        .bind(task.created_at.to_rfc3339())
        .bind(source)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Mark a task as completed with the given status string.
    ///
    /// # Errors
    /// Returns `Err` if the database update fails.
    pub async fn mark_completed(&self, id: &TaskId, status: &str) -> Result<()> {
        sqlx::query("UPDATE tasks SET status = ?1, completed_at = ?2 WHERE id = ?3")
            .bind(status)
            .bind(Utc::now().to_rfc3339())
            .bind(id.0.to_string())
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Persist an agent attempt record.
    ///
    /// # Errors
    /// Returns `Err` if serialisation of errors or the database insert fails.
    pub async fn save_attempt(&self, attempt: &Attempt) -> Result<()> {
        let (pass, lint, diff) = match &attempt.score {
            Some(s) => (
                Some(s.test_pass_rate),
                Some(i64::from(s.lint_errors)),
                Some(i64::from(s.diff_lines)),
            ),
            None => (None, None, None),
        };
        let errors = attempt
            .score
            .as_ref()
            .map(|s| serde_json::to_string(&s.errors).unwrap_or_default())
            .unwrap_or_default();
        sqlx::query(
            "INSERT INTO attempts (id, task_id, attempt_num, branch, \
             score_test_pass_rate, score_lint_errors, score_diff_lines, outcome, errors, \
             created_at, weighted_score) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .bind(attempt.id.to_string())
        .bind(attempt.task_id.0.to_string())
        .bind(i64::from(attempt.attempt_num))
        .bind(&attempt.branch)
        .bind(pass)
        .bind(lint)
        .bind(diff)
        .bind(&attempt.outcome)
        .bind(errors)
        .bind(attempt.created_at.to_rfc3339())
        .bind(attempt.weighted_score.map(f64::from))
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Recent tasks, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_history(&self, limit: i64) -> Result<Vec<TaskRow>> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, goal, status, created_at, completed_at FROM tasks \
             ORDER BY created_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Return goals of the `limit` most recent failed tasks, newest first.
    ///
    /// Used by the self-modify automation to construct the improvement goal string.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn recent_failures(&self, limit: i64) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT goal FROM tasks WHERE status = 'failed' \
             ORDER BY completed_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await
        .context("recent_failures query failed")?;
        Ok(rows.into_iter().map(|(g,)| g).collect())
    }
}
