//! Durable cron schedule storage.
//!
//! lopi's original schedules live in `lopi.toml` as a static `[[schedules]]`
//! list (`lopi_core::ScheduleEntry`) that is read once at boot. That is fine
//! for a checked-in config but useless for an interactive dashboard: there is
//! no way to add, edit, enable/disable, or inspect the run history of a
//! schedule at runtime.
//!
//! This module backs the OpenClaw-style cron UI. Schedules are persisted in
//! the `schedules` table and every fire (cron tick or manual run-now) appends
//! a row to `schedule_runs`. The web layer exposes CRUD over `/api/schedules`
//! and the orchestrator's `ScheduleManager` registers the enabled rows as live
//! cron jobs on boot.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row as _;
use uuid::Uuid;

use super::MemoryStore;

/// One persisted schedule row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleRow {
    /// Row-level UUID — primary key, stable across edits.
    pub id: String,
    /// Human-readable name shown in the UI.
    pub name: String,
    /// Standard 5-field cron expression, e.g. `"0 2 * * *"`.
    pub cron: String,
    /// Agent goal submitted when the schedule fires.
    pub goal: String,
    /// Repo the task targets, if any.
    pub repo: Option<String>,
    /// Priority string: `low` / `normal` / `high` / `critical`.
    pub priority: String,
    /// Directories the agent may touch (overrides global config when non-empty).
    pub allowed_dirs: Vec<String>,
    /// Directories the agent must not touch.
    pub forbidden_dirs: Vec<String>,
    /// When `false` the schedule is persisted but not registered as a live job.
    pub enabled: bool,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 timestamp of the last edit.
    pub updated_at: String,
}

/// Input to [`MemoryStore::upsert_schedule`]. `id == None` creates a new row;
/// `id == Some(_)` updates the matching row in place.
#[derive(Debug, Clone)]
pub struct ScheduleInput {
    /// Existing row id to update, or `None` to insert a fresh row.
    pub id: Option<String>,
    /// Human-readable name.
    pub name: String,
    /// Cron expression (validated by the caller before persisting).
    pub cron: String,
    /// Agent goal.
    pub goal: String,
    /// Target repo path, if any.
    pub repo: Option<String>,
    /// Priority string.
    pub priority: String,
    /// Allowed directories.
    pub allowed_dirs: Vec<String>,
    /// Forbidden directories.
    pub forbidden_dirs: Vec<String>,
    /// Whether the schedule should be live.
    pub enabled: bool,
}

/// One row from a schedule's run history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRunRow {
    /// Run-level UUID — primary key.
    pub id: String,
    /// Owning schedule id.
    pub schedule_id: String,
    /// When the schedule fired (ISO-8601).
    pub fired_at: String,
    /// Task id queued by this fire, if one was created.
    pub task_id: Option<String>,
    /// Short outcome string: `queued`, `duplicate`, `error`, …
    pub outcome: String,
}

impl MemoryStore {
    /// List every schedule, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn list_schedules(&self) -> Result<Vec<ScheduleRow>> {
        let sql = format!("{SELECT_COLS} ORDER BY created_at DESC");
        let rows = sqlx::query(&sql)
            .fetch_all(&self.read_pool)
            .await
            .context("listing schedules")?;
        Ok(rows.into_iter().map(schedule_from_row).collect())
    }

    /// Fetch a single schedule by id.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn get_schedule(&self, id: &str) -> Result<Option<ScheduleRow>> {
        let sql = format!("{SELECT_COLS} WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.read_pool)
            .await
            .context("fetching schedule")?;
        Ok(row.map(schedule_from_row))
    }

    /// Look up a schedule by its name (used to seed TOML entries idempotently).
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn find_schedule_by_name(&self, name: &str) -> Result<Option<ScheduleRow>> {
        let sql = format!("{SELECT_COLS} WHERE name = ? LIMIT 1");
        let row = sqlx::query(&sql)
            .bind(name)
            .fetch_optional(&self.read_pool)
            .await
            .context("finding schedule by name")?;
        Ok(row.map(schedule_from_row))
    }

    /// Insert (when `input.id` is `None`) or update an existing schedule.
    /// Returns the stored row, including its generated id and timestamps.
    ///
    /// # Errors
    /// Returns `Err` if JSON serialisation or the write fails.
    pub async fn upsert_schedule(&self, input: &ScheduleInput) -> Result<ScheduleRow> {
        let id = input.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = Utc::now().to_rfc3339();
        let allowed = serde_json::to_string(&input.allowed_dirs)?;
        let forbidden = serde_json::to_string(&input.forbidden_dirs)?;
        sqlx::query(
            "INSERT INTO schedules
               (id, name, cron, goal, repo, priority, allowed_dirs,
                forbidden_dirs, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name, cron = excluded.cron, goal = excluded.goal,
               repo = excluded.repo, priority = excluded.priority,
               allowed_dirs = excluded.allowed_dirs,
               forbidden_dirs = excluded.forbidden_dirs,
               enabled = excluded.enabled, updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.cron)
        .bind(&input.goal)
        .bind(&input.repo)
        .bind(&input.priority)
        .bind(&allowed)
        .bind(&forbidden)
        .bind(i64::from(input.enabled))
        .bind(&now)
        .bind(&now)
        .execute(&self.write_pool)
        .await
        .context("upserting schedule")?;
        // Re-read so the returned row reflects the persisted created_at on update.
        self.get_schedule(&id)
            .await?
            .context("schedule vanished after upsert")
    }

    /// Toggle a schedule's `enabled` flag. Returns `true` when a row matched.
    ///
    /// # Errors
    /// Returns `Err` if the write fails.
    pub async fn set_schedule_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let res = sqlx::query("UPDATE schedules SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(i64::from(enabled))
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.write_pool)
            .await
            .context("setting schedule enabled")?;
        Ok(res.rows_affected() > 0)
    }

    /// Permanently delete a schedule and its run history.
    /// Returns `true` when the schedule existed.
    ///
    /// # Errors
    /// Returns `Err` if either delete fails.
    pub async fn delete_schedule(&self, id: &str) -> Result<bool> {
        let mut tx = self.write_pool.begin().await?;
        sqlx::query("DELETE FROM schedule_runs WHERE schedule_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("deleting schedule runs")?;
        let res = sqlx::query("DELETE FROM schedules WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("deleting schedule")?;
        tx.commit().await?;
        Ok(res.rows_affected() > 0)
    }

    /// Append a run-history row when a schedule fires.
    ///
    /// # Errors
    /// Returns `Err` if the insert fails.
    pub async fn record_schedule_run(
        &self,
        schedule_id: &str,
        task_id: Option<&str>,
        outcome: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO schedule_runs (id, schedule_id, fired_at, task_id, outcome)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(schedule_id)
        .bind(Utc::now().to_rfc3339())
        .bind(task_id)
        .bind(outcome)
        .execute(&self.write_pool)
        .await
        .context("recording schedule run")?;
        Ok(())
    }

    /// Most-recent `limit` run-history rows for a schedule, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn list_schedule_runs(
        &self,
        schedule_id: &str,
        limit: i64,
    ) -> Result<Vec<ScheduleRunRow>> {
        let rows = sqlx::query(
            "SELECT id, schedule_id, fired_at, task_id, outcome
             FROM schedule_runs WHERE schedule_id = ?
             ORDER BY fired_at DESC, id DESC LIMIT ?",
        )
        .bind(schedule_id)
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await
        .context("listing schedule runs")?;
        Ok(rows.into_iter().map(run_from_row).collect())
    }
}

const SELECT_COLS: &str = "SELECT id, name, cron, goal, repo, priority, allowed_dirs, \
     forbidden_dirs, enabled, created_at, updated_at FROM schedules";

fn schedule_from_row(row: sqlx::sqlite::SqliteRow) -> ScheduleRow {
    ScheduleRow {
        id: row.get("id"),
        name: row.get("name"),
        cron: row.get("cron"),
        goal: row.get("goal"),
        repo: row.get("repo"),
        priority: row.get("priority"),
        allowed_dirs: parse_json_array(&row.get::<String, _>("allowed_dirs")),
        forbidden_dirs: parse_json_array(&row.get::<String, _>("forbidden_dirs")),
        enabled: row.get::<i64, _>("enabled") != 0,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn run_from_row(row: sqlx::sqlite::SqliteRow) -> ScheduleRunRow {
    ScheduleRunRow {
        id: row.get("id"),
        schedule_id: row.get("schedule_id"),
        fired_at: row.get("fired_at"),
        task_id: row.get("task_id"),
        outcome: row.get("outcome"),
    }
}

/// Parse a JSON string array, falling back to empty on malformed data so a
/// corrupt column never crashes a list query.
fn parse_json_array(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn input(name: &str) -> ScheduleInput {
        ScheduleInput {
            id: None,
            name: name.into(),
            cron: "0 2 * * *".into(),
            goal: "run nightly checks".into(),
            repo: Some("/tmp/repo".into()),
            priority: "high".into(),
            allowed_dirs: vec!["src/".into()],
            forbidden_dirs: vec!["infra/".into()],
            enabled: true,
        }
    }

    #[tokio::test]
    async fn upsert_then_get_round_trips() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_schedule(&input("nightly")).await.unwrap();
        let fetched = store.get_schedule(&row.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "nightly");
        assert_eq!(fetched.cron, "0 2 * * *");
        assert_eq!(fetched.priority, "high");
        assert_eq!(fetched.allowed_dirs, vec!["src/".to_string()]);
        assert_eq!(fetched.forbidden_dirs, vec!["infra/".to_string()]);
        assert!(fetched.enabled);
        assert_eq!(fetched.repo.as_deref(), Some("/tmp/repo"));
    }

    #[tokio::test]
    async fn upsert_with_id_updates_in_place() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_schedule(&input("orig")).await.unwrap();
        let mut edit = input("renamed");
        edit.id = Some(row.id.clone());
        edit.cron = "0 5 * * *".into();
        let updated = store.upsert_schedule(&edit).await.unwrap();
        assert_eq!(updated.id, row.id);
        assert_eq!(updated.name, "renamed");
        assert_eq!(updated.cron, "0 5 * * *");
        // Only one row should exist after an in-place update.
        assert_eq!(store.list_schedules().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn set_enabled_toggles_flag() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_schedule(&input("toggle")).await.unwrap();
        assert!(store.set_schedule_enabled(&row.id, false).await.unwrap());
        assert!(!store.get_schedule(&row.id).await.unwrap().unwrap().enabled);
        assert!(store.set_schedule_enabled(&row.id, true).await.unwrap());
        assert!(store.get_schedule(&row.id).await.unwrap().unwrap().enabled);
        // Unknown id reports no row matched.
        assert!(!store.set_schedule_enabled("nope", true).await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_schedule_and_runs() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_schedule(&input("doomed")).await.unwrap();
        store
            .record_schedule_run(&row.id, Some("task-1"), "queued")
            .await
            .unwrap();
        assert!(store.delete_schedule(&row.id).await.unwrap());
        assert!(store.get_schedule(&row.id).await.unwrap().is_none());
        assert!(store.list_schedule_runs(&row.id, 10).await.unwrap().is_empty());
        // Second delete is a clean false.
        assert!(!store.delete_schedule(&row.id).await.unwrap());
    }

    #[tokio::test]
    async fn find_by_name_matches() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store.upsert_schedule(&input("unique-name")).await.unwrap();
        assert!(store
            .find_schedule_by_name("unique-name")
            .await
            .unwrap()
            .is_some());
        assert!(store
            .find_schedule_by_name("missing")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn run_history_is_newest_first_and_limited() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_schedule(&input("hist")).await.unwrap();
        for tid in ["a", "b", "c"] {
            store
                .record_schedule_run(&row.id, Some(tid), "queued")
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        let runs = store.list_schedule_runs(&row.id, 2).await.unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].task_id.as_deref(), Some("c"));
        assert_eq!(runs[0].outcome, "queued");
    }

    #[tokio::test]
    async fn list_is_empty_on_fresh_store() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        assert!(store.list_schedules().await.unwrap().is_empty());
    }
}
