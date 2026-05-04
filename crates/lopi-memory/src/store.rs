use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::{Attempt, Task, TaskId};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteConnectOptions};
use std::path::Path;
use std::str::FromStr;

const SCHEMA: &str = include_str!("schema.sql");

#[derive(Clone)]
pub struct MemoryStore {
    pool: SqlitePool,
}

impl MemoryStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
            .context("parsing sqlite path")?
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await
            .context("opening sqlite pool")?;
        sqlx::query(SCHEMA).execute(&pool).await.ok();
        // sqlx::query requires a single statement; fall back to splitting on ";" for multi-statement schema.
        for stmt in SCHEMA.split(';') {
            let s = stmt.trim();
            if s.is_empty() { continue; }
            sqlx::query(s).execute(&pool).await.context("applying schema")?;
        }
        Ok(Self { pool })
    }

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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_completed(&self, id: &TaskId, status: &str) -> Result<()> {
        sqlx::query(
            "UPDATE tasks SET status = ?1, completed_at = ?2 WHERE id = ?3",
        )
        .bind(status)
        .bind(Utc::now().to_rfc3339())
        .bind(id.0.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn save_attempt(&self, attempt: &Attempt) -> Result<()> {
        let (pass, lint, diff) = match &attempt.score {
            Some(s) => (Some(s.test_pass_rate), Some(s.lint_errors as i64), Some(s.diff_lines as i64)),
            None => (None, None, None),
        };
        let errors = attempt.score.as_ref()
            .map(|s| serde_json::to_string(&s.errors).unwrap_or_default())
            .unwrap_or_default();
        sqlx::query(
            "INSERT INTO attempts (id, task_id, attempt_num, branch, \
             score_test_pass_rate, score_lint_errors, score_diff_lines, outcome, errors, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(attempt.id.to_string())
        .bind(attempt.task_id.0.to_string())
        .bind(attempt.attempt_num as i64)
        .bind(&attempt.branch)
        .bind(pass)
        .bind(lint)
        .bind(diff)
        .bind(&attempt.outcome)
        .bind(errors)
        .bind(attempt.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Recent tasks, newest first.
    pub async fn load_history(&self, limit: i64) -> Result<Vec<TaskRow>> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, goal, status, created_at, completed_at FROM tasks \
             ORDER BY created_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Crude similarity search: matches any keyword token in `goal`.
    pub async fn find_similar_patterns(&self, goal: &str) -> Result<Vec<PatternRow>> {
        let mut rows: Vec<PatternRow> = vec![];
        for kw in goal.split_whitespace().filter(|w| w.len() > 3).take(5) {
            let like = format!("%{}%", kw.to_lowercase());
            let mut hits: Vec<PatternRow> = sqlx::query_as::<_, PatternRow>(
                "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, last_seen \
                 FROM patterns WHERE lower(goal_keywords) LIKE ?1 LIMIT 5",
            )
            .bind(like)
            .fetch_all(&self.pool)
            .await?;
            rows.append(&mut hits);
        }
        Ok(rows)
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct TaskRow {
    pub id: String,
    pub goal: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PatternRow {
    pub id: String,
    pub goal_keywords: String,
    pub successful_constraints: Option<String>,
    pub avg_attempts: Option<f64>,
    pub success_rate: Option<f64>,
    pub last_seen: String,
}
