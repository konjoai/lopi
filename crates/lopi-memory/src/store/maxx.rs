//! MAXX (opportunistic backlog dispatch) entry storage.
//!
//! Mirrors `schedules.rs` exactly — same CRUD shape, same `maxx_runs` fire-history
//! table pattern — minus `cron`, plus the favorability fields `maxx_loop`'s tick
//! reads: `quiet_hours_start`/`quiet_hours_end`, `headroom_gate`, `windows_json`.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row as _;
use uuid::Uuid;

use super::MemoryStore;

/// One persisted MAXX entry row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaxxRow {
    /// Row-level UUID — primary key, stable across edits.
    pub id: String,
    /// Human-readable name shown in the UI.
    pub name: String,
    /// Agent goal submitted when the entry fires.
    pub goal: String,
    /// Repo the task targets, if any.
    pub repo: Option<String>,
    /// Priority string: `low` / `normal` / `high` / `critical`.
    pub priority: String,
    /// Directories the agent may touch (overrides global config when non-empty).
    pub allowed_dirs: Vec<String>,
    /// Directories the agent must not touch.
    pub forbidden_dirs: Vec<String>,
    /// When `false` the entry is persisted but never checked by the tick.
    pub enabled: bool,
    /// Trust level (L1–L4) governing how far this loop may act without a human.
    pub autonomy_level: String,
    /// Report on Finish channel, e.g. `"telegram"`. `None` if unset.
    pub report: Option<String>,
    /// Quiet-hours start, local hour `0..=23`. `None` when unset.
    pub quiet_hours_start: Option<u8>,
    /// Quiet-hours end, local hour `0..=23`. `None` when unset.
    pub quiet_hours_end: Option<u8>,
    /// Whether the quota-headroom condition is checked.
    pub headroom_gate: bool,
    /// `LimitWindow` tags `headroom_gate` checks, e.g. `["five_hour"]`.
    pub windows: Vec<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 timestamp of the last edit.
    pub updated_at: String,
}

/// Input to [`MemoryStore::upsert_maxx_entry`]. `id == None` creates a new row.
#[derive(Debug, Clone)]
pub struct MaxxInput {
    /// Existing row id to update, or `None` to insert a fresh row.
    pub id: Option<String>,
    /// Human-readable name.
    pub name: String,
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
    /// Whether the entry should be checked by the tick.
    pub enabled: bool,
    /// Trust level tag (`report_only` … `auto_merge`). Empty falls back to `draft_pr`.
    pub autonomy_level: String,
    /// Report on Finish channel, if any.
    pub report: Option<String>,
    /// Quiet-hours start, local hour `0..=23`.
    pub quiet_hours_start: Option<u8>,
    /// Quiet-hours end, local hour `0..=23`.
    pub quiet_hours_end: Option<u8>,
    /// Whether the quota-headroom condition is checked.
    pub headroom_gate: bool,
    /// `LimitWindow` tags `headroom_gate` checks.
    pub windows: Vec<String>,
}

/// One row from a MAXX entry's fire history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaxxRunRow {
    /// Run-level UUID — primary key.
    pub id: String,
    /// Owning entry id.
    pub maxx_id: String,
    /// When the entry fired (ISO-8601).
    pub fired_at: String,
    /// Task id queued by this fire, if one was created.
    pub task_id: Option<String>,
    /// Short outcome string: `queued`, `duplicate`, `error`, …
    pub outcome: String,
}

impl MemoryStore {
    /// List every MAXX entry, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn list_maxx_entries(&self) -> Result<Vec<MaxxRow>> {
        let sql = format!("{SELECT_COLS} ORDER BY created_at DESC");
        let rows = sqlx::query(&sql)
            .fetch_all(&self.read_pool)
            .await
            .context("listing maxx entries")?;
        Ok(rows.into_iter().map(maxx_from_row).collect())
    }

    /// Fetch a single MAXX entry by id.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn get_maxx_entry(&self, id: &str) -> Result<Option<MaxxRow>> {
        let sql = format!("{SELECT_COLS} WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.read_pool)
            .await
            .context("fetching maxx entry")?;
        Ok(row.map(maxx_from_row))
    }

    /// Insert (when `input.id` is `None`) or update an existing MAXX entry.
    /// Returns the stored row, including its generated id and timestamps.
    ///
    /// # Errors
    /// Returns `Err` if JSON serialisation or the write fails.
    pub async fn upsert_maxx_entry(&self, input: &MaxxInput) -> Result<MaxxRow> {
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = Utc::now().to_rfc3339();
        let allowed = serde_json::to_string(&input.allowed_dirs)?;
        let forbidden = serde_json::to_string(&input.forbidden_dirs)?;
        let windows = serde_json::to_string(&input.windows)?;
        let autonomy = normalize_autonomy(&input.autonomy_level);
        sqlx::query(
            "INSERT INTO maxx_entries
               (id, name, goal, repo, priority, allowed_dirs, forbidden_dirs, enabled,
                autonomy_level, report, quiet_hours_start, quiet_hours_end, headroom_gate,
                windows_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name, goal = excluded.goal, repo = excluded.repo,
               priority = excluded.priority, allowed_dirs = excluded.allowed_dirs,
               forbidden_dirs = excluded.forbidden_dirs, enabled = excluded.enabled,
               autonomy_level = excluded.autonomy_level, report = excluded.report,
               quiet_hours_start = excluded.quiet_hours_start,
               quiet_hours_end = excluded.quiet_hours_end,
               headroom_gate = excluded.headroom_gate, windows_json = excluded.windows_json,
               updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.goal)
        .bind(&input.repo)
        .bind(&input.priority)
        .bind(&allowed)
        .bind(&forbidden)
        .bind(i64::from(input.enabled))
        .bind(&autonomy)
        .bind(&input.report)
        .bind(input.quiet_hours_start.map(i64::from))
        .bind(input.quiet_hours_end.map(i64::from))
        .bind(i64::from(input.headroom_gate))
        .bind(&windows)
        .bind(&now)
        .bind(&now)
        .execute(&self.write_pool)
        .await
        .context("upserting maxx entry")?;
        self.get_maxx_entry(&id)
            .await?
            .context("maxx entry vanished after upsert")
    }

    /// Toggle a MAXX entry's `enabled` flag. Returns `true` when a row matched.
    ///
    /// # Errors
    /// Returns `Err` if the write fails.
    pub async fn set_maxx_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let res = sqlx::query("UPDATE maxx_entries SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(i64::from(enabled))
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.write_pool)
            .await
            .context("setting maxx entry enabled")?;
        Ok(res.rows_affected() > 0)
    }

    /// Permanently delete a MAXX entry and its run history.
    /// Returns `true` when the entry existed.
    ///
    /// # Errors
    /// Returns `Err` if either delete fails.
    pub async fn delete_maxx_entry(&self, id: &str) -> Result<bool> {
        let mut tx = self.write_pool.begin().await?;
        sqlx::query("DELETE FROM maxx_runs WHERE maxx_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("deleting maxx runs")?;
        let res = sqlx::query("DELETE FROM maxx_entries WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("deleting maxx entry")?;
        tx.commit().await?;
        Ok(res.rows_affected() > 0)
    }

    /// Append a run-history row when a MAXX entry fires.
    ///
    /// # Errors
    /// Returns `Err` if the insert fails.
    pub async fn record_maxx_run(
        &self,
        maxx_id: &str,
        task_id: Option<&str>,
        outcome: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO maxx_runs (id, maxx_id, fired_at, task_id, outcome)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(maxx_id)
        .bind(Utc::now().to_rfc3339())
        .bind(task_id)
        .bind(outcome)
        .execute(&self.write_pool)
        .await
        .context("recording maxx run")?;
        Ok(())
    }

    /// Most-recent `limit` run-history rows for a MAXX entry, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn list_maxx_runs(&self, maxx_id: &str, limit: i64) -> Result<Vec<MaxxRunRow>> {
        let rows = sqlx::query(
            "SELECT id, maxx_id, fired_at, task_id, outcome
             FROM maxx_runs WHERE maxx_id = ?
             ORDER BY fired_at DESC, id DESC LIMIT ?",
        )
        .bind(maxx_id)
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await
        .context("listing maxx runs")?;
        Ok(rows.into_iter().map(run_from_row).collect())
    }
}

const SELECT_COLS: &str = "SELECT id, name, goal, repo, priority, allowed_dirs, forbidden_dirs, \
     enabled, autonomy_level, report, quiet_hours_start, quiet_hours_end, headroom_gate, \
     windows_json, created_at, updated_at FROM maxx_entries";

fn maxx_from_row(row: sqlx::sqlite::SqliteRow) -> MaxxRow {
    MaxxRow {
        id: row.get("id"),
        name: row.get("name"),
        goal: row.get("goal"),
        repo: row.get("repo"),
        priority: row.get("priority"),
        allowed_dirs: parse_json_array(&row.get::<String, _>("allowed_dirs")),
        forbidden_dirs: parse_json_array(&row.get::<String, _>("forbidden_dirs")),
        enabled: row.get::<i64, _>("enabled") != 0,
        autonomy_level: row.get("autonomy_level"),
        report: row.get("report"),
        quiet_hours_start: row
            .get::<Option<i64>, _>("quiet_hours_start")
            .map(|v| v as u8),
        quiet_hours_end: row
            .get::<Option<i64>, _>("quiet_hours_end")
            .map(|v| v as u8),
        headroom_gate: row.get::<i64, _>("headroom_gate") != 0,
        windows: parse_json_array(&row.get::<String, _>("windows_json")),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

/// Normalize an autonomy tag to a canonical value, defaulting to `draft_pr`.
fn normalize_autonomy(level: &str) -> String {
    lopi_core::AutonomyLevel::parse(level)
        .unwrap_or_default()
        .tag_snake()
        .to_string()
}

fn run_from_row(row: sqlx::sqlite::SqliteRow) -> MaxxRunRow {
    MaxxRunRow {
        id: row.get("id"),
        maxx_id: row.get("maxx_id"),
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

    fn input(name: &str) -> MaxxInput {
        MaxxInput {
            id: None,
            name: name.into(),
            goal: "work the backlog".into(),
            repo: Some("/tmp/repo".into()),
            priority: "low".into(),
            allowed_dirs: vec!["src/".into()],
            forbidden_dirs: vec!["infra/".into()],
            enabled: true,
            autonomy_level: "draft_pr".into(),
            report: None,
            quiet_hours_start: Some(23),
            quiet_hours_end: Some(7),
            headroom_gate: true,
            windows: vec!["five_hour".into(), "seven_day".into()],
        }
    }

    #[tokio::test]
    async fn upsert_then_get_round_trips() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_maxx_entry(&input("overnight")).await.unwrap();
        let fetched = store.get_maxx_entry(&row.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "overnight");
        assert_eq!(fetched.quiet_hours_start, Some(23));
        assert_eq!(fetched.quiet_hours_end, Some(7));
        assert!(fetched.headroom_gate);
        assert_eq!(fetched.windows, vec!["five_hour", "seven_day"]);
        assert!(fetched.enabled);
    }

    #[tokio::test]
    async fn upsert_with_id_updates_in_place() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_maxx_entry(&input("orig")).await.unwrap();
        let mut edit = input("renamed");
        edit.id = Some(row.id.clone());
        edit.headroom_gate = false;
        let updated = store.upsert_maxx_entry(&edit).await.unwrap();
        assert_eq!(updated.id, row.id);
        assert_eq!(updated.name, "renamed");
        assert!(!updated.headroom_gate);
        assert_eq!(store.list_maxx_entries().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn set_enabled_toggles_flag() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_maxx_entry(&input("toggle")).await.unwrap();
        assert!(store.set_maxx_enabled(&row.id, false).await.unwrap());
        assert!(
            !store
                .get_maxx_entry(&row.id)
                .await
                .unwrap()
                .unwrap()
                .enabled
        );
        assert!(!store.set_maxx_enabled("nope", true).await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_entry_and_runs() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_maxx_entry(&input("doomed")).await.unwrap();
        store
            .record_maxx_run(&row.id, Some("task-1"), "queued")
            .await
            .unwrap();
        assert!(store.delete_maxx_entry(&row.id).await.unwrap());
        assert!(store.get_maxx_entry(&row.id).await.unwrap().is_none());
        assert!(store.list_maxx_runs(&row.id, 10).await.unwrap().is_empty());
        assert!(!store.delete_maxx_entry(&row.id).await.unwrap());
    }

    #[tokio::test]
    async fn run_history_is_newest_first_and_limited() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let row = store.upsert_maxx_entry(&input("hist")).await.unwrap();
        for tid in ["a", "b", "c"] {
            store
                .record_maxx_run(&row.id, Some(tid), "queued")
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        let runs = store.list_maxx_runs(&row.id, 2).await.unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].task_id.as_deref(), Some("c"));
    }

    #[tokio::test]
    async fn list_is_empty_on_fresh_store() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        assert!(store.list_maxx_entries().await.unwrap().is_empty());
    }
}
