//! Durable whole-stack cron chain storage.
//!
//! Sibling to [`super::schedules`], but a `schedules` row models exactly one
//! goal — it has no way to represent a *sequence* of independent goals. A
//! `schedule_chains` row is the cron header (mirroring `schedules`' columns);
//! its ordered steps live in `schedule_chain_steps` (one row per stack card);
//! and every fire (cron tick or manual run-now) gets a `schedule_chain_runs`
//! row tracking which step is currently in flight. That last table is what
//! lets `ChainScheduleManager` resume a chain after a backend restart instead
//! of restarting from step 1 or silently dropping the rest of the chain.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row as _;
use uuid::Uuid;

use super::MemoryStore;

/// One ordered step of a chain — the durable shape of a single stack card.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChainStepRow {
    /// Position in the chain, 0-based.
    pub step_order: i64,
    /// Agent goal submitted when this step fires.
    pub goal: String,
    /// Directories the agent may touch (overrides global config when non-empty).
    pub allowed_dirs: Vec<String>,
    /// Directories the agent must not touch.
    pub forbidden_dirs: Vec<String>,
}

/// Input shape for a single step, used when building a [`ScheduleChainInput`].
#[derive(Debug, Clone)]
pub struct ChainStepInput {
    /// Agent goal submitted when this step fires.
    pub goal: String,
    /// Allowed directories.
    pub allowed_dirs: Vec<String>,
    /// Forbidden directories.
    pub forbidden_dirs: Vec<String>,
}

/// One persisted chain, with its steps loaded in order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleChainRow {
    /// Row-level UUID — primary key, stable across edits.
    pub id: String,
    /// Human-readable name shown in the UI.
    pub name: String,
    /// Standard 5-field cron expression, e.g. `"0 2 * * *"`.
    pub cron: String,
    /// Repo the chain targets, if any.
    pub repo: Option<String>,
    /// Priority string: `low` / `normal` / `high` / `critical`.
    pub priority: String,
    /// Trust level (L1-L4) governing how far each step may act without a human.
    pub autonomy_level: String,
    /// Policy applied when a step fails: `stop` / `continue` / `backoff`.
    /// Mirrors the client-side `OnFail` type (`web/src/lib/stores/stack.ts`).
    pub on_fail: String,
    /// When `false` the chain is persisted but not registered as a live job.
    pub enabled: bool,
    /// Ordered steps, one per stack card.
    pub steps: Vec<ChainStepRow>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 timestamp of the last edit.
    pub updated_at: String,
}

/// Input to [`MemoryStore::upsert_schedule_chain`]. `id == None` creates a new
/// chain; `id == Some(_)` replaces the matching chain's header and steps.
#[derive(Debug, Clone)]
pub struct ScheduleChainInput {
    /// Existing chain id to update, or `None` to insert a fresh chain.
    pub id: Option<String>,
    /// Human-readable name.
    pub name: String,
    /// Cron expression (validated by the caller before persisting).
    pub cron: String,
    /// Target repo path, if any.
    pub repo: Option<String>,
    /// Priority string.
    pub priority: String,
    /// Trust level tag.
    pub autonomy_level: String,
    /// On-fail policy tag.
    pub on_fail: String,
    /// Whether the chain should be live.
    pub enabled: bool,
    /// Ordered steps — replaces any existing steps for this chain.
    pub steps: Vec<ChainStepInput>,
}

/// One fire attempt of a chain — tracks which step is currently in flight so
/// a restart can resume rather than replay from the start.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChainRunRow {
    /// Run-level UUID — primary key.
    pub id: String,
    /// Owning chain id.
    pub chain_id: String,
    /// When the chain fired (ISO-8601).
    pub fired_at: String,
    /// 0-based index of the step currently (or most recently) in flight.
    pub current_step: i64,
    /// Task id submitted for the current step, if one has been queued.
    pub current_task_id: Option<String>,
    /// `running` / `completed` / `failed`.
    pub status: String,
    /// ISO-8601 timestamp of the last update to this run.
    pub updated_at: String,
}

impl MemoryStore {
    /// List every chain, newest first, with steps loaded in order.
    ///
    /// # Errors
    /// Returns `Err` if either query fails.
    pub async fn list_schedule_chains(&self) -> Result<Vec<ScheduleChainRow>> {
        let sql = format!("{CHAIN_SELECT_COLS} ORDER BY created_at DESC");
        let rows = sqlx::query(&sql)
            .fetch_all(&self.read_pool)
            .await
            .context("listing schedule chains")?;
        let mut chains = Vec::with_capacity(rows.len());
        for row in rows {
            let mut chain = chain_from_row(row);
            chain.steps = self.list_chain_steps(&chain.id).await?;
            chains.push(chain);
        }
        Ok(chains)
    }

    /// Fetch a single chain by id, with its steps loaded in order.
    ///
    /// # Errors
    /// Returns `Err` if either query fails.
    pub async fn get_schedule_chain(&self, id: &str) -> Result<Option<ScheduleChainRow>> {
        let sql = format!("{CHAIN_SELECT_COLS} WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.read_pool)
            .await
            .context("fetching schedule chain")?;
        let Some(row) = row else { return Ok(None) };
        let mut chain = chain_from_row(row);
        chain.steps = self.list_chain_steps(&chain.id).await?;
        Ok(Some(chain))
    }

    async fn list_chain_steps(&self, chain_id: &str) -> Result<Vec<ChainStepRow>> {
        let rows = sqlx::query(
            "SELECT step_order, goal, allowed_dirs, forbidden_dirs \
             FROM schedule_chain_steps WHERE chain_id = ? ORDER BY step_order ASC",
        )
        .bind(chain_id)
        .fetch_all(&self.read_pool)
        .await
        .context("listing chain steps")?;
        Ok(rows.into_iter().map(step_from_row).collect())
    }

    /// Insert (when `input.id` is `None`) or replace an existing chain's
    /// header and steps. Steps are always fully replaced — there is no
    /// partial-step edit — so an edit that reorders or drops cards is a
    /// single consistent write.
    ///
    /// # Errors
    /// Returns `Err` if JSON serialisation or the write fails.
    pub async fn upsert_schedule_chain(
        &self,
        input: &ScheduleChainInput,
    ) -> Result<ScheduleChainRow> {
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = Utc::now().to_rfc3339();
        let autonomy = normalize_autonomy(&input.autonomy_level);
        let on_fail = normalize_on_fail(&input.on_fail);

        let mut tx = self.write_pool.begin().await?;
        sqlx::query(
            "INSERT INTO schedule_chains
               (id, name, cron, repo, priority, autonomy_level, on_fail, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name, cron = excluded.cron, repo = excluded.repo,
               priority = excluded.priority, autonomy_level = excluded.autonomy_level,
               on_fail = excluded.on_fail, enabled = excluded.enabled,
               updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.cron)
        .bind(&input.repo)
        .bind(&input.priority)
        .bind(&autonomy)
        .bind(&on_fail)
        .bind(i64::from(input.enabled))
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .context("upserting schedule chain")?;

        sqlx::query("DELETE FROM schedule_chain_steps WHERE chain_id = ?")
            .bind(&id)
            .execute(&mut *tx)
            .await
            .context("clearing chain steps")?;
        for (order, step) in input.steps.iter().enumerate() {
            let allowed = serde_json::to_string(&step.allowed_dirs)?;
            let forbidden = serde_json::to_string(&step.forbidden_dirs)?;
            sqlx::query(
                "INSERT INTO schedule_chain_steps
                   (chain_id, step_order, goal, allowed_dirs, forbidden_dirs)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(i64::try_from(order).unwrap_or(i64::MAX))
            .bind(&step.goal)
            .bind(&allowed)
            .bind(&forbidden)
            .execute(&mut *tx)
            .await
            .context("inserting chain step")?;
        }
        tx.commit().await?;

        self.get_schedule_chain(&id)
            .await?
            .context("chain vanished after upsert")
    }

    /// Toggle a chain's `enabled` flag. Returns `true` when a row matched.
    ///
    /// # Errors
    /// Returns `Err` if the write fails.
    pub async fn set_schedule_chain_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let res =
            sqlx::query("UPDATE schedule_chains SET enabled = ?, updated_at = ? WHERE id = ?")
                .bind(i64::from(enabled))
                .bind(Utc::now().to_rfc3339())
                .bind(id)
                .execute(&self.write_pool)
                .await
                .context("setting chain enabled")?;
        Ok(res.rows_affected() > 0)
    }

    /// Permanently delete a chain, its steps, and its run history.
    /// Returns `true` when the chain existed.
    ///
    /// # Errors
    /// Returns `Err` if any of the cascading deletes fail.
    pub async fn delete_schedule_chain(&self, id: &str) -> Result<bool> {
        let mut tx = self.write_pool.begin().await?;
        sqlx::query("DELETE FROM schedule_chain_runs WHERE chain_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("deleting chain runs")?;
        sqlx::query("DELETE FROM schedule_chain_steps WHERE chain_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("deleting chain steps")?;
        let res = sqlx::query("DELETE FROM schedule_chains WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("deleting chain")?;
        tx.commit().await?;
        Ok(res.rows_affected() > 0)
    }

    /// Start a new run for `chain_id` at step 0, with no task queued yet.
    /// Returns the created run row.
    ///
    /// # Errors
    /// Returns `Err` if the insert fails.
    pub async fn start_chain_run(&self, chain_id: &str) -> Result<ChainRunRow> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO schedule_chain_runs
               (id, chain_id, fired_at, current_step, current_task_id, status, updated_at)
             VALUES (?, ?, ?, 0, NULL, 'running', ?)",
        )
        .bind(&id)
        .bind(chain_id)
        .bind(&now)
        .bind(&now)
        .execute(&self.write_pool)
        .await
        .context("starting chain run")?;
        Ok(ChainRunRow {
            id,
            chain_id: chain_id.to_string(),
            fired_at: now.clone(),
            current_step: 0,
            current_task_id: None,
            status: "running".into(),
            updated_at: now,
        })
    }

    /// Record that `run_id` has moved to `step_order` and is waiting on
    /// `task_id`. Called every time a step is (re-)submitted, including on
    /// restart-resume, so the run row always reflects the true in-flight step.
    ///
    /// # Errors
    /// Returns `Err` if the write fails.
    pub async fn advance_chain_run(
        &self,
        run_id: &str,
        step_order: i64,
        task_id: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE schedule_chain_runs \
             SET current_step = ?, current_task_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(step_order)
        .bind(task_id)
        .bind(Utc::now().to_rfc3339())
        .bind(run_id)
        .execute(&self.write_pool)
        .await
        .context("advancing chain run")?;
        Ok(())
    }

    /// Mark a run terminal (`completed` or `failed`).
    ///
    /// # Errors
    /// Returns `Err` if the write fails.
    pub async fn finish_chain_run(&self, run_id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE schedule_chain_runs SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(Utc::now().to_rfc3339())
            .bind(run_id)
            .execute(&self.write_pool)
            .await
            .context("finishing chain run")?;
        Ok(())
    }

    /// Fetch a single run row by id.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn get_chain_run(&self, run_id: &str) -> Result<Option<ChainRunRow>> {
        let row = sqlx::query(
            "SELECT id, chain_id, fired_at, current_step, current_task_id, status, updated_at \
             FROM schedule_chain_runs WHERE id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.read_pool)
        .await
        .context("fetching chain run")?;
        Ok(row.map(run_from_row))
    }

    /// Every run still marked `running` — the boot-time resume set. A run
    /// left in this state across a restart is, by definition, orphaned: no
    /// in-memory pool state survives a process restart (see `AgentPool`), so
    /// these are exactly the runs `ChainScheduleManager::resume_orphaned`
    /// must re-drive.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn list_running_chain_runs(&self) -> Result<Vec<ChainRunRow>> {
        let rows = sqlx::query(
            "SELECT id, chain_id, fired_at, current_step, current_task_id, status, updated_at \
             FROM schedule_chain_runs WHERE status = 'running'",
        )
        .fetch_all(&self.read_pool)
        .await
        .context("listing running chain runs")?;
        Ok(rows.into_iter().map(run_from_row).collect())
    }

    /// Most-recent `limit` run rows for a chain, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn list_chain_runs(&self, chain_id: &str, limit: i64) -> Result<Vec<ChainRunRow>> {
        let rows = sqlx::query(
            "SELECT id, chain_id, fired_at, current_step, current_task_id, status, updated_at \
             FROM schedule_chain_runs WHERE chain_id = ? \
             ORDER BY fired_at DESC, id DESC LIMIT ?",
        )
        .bind(chain_id)
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await
        .context("listing chain runs")?;
        Ok(rows.into_iter().map(run_from_row).collect())
    }
}

const CHAIN_SELECT_COLS: &str = "SELECT id, name, cron, repo, priority, autonomy_level, \
     on_fail, enabled, created_at, updated_at FROM schedule_chains";

fn chain_from_row(row: sqlx::sqlite::SqliteRow) -> ScheduleChainRow {
    ScheduleChainRow {
        id: row.get("id"),
        name: row.get("name"),
        cron: row.get("cron"),
        repo: row.get("repo"),
        priority: row.get("priority"),
        autonomy_level: row.get("autonomy_level"),
        on_fail: row.get("on_fail"),
        enabled: row.get::<i64, _>("enabled") != 0,
        steps: Vec::new(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn step_from_row(row: sqlx::sqlite::SqliteRow) -> ChainStepRow {
    ChainStepRow {
        step_order: row.get("step_order"),
        goal: row.get("goal"),
        allowed_dirs: parse_json_array(&row.get::<String, _>("allowed_dirs")),
        forbidden_dirs: parse_json_array(&row.get::<String, _>("forbidden_dirs")),
    }
}

fn run_from_row(row: sqlx::sqlite::SqliteRow) -> ChainRunRow {
    ChainRunRow {
        id: row.get("id"),
        chain_id: row.get("chain_id"),
        fired_at: row.get("fired_at"),
        current_step: row.get("current_step"),
        current_task_id: row.get("current_task_id"),
        status: row.get("status"),
        updated_at: row.get("updated_at"),
    }
}

/// Normalize an autonomy tag to a canonical value, defaulting to `draft_pr`
/// for empty or unrecognized input. Mirrors `super::schedules::normalize_autonomy`.
fn normalize_autonomy(level: &str) -> String {
    lopi_core::AutonomyLevel::parse(level)
        .unwrap_or_default()
        .tag_snake()
        .to_string()
}

/// Normalize an on-fail tag, defaulting to the conservative `stop` for
/// anything not in the client's `OnFail` union.
fn normalize_on_fail(tag: &str) -> String {
    match tag {
        "continue" | "backoff" => tag.to_string(),
        _ => "stop".to_string(),
    }
}

/// Parse a JSON string array, falling back to empty on malformed data so a
/// corrupt column never crashes a list query.
fn parse_json_array(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

#[cfg(test)]
#[path = "schedule_chains_tests.rs"]
mod tests;
