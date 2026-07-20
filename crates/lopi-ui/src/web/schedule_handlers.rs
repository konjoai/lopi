//! Cron schedule REST surface — the backend for the dashboard cron UI.
//!
//! Schedules persist in the `schedules` table (`lopi_memory::ScheduleRow`) and
//! the live [`ScheduleManager`](lopi_orchestrator::ScheduleManager) registers
//! the enabled ones as cron jobs. These handlers keep the two in sync: every
//! mutation writes the store and then registers/unregisters the live job.
//!
//! Routes (all behind the shared Bearer-auth + rate-limit middleware):
//! - `GET    /api/schedules`            — list with next-runs + last-run
//! - `POST   /api/schedules`            — create
//! - `GET    /api/schedules/:id`        — one schedule + run history
//! - `PUT    /api/schedules/:id`        — edit
//! - `DELETE /api/schedules/:id`        — delete
//! - `POST   /api/schedules/:id/enable` — enable + register
//! - `POST   /api/schedules/:id/disable`— disable + unregister
//! - `POST   /api/schedules/:id/run-now`— fire immediately

use super::types::{reject_control_chars, MAX_GOAL_LENGTH};
use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_memory::{ScheduleInput, ScheduleRow};
use lopi_orchestrator::next_run_times;
use serde::Deserialize;
use serde_json::{json, Value};

/// Create/update body. `enabled` defaults to `true` on create.
#[derive(Debug, Deserialize)]
pub(super) struct ScheduleBody {
    pub name: String,
    pub cron: String,
    pub goal: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub allowed_dirs: Option<Vec<String>>,
    #[serde(default)]
    pub forbidden_dirs: Option<Vec<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Trust level: `report_only` / `draft_pr` / `verified_pr` / `auto_merge`.
    #[serde(default)]
    pub autonomy_level: Option<String>,
}

impl ScheduleBody {
    /// Validate inputs at the API boundary. Returns an error message on failure.
    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("name must not be empty".into());
        }
        if self.goal.trim().is_empty() {
            return Err("goal must not be empty".into());
        }
        if self.goal.chars().count() > MAX_GOAL_LENGTH {
            return Err(format!("goal exceeds {MAX_GOAL_LENGTH} chars"));
        }
        reject_control_chars(&self.goal)?;
        if next_run_times(&self.cron, 1).is_empty() {
            return Err("invalid cron expression (expected 5 fields)".into());
        }
        Ok(())
    }

    fn into_input(self, id: Option<String>) -> ScheduleInput {
        ScheduleInput {
            id,
            name: self.name,
            cron: self.cron,
            goal: self.goal,
            repo: self.repo,
            priority: self.priority.unwrap_or_else(|| "normal".into()),
            allowed_dirs: self.allowed_dirs.unwrap_or_default(),
            forbidden_dirs: self.forbidden_dirs.unwrap_or_default(),
            enabled: self.enabled.unwrap_or(true),
            autonomy_level: self.autonomy_level.unwrap_or_default(),
        }
    }
}

pub(super) async fn list_schedules(State(s): State<AppState>) -> impl IntoResponse {
    match s.store.list_schedules().await {
        Ok(rows) => {
            let mut body = Vec::with_capacity(rows.len());
            for row in rows {
                body.push(schedule_to_json(&s, row).await);
            }
            (StatusCode::OK, Json(json!({ "schedules": body }))).into_response()
        }
        Err(e) => server_error(&e),
    }
}

pub(super) async fn create_schedule(
    State(s): State<AppState>,
    Json(body): Json<ScheduleBody>,
) -> impl IntoResponse {
    if let Err(msg) = body.validate() {
        return unprocessable(&msg);
    }
    persist_and_register(&s, body.into_input(None), StatusCode::CREATED).await
}

pub(super) async fn get_schedule(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.get_schedule(&id).await {
        Ok(Some(row)) => {
            let runs = s
                .store
                .list_schedule_runs(&id, 20)
                .await
                .unwrap_or_default();
            let mut value = schedule_to_json(&s, row).await;
            value["runs"] = json!(runs);
            (StatusCode::OK, Json(value)).into_response()
        }
        Ok(None) => not_found(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn update_schedule(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(body): Json<ScheduleBody>,
) -> impl IntoResponse {
    if let Err(msg) = body.validate() {
        return unprocessable(&msg);
    }
    match s.store.get_schedule(&id).await {
        Ok(Some(_)) => persist_and_register(&s, body.into_input(Some(id)), StatusCode::OK).await,
        Ok(None) => not_found(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn delete_schedule(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.delete_schedule(&id).await {
        Ok(true) => {
            if let Err(e) = s.schedules.unregister(&id).await {
                tracing::warn!("unregister after delete failed: {e:#}");
            }
            (StatusCode::OK, Json(json!({ "deleted": id }))).into_response()
        }
        Ok(false) => not_found(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn enable_schedule(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    set_enabled(&s, &id, true).await
}

pub(super) async fn disable_schedule(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    set_enabled(&s, &id, false).await
}

pub(super) async fn run_now(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.get_schedule(&id).await {
        Ok(Some(row)) => match s.schedules.run_now(&row.into()).await {
            Ok(task_id) => (
                StatusCode::ACCEPTED,
                Json(json!({ "schedule_id": id, "task_id": task_id, "queued": task_id.is_some() })),
            )
                .into_response(),
            Err(e) => server_error(&e),
        },
        Ok(None) => not_found(),
        Err(e) => server_error(&e),
    }
}

/// Body for `POST /api/schedules/:id/autonomy` — the Loop Engineering
/// Trust-Level dropdown writes here.
#[derive(Debug, Deserialize)]
pub(super) struct AutonomyBody {
    /// Trust level tag: `report_only` / `draft_pr` / `verified_pr` / `auto_merge`.
    pub level: String,
}

/// Set a schedule's trust (autonomy) level. The store normalizes unrecognized
/// values to the conservative `draft_pr`.
pub(super) async fn set_autonomy(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(body): Json<AutonomyBody>,
) -> impl IntoResponse {
    match s.store.set_schedule_autonomy(&id, &body.level).await {
        Ok(true) => match s.store.get_schedule(&id).await {
            Ok(Some(row)) => (
                StatusCode::OK,
                Json(json!({ "id": id, "autonomy_level": row.autonomy_level })),
            )
                .into_response(),
            Ok(None) => not_found(),
            Err(e) => server_error(&e),
        },
        Ok(false) => not_found(),
        Err(e) => server_error(&e),
    }
}

/// Upsert a schedule then (un)register its live job to match `enabled`.
async fn persist_and_register(
    s: &AppState,
    input: ScheduleInput,
    ok: StatusCode,
) -> axum::response::Response {
    let row = match s.store.upsert_schedule(&input).await {
        Ok(r) => r,
        Err(e) => return server_error(&e),
    };
    sync_job(s, &row).await;
    let value = schedule_to_json(s, row).await;
    (ok, Json(value)).into_response()
}

/// Flip a schedule's enabled flag and bring its live job into line.
async fn set_enabled(s: &AppState, id: &str, enabled: bool) -> axum::response::Response {
    match s.store.set_schedule_enabled(id, enabled).await {
        Ok(true) => {
            if let Ok(Some(row)) = s.store.get_schedule(id).await {
                sync_job(s, &row).await;
            }
            (
                StatusCode::OK,
                Json(json!({ "id": id, "enabled": enabled })),
            )
                .into_response()
        }
        Ok(false) => not_found(),
        Err(e) => server_error(&e),
    }
}

/// Register the row's job when enabled, unregister it otherwise. Logs and
/// swallows scheduler errors so a job-registration hiccup never fails the API
/// write that already succeeded.
async fn sync_job(s: &AppState, row: &ScheduleRow) {
    let result = if row.enabled {
        s.schedules.register(row.clone().into()).await.map(|_| ())
    } else {
        s.schedules.unregister(&row.id).await
    };
    if let Err(e) = result {
        tracing::warn!(schedule = %row.id, "scheduler sync failed: {e:#}");
    }
}

/// Serialize a schedule plus its computed next-run times and last run.
async fn schedule_to_json(s: &AppState, row: ScheduleRow) -> Value {
    let next_runs: Vec<String> = next_run_times(&row.cron, 3)
        .into_iter()
        .map(|t| t.to_rfc3339())
        .collect();
    let last_run = s
        .store
        .list_schedule_runs(&row.id, 1)
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    json!({
        "id": row.id, "name": row.name, "cron": row.cron, "goal": row.goal,
        "repo": row.repo, "priority": row.priority,
        "allowed_dirs": row.allowed_dirs, "forbidden_dirs": row.forbidden_dirs,
        "enabled": row.enabled, "autonomy_level": row.autonomy_level,
        "created_at": row.created_at,
        "updated_at": row.updated_at, "next_runs": next_runs, "last_run": last_run,
    })
}

fn not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "schedule not found" })),
    )
        .into_response()
}

fn unprocessable(msg: &str) -> axum::response::Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({ "error": msg })),
    )
        .into_response()
}

fn server_error(e: &anyhow::Error) -> axum::response::Response {
    tracing::warn!("schedule handler error: {e:#}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": format!("{e:#}") })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_body() -> ScheduleBody {
        ScheduleBody {
            name: "nightly".to_string(),
            cron: "0 2 * * *".to_string(),
            goal: "run tests".to_string(),
            repo: None,
            priority: None,
            allowed_dirs: None,
            forbidden_dirs: None,
            enabled: None,
            autonomy_level: None,
        }
    }

    /// Regression: `POST /api/tasks` rejects control characters (log-poisoning
    /// / ANSI-injection guard) via `handlers::validate_goal`, but schedule
    /// creation had its own, separate `validate()` that skipped this check —
    /// a scheduled goal could carry a NUL byte or ANSI escape straight
    /// through to the cron log and the dispatched task.
    #[test]
    fn validate_rejects_control_char_in_goal() {
        let mut body = valid_body();
        body.goal = "do the thing\u{0007}".to_string();
        assert!(body.validate().is_err());
    }

    #[test]
    fn validate_accepts_a_well_formed_body() {
        assert!(valid_body().validate().is_ok());
    }
}
