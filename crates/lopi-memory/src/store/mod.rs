use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::{Attempt, Task, TaskId, TurnMetrics};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

const SCHEMA: &str = include_str!("../schema.sql");

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

    /// Open an isolated per-customer database.
    ///
    /// Creates `{base_dir}/{customer_id}/lopi.db` — each customer gets a
    /// separate SQLite file so pattern stores, lessons, and quality runs
    /// cannot bleed across tenants.
    ///
    /// # Errors
    /// Returns `Err` if the directory cannot be created or the database cannot be opened.
    pub async fn open_for_customer(base_dir: impl AsRef<Path>, customer_id: &str) -> Result<Self> {
        // Sanitise: only alphanumeric + hyphen/underscore allowed in customer_id.
        let safe_id: String = customer_id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let db_path = base_dir.as_ref().join(&safe_id).join("lopi.db");
        Self::open(db_path).await
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
            // Strip leading SQL line comments (`-- foo`) so the prefix check
            // below correctly identifies ALTER TABLE statements that have
            // documentation comments above them.
            let body: String = s
                .lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            if body.is_empty() {
                continue;
            }

            let result = sqlx::query(&body).execute(pool).await;
            // ALTER TABLE ... ADD COLUMN errors on duplicate columns — silently ignore.
            if let Err(e) = result {
                if !body.to_lowercase().starts_with("alter table") {
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
            "INSERT INTO tasks (id, goal, status, created_at, source, client_ref) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(id) DO UPDATE SET status = excluded.status",
        )
        .bind(task.id.0.to_string())
        .bind(&task.goal)
        .bind(status)
        .bind(task.created_at.to_rfc3339())
        .bind(source)
        .bind(&task.client_ref)
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

    /// Transition a task's durable status to `running` when execution begins.
    ///
    /// Deliberately does not touch `completed_at` — the row is still in flight.
    /// This is the single persisted "in flight" marker: without it the row
    /// stays `queued` for the whole run, so a fresh dashboard load (which only
    /// has the snapshot to read) mis-reports a running task as queued.
    ///
    /// # Errors
    /// Returns `Err` if the database update fails.
    pub async fn mark_running(&self, id: &TaskId) -> Result<()> {
        sqlx::query("UPDATE tasks SET status = 'running' WHERE id = ?1")
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

    /// Permanently remove a task and every row that references it (attempts,
    /// turn metrics, agent checkpoints, dead-letter entries, task logs,
    /// verifier verdicts). Lessons keep their content but lose the back-
    /// reference. Returns `true` when the task row existed and was deleted.
    ///
    /// Used by the dashboard's per-pane close (`✕`) so a dismissed session
    /// stays dismissed across reloads instead of repopulating from the
    /// snapshot on the next WebSocket connect.
    ///
    /// # Errors
    /// Returns `Err` if any of the cascading writes fails.
    pub async fn delete_task(&self, id: &TaskId) -> Result<bool> {
        let id_str = id.0.to_string();
        let mut tx = self.write_pool.begin().await?;
        for table in [
            "attempts",
            "turn_metrics",
            "agent_checkpoints",
            "task_logs",
            "verifier_verdicts",
            "eval_outcomes",
        ] {
            let sql = format!("DELETE FROM {table} WHERE task_id = ?1");
            sqlx::query(&sql).bind(&id_str).execute(&mut *tx).await?;
        }
        // Preserve lessons (they encode reusable insight) but sever the link
        // so the deleted task can't be re-derived from them.
        sqlx::query("UPDATE lessons SET task_id = NULL WHERE task_id = ?1")
            .bind(&id_str)
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM tasks WHERE id = ?1")
            .bind(&id_str)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    /// Fetch a single task row by id.
    ///
    /// Stack-Chain-1 — used by `ChainScheduleManager`'s boot-time resume to
    /// tell whether a step's task actually reached a terminal state before
    /// the process restarted (in which case the chain advances) or was still
    /// in flight and lost with the old process's in-memory queue (in which
    /// case the step must be resubmitted).
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn get_task(&self, id: &TaskId) -> Result<Option<TaskRow>> {
        let row = sqlx::query_as::<_, TaskRow>(
            "SELECT id, goal, status, created_at, completed_at, client_ref FROM tasks WHERE id = ?1",
        )
        .bind(id.0.to_string())
        .fetch_optional(&self.read_pool)
        .await?;
        Ok(row)
    }

    /// Recent tasks, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_history(&self, limit: i64) -> Result<Vec<TaskRow>> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, goal, status, created_at, completed_at, client_ref FROM tasks \
             ORDER BY created_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
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

    /// Count tasks by lifecycle bucket, from the durable store.
    ///
    /// The source of truth for `/api/stats`' `running`/`queued`/`succeeded`/
    /// `failed` counts. Reads the DB rather than a pool's in-memory counters
    /// because those counters are **per-pool**: in multi-repo mode each extra
    /// repo runs its own pool, so the primary pool's counters miss every task
    /// dispatched to another repo (Verify-1 F3/F4 — the topbar "N live" showed
    /// 1 while 2 agents ran, and `succeeded` read 3 against 7 real). The DB is
    /// shared across all pools, so its counts are exact regardless of repo
    /// count. Status matching is prefix-based to tolerate any legacy decorated
    /// rows a pre-Fix-1 write may have left behind.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn status_counts(&self) -> Result<TaskStatusCounts> {
        let rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT status, COUNT(*) FROM tasks GROUP BY status")
                .fetch_all(&self.read_pool)
                .await?;
        let mut counts = TaskStatusCounts::default();
        for (status, n) in rows {
            let n = usize::try_from(n).unwrap_or(0);
            match status.as_str() {
                s if s.starts_with("running") => counts.running += n,
                s if s.starts_with("queued") => counts.queued += n,
                s if s.starts_with("success") => counts.succeeded += n,
                s if s.starts_with("failed") => counts.failed += n,
                _ => {}
            }
        }
        Ok(counts)
    }

    /// Whether a task with exactly this id exists in the store.
    ///
    /// Lets id-scoped read endpoints (`/logs`, `/stream`, agent `/dag`) tell a
    /// *bogus* id (→ 404) apart from a *known* task that simply has no rows yet
    /// (→ valid empty 200) — the distinction Ops-2 #8 / Verify-1 F8 asked for.
    /// Safe against a create race: `AgentPool::submit` persists the task
    /// (`save_task(.., "queued")`) before `create_task` returns its id.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn task_exists(&self, id: &str) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM tasks WHERE id = ?1 LIMIT 1")
            .bind(id)
            .fetch_optional(&self.read_pool)
            .await?;
        Ok(row.is_some())
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

/// Task counts by lifecycle bucket, returned by [`MemoryStore::status_counts`].
///
/// Computed from the shared durable store so the totals are correct across
/// every repo/pool (see the method's docs for the multi-repo undercount this
/// avoids).
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskStatusCounts {
    /// Tasks currently executing.
    pub running: usize,
    /// Tasks queued but not yet started.
    pub queued: usize,
    /// Tasks that reached a successful terminal state.
    pub succeeded: usize,
    /// Tasks that reached a failed terminal state.
    pub failed: usize,
}

/// Flat view of a task record returned by [`MemoryStore::load_history`].
#[derive(Debug, sqlx::FromRow)]
pub struct TaskRow {
    /// Stringified UUID — primary key matching the `tasks` table.
    pub id: String,
    /// Human-readable goal text submitted with the task.
    pub goal: String,
    /// Current lifecycle status string (e.g. `"pending"`, `"done"`, `"failed"`).
    pub status: String,
    /// ISO-8601 timestamp when the task was created.
    pub created_at: String,
    /// ISO-8601 timestamp when the task reached a terminal state, if any.
    pub completed_at: Option<String>,
    /// Backend-1 — the caller-supplied [`lopi_core::Task::client_ref`], if any.
    pub client_ref: Option<String>,
}

mod audit;
mod checkpoints;
mod dag;
mod eval_outcomes;
mod installations;
mod learnings;
mod lessons;
mod loop_health;
mod maxx;
mod patterns;
mod q_routing;
mod quality;
mod quota;
mod result_cache;
mod run_trace;
mod schedule_chains;
mod schedules;
mod stability;
mod task_logs;
mod trust_ledger;
mod verifier;
// Re-export helpers for tests (tests.rs uses `use super::*`).
pub use audit::{AuditInput, AuditQuery, AuditRow};
pub use checkpoints::{CheckpointInput, CheckpointRow};
pub use dag::DagNodeRow;
pub use eval_outcomes::{EvalOutcomeRow, ScorePoint};
pub use installations::InstallationRow;
pub use learnings::LearningRow;
pub use lessons::LessonRow;
pub use loop_health::{LoopAttemptRow, LoopTurnRow};
pub use maxx::{MaxxInput, MaxxRow, MaxxRunRow};
pub use patterns::{jaccard_similarity, keyword_fingerprint, PatternRow};
pub use q_routing::RoutingQValueRow;
pub use quality::{QualityRunRecord, QualityRunRow};
pub use quota::QuotaObservationRow;
pub use result_cache::{compute_key as compute_cache_key, CacheStats, CachedResult};
pub use run_trace::{LoopRunRow, RunAttemptRow, RunTurnAgg};
pub use schedule_chains::{
    ChainRunRow, ChainStepInput, ChainStepRow, ScheduleChainInput, ScheduleChainRow,
};
pub use schedules::{ScheduleInput, ScheduleRow, ScheduleRunRow};
pub use stability::{StabilityEntry, StabilityRecord};
pub use task_logs::{TaskLogRow, MAX_PER_TASK as TASK_LOG_MAX_PER_TASK};
pub use trust_ledger::TrustLedgerRow;
pub use verifier::VerifierVerdictRow;

#[cfg(test)]
mod lessons_tests;
#[cfg(test)]
mod tests;
