use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::{Attempt, Task, TaskId, TurnMetrics};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;

const SCHEMA: &str = include_str!("../schema.sql");

/// Jaccard similarity between two token sets derived from goal fingerprint strings.
/// Returns a value in [0.0, 1.0] — 1.0 means identical token sets.
fn jaccard_similarity(a: &str, b: &str) -> f32 {
    let tokens_a: HashSet<&str> = a.split_whitespace().collect();
    let tokens_b: HashSet<&str> = b.split_whitespace().collect();
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    // usize→f64 precision loss is acceptable: token-count similarity is a rough heuristic.
    #[allow(clippy::cast_precision_loss)]
    let intersection = tokens_a.intersection(&tokens_b).count() as f64;
    #[allow(clippy::cast_precision_loss)]
    let union = tokens_a.union(&tokens_b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        #[allow(clippy::cast_possible_truncation)]
        let ratio = (intersection / union) as f32;
        ratio
    }
}

/// Build the keyword fingerprint for a goal string.
/// Sorted, deduped tokens longer than 3 characters, lowercased.
fn keyword_fingerprint(goal: &str) -> String {
    let mut words: Vec<String> = goal
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() > 3)
        .map(str::to_lowercase)
        .collect();
    words.sort_unstable();
    words.dedup();
    words.join(" ")
}

/// `SQLite` dual-pool store: one serialising write connection, up to 8 read-only connections.
///
/// `SQLite` supports only one concurrent writer. Using a single-connection write pool
/// ensures `INSERT`/`UPDATE`/`DELETE`/`DDL` statements never contend on the write lock.
/// A separate read-only pool with up to 8 connections allows concurrent `SELECT` queries
/// without blocking or being blocked by writes (WAL mode makes this safe).
#[derive(Clone)]
pub struct MemoryStore {
    /// Single-connection pool — serialises all mutations.
    write_pool: SqlitePool,
    /// Read-only pool — up to 8 concurrent readers.
    read_pool: SqlitePool,
}

impl MemoryStore {
    /// Open or create a persistent `SQLite` database at `path`.
    ///
    /// # Errors
    /// Returns `Err` if the database cannot be created or the schema cannot be applied.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let url = format!("sqlite://{}", path.display());

        // Write pool: single connection, full WAL + synchronous=NORMAL pragmas.
        let write_opts = SqliteConnectOptions::from_str(&url)
            .context("parsing sqlite path (write)")?
            .create_if_missing(true)
            .pragma("journal_mode", "WAL")
            .pragma("synchronous", "NORMAL")
            .pragma("busy_timeout", "5000");
        let write_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(write_opts)
            .await
            .context("opening sqlite write pool")?;

        // Apply schema through the write connection before handing out reads.
        Self::apply_schema(&write_pool).await?;

        // Read pool: up to 8 connections, read-only mode.
        let read_opts = SqliteConnectOptions::from_str(&url)
            .context("parsing sqlite path (read)")?
            .read_only(true)
            .pragma("busy_timeout", "5000");
        let read_pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(read_opts)
            .await
            .context("opening sqlite read pool")?;

        Ok(Self {
            write_pool,
            read_pool,
        })
    }

    /// Open an in-memory `SQLite` database — useful for tests.
    ///
    /// In-memory databases do not support WAL or multiple connections sharing state,
    /// so a single pool services both reads and writes.
    ///
    /// # Errors
    /// Returns `Err` if the in-memory database cannot be opened or the schema cannot be applied.
    pub async fn open_in_memory() -> Result<Self> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .context("parsing in-memory sqlite")?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .context("opening in-memory sqlite pool")?;
        Self::apply_schema(&pool).await?;
        // Use the same pool for both reads and writes — safe for single-connection in-memory DB.
        Ok(Self {
            write_pool: pool.clone(),
            read_pool: pool,
        })
    }

    async fn apply_schema(pool: &SqlitePool) -> Result<()> {
        for stmt in SCHEMA.split(';') {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            let result = sqlx::query(s).execute(pool).await;
            // ALTER TABLE ... ADD COLUMN errors on duplicate columns — silently ignore.
            if let Err(e) = result {
                if !s.to_lowercase().starts_with("alter table") {
                    return Err(e).context("applying schema");
                }
            }
        }
        Ok(())
    }

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
             score_test_pass_rate, score_lint_errors, score_diff_lines, outcome, errors, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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

    /// Jaccard similarity search over stored keyword fingerprints.
    ///
    /// Returns up to 5 patterns most similar to `goal` with Jaccard score > 0.3.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn find_similar_patterns(&self, goal: &str) -> Result<Vec<PatternRow>> {
        let query_fp = keyword_fingerprint(goal);
        if query_fp.is_empty() {
            return Ok(vec![]);
        }

        let all: Vec<PatternRow> = sqlx::query_as::<_, PatternRow>(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, last_seen \
             FROM patterns",
        )
        .fetch_all(&self.read_pool)
        .await?;

        let mut scored: Vec<(f32, PatternRow)> = all
            .into_iter()
            .filter_map(|row| {
                let sim = jaccard_similarity(&query_fp, &row.goal_keywords);
                if sim > 0.3 {
                    Some((sim, row))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(5).map(|(_, r)| r).collect())
    }

    /// Load all patterns ordered by `success_rate` descending.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_patterns(&self, limit: i64) -> Result<Vec<PatternRow>> {
        let rows = sqlx::query_as::<_, PatternRow>(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, last_seen \
             FROM patterns ORDER BY success_rate DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Mine a completed task's attempts into the patterns table.
    ///
    /// # Errors
    /// Returns `Err` if any database query or update fails.
    pub async fn mine_patterns(&self, task_id: &TaskId, goal: &str) -> Result<()> {
        let fingerprint = keyword_fingerprint(goal);
        if fingerprint.is_empty() {
            return Ok(());
        }

        // Reads can go through the read pool.
        let stats: Option<(f64, i64)> = sqlx::query_as(
            "SELECT AVG(score_test_pass_rate), COUNT(*) \
             FROM attempts WHERE task_id = ?1",
        )
        .bind(task_id.0.to_string())
        .fetch_optional(&self.read_pool)
        .await?;

        let (avg_pass, attempt_count) = stats.unwrap_or((0.0, 0));
        let success_rate = avg_pass.clamp(0.0, 1.0);

        let existing: Option<(String, Option<f64>, Option<f64>)> = sqlx::query_as(
            "SELECT id, avg_attempts, success_rate FROM patterns WHERE goal_keywords = ?1",
        )
        .bind(&fingerprint)
        .fetch_optional(&self.read_pool)
        .await?;

        // i64→f64 cast: precision loss is acceptable for attempt count statistics.
        #[allow(clippy::cast_precision_loss)]
        let attempt_count_f = attempt_count as f64;

        let now = Utc::now().to_rfc3339();
        if let Some((existing_id, prev_avg, prev_sr)) = existing {
            let new_avg = f64::midpoint(prev_avg.unwrap_or(0.0), attempt_count_f).max(1.0);
            let new_sr = f64::midpoint(prev_sr.unwrap_or(0.0), success_rate).clamp(0.0, 1.0);
            sqlx::query(
                "UPDATE patterns SET avg_attempts = ?1, success_rate = ?2, last_seen = ?3 WHERE id = ?4",
            )
            .bind(new_avg)
            .bind(new_sr)
            .bind(&now)
            .bind(existing_id)
            .execute(&self.write_pool)
            .await?;
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO patterns (id, goal_keywords, avg_attempts, success_rate, last_seen) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(id)
            .bind(&fingerprint)
            .bind(attempt_count_f)
            .bind(success_rate)
            .bind(&now)
            .execute(&self.write_pool)
            .await?;
        }
        Ok(())
    }

    /// Return the total number of tasks in the database.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn task_count(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tasks")
            .fetch_one(&self.read_pool)
            .await?;
        Ok(row.0)
    }

    /// Persist a single per-turn observability record.
    ///
    /// # Errors
    /// Returns `Err` if the database insert fails.
    pub async fn save_turn_metrics(&self, m: &TurnMetrics) -> Result<()> {
        sqlx::query(
            "INSERT INTO turn_metrics \
             (turn_id, task_id, session_id, model, attempt_number, \
              input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, \
              ttft_ms, turn_latency_ms, tool_execution_ms, \
              context_tokens, context_pressure, evictions, \
              tool_calls, tools_parallel, estimated_cost_usd, timestamp) \
             VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19) \
             ON CONFLICT(turn_id) DO NOTHING",
        )
        .bind(m.turn_id.to_string())
        .bind(m.task_id.0.to_string())
        .bind(m.session_id.to_string())
        .bind(&m.model)
        .bind(i64::from(m.attempt_number))
        .bind(i64::from(m.input_tokens))
        .bind(i64::from(m.output_tokens))
        .bind(i64::from(m.cache_read_input_tokens))
        .bind(i64::from(m.cache_write_input_tokens))
        // u64→i64: latency values are bounded well under i64::MAX in practice.
        .bind(m.ttft_ms.cast_signed())
        .bind(m.turn_latency_ms.cast_signed())
        .bind(m.tool_execution_ms.cast_signed())
        .bind(i64::from(m.context_tokens))
        .bind(f64::from(m.context_pressure))
        .bind(i64::from(m.evictions_this_turn))
        .bind(i64::from(m.tool_calls))
        .bind(i64::from(m.tools_parallel))
        .bind(m.estimated_cost_usd)
        .bind(m.timestamp.to_rfc3339())
        .execute(&self.write_pool)
        .await?;
        Ok(())
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

mod lessons;
pub use lessons::LessonRow;

#[cfg(test)]
mod tests;
