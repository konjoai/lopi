//! MAXX (opportunistic backlog dispatch) REST surface. Mirrors
//! `schedule_handlers.rs`'s shape and conventions exactly — CRUD plus
//! enable/disable, minus `run-now`/`autonomy` (MAXX entries fire on
//! favorability, not on demand, and share the schedule autonomy convention
//! without needing its own setter route in this sprint).
//!
//! Routes (all behind the shared Bearer-auth + rate-limit middleware):
//! - `GET    /api/maxx`            — list with run history's last run
//! - `POST   /api/maxx`            — create
//! - `GET    /api/maxx/:id`        — one entry + run history
//! - `PUT    /api/maxx/:id`        — edit
//! - `DELETE /api/maxx/:id`        — delete
//! - `POST   /api/maxx/:id/enable` — enable
//! - `POST   /api/maxx/:id/disable`— disable

use super::types::MAX_GOAL_LENGTH;
use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_core::LimitWindow;
use lopi_memory::{MaxxInput, MaxxRow};
use serde::Deserialize;
use serde_json::{json, Value};

/// Create/update body. `enabled` defaults to `true` on create.
#[derive(Debug, Deserialize)]
pub(super) struct MaxxBody {
    pub name: String,
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
    #[serde(default)]
    pub autonomy_level: Option<String>,
    #[serde(default)]
    pub report: Option<String>,
    /// `(start, end)` local hours, e.g. `[23, 7]` for 11PM-7AM.
    #[serde(default)]
    pub quiet_hours: Option<(u8, u8)>,
    #[serde(default)]
    pub headroom_gate: bool,
    /// Window tags: `"five_hour"` / `"seven_day"`.
    #[serde(default)]
    pub windows: Vec<String>,
}

impl MaxxBody {
    /// Validate inputs at the API boundary. Returns an error message on failure.
    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("name must not be empty".into());
        }
        if self.goal.trim().is_empty() {
            return Err("goal must not be empty".into());
        }
        if self.goal.len() > MAX_GOAL_LENGTH {
            return Err(format!("goal exceeds {MAX_GOAL_LENGTH} chars"));
        }
        if let Some((start, end)) = self.quiet_hours {
            if start > 23 || end > 23 {
                return Err("quiet_hours must be within 0..=23".into());
            }
        }
        if let Some(bad) = self.windows.iter().find(|w| LimitWindow::parse(w).is_none()) {
            return Err(format!("unknown window '{bad}' (expected five_hour/seven_day)"));
        }
        Ok(())
    }

    fn into_input(self, id: Option<String>) -> MaxxInput {
        MaxxInput {
            id,
            name: self.name,
            goal: self.goal,
            repo: self.repo,
            priority: self.priority.unwrap_or_else(|| "normal".into()),
            allowed_dirs: self.allowed_dirs.unwrap_or_default(),
            forbidden_dirs: self.forbidden_dirs.unwrap_or_default(),
            enabled: self.enabled.unwrap_or(true),
            autonomy_level: self.autonomy_level.unwrap_or_default(),
            report: self.report,
            quiet_hours_start: self.quiet_hours.map(|(s, _)| s),
            quiet_hours_end: self.quiet_hours.map(|(_, e)| e),
            headroom_gate: self.headroom_gate,
            windows: self.windows,
        }
    }
}

pub(super) async fn list_maxx(State(s): State<AppState>) -> impl IntoResponse {
    match s.store.list_maxx_entries().await {
        Ok(rows) => {
            let mut body = Vec::with_capacity(rows.len());
            for row in rows {
                body.push(maxx_to_json(&s, row).await);
            }
            (StatusCode::OK, Json(json!({ "maxx": body }))).into_response()
        }
        Err(e) => server_error(&e),
    }
}

pub(super) async fn create_maxx(
    State(s): State<AppState>,
    Json(body): Json<MaxxBody>,
) -> impl IntoResponse {
    if let Err(msg) = body.validate() {
        return unprocessable(&msg);
    }
    match s.store.upsert_maxx_entry(&body.into_input(None)).await {
        Ok(row) => (StatusCode::CREATED, Json(maxx_to_json(&s, row).await)).into_response(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn get_maxx(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.get_maxx_entry(&id).await {
        Ok(Some(row)) => {
            let runs = s.store.list_maxx_runs(&id, 20).await.unwrap_or_default();
            let mut value = maxx_to_json(&s, row).await;
            value["runs"] = json!(runs);
            (StatusCode::OK, Json(value)).into_response()
        }
        Ok(None) => not_found(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn update_maxx(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(body): Json<MaxxBody>,
) -> impl IntoResponse {
    if let Err(msg) = body.validate() {
        return unprocessable(&msg);
    }
    match s.store.get_maxx_entry(&id).await {
        Ok(Some(_)) => match s.store.upsert_maxx_entry(&body.into_input(Some(id))).await {
            Ok(row) => (StatusCode::OK, Json(maxx_to_json(&s, row).await)).into_response(),
            Err(e) => server_error(&e),
        },
        Ok(None) => not_found(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn delete_maxx(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.delete_maxx_entry(&id).await {
        Ok(true) => (StatusCode::OK, Json(json!({ "deleted": id }))).into_response(),
        Ok(false) => not_found(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn enable_maxx(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    set_enabled(&s, &id, true).await
}

pub(super) async fn disable_maxx(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    set_enabled(&s, &id, false).await
}

async fn set_enabled(s: &AppState, id: &str, enabled: bool) -> axum::response::Response {
    match s.store.set_maxx_enabled(id, enabled).await {
        Ok(true) => (
            StatusCode::OK,
            Json(json!({ "id": id, "enabled": enabled })),
        )
            .into_response(),
        Ok(false) => not_found(),
        Err(e) => server_error(&e),
    }
}

/// Serialize a MAXX entry plus its last run.
async fn maxx_to_json(s: &AppState, row: MaxxRow) -> Value {
    let last_run = s
        .store
        .list_maxx_runs(&row.id, 1)
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    json!({
        "id": row.id, "name": row.name, "goal": row.goal,
        "repo": row.repo, "priority": row.priority,
        "allowed_dirs": row.allowed_dirs, "forbidden_dirs": row.forbidden_dirs,
        "enabled": row.enabled, "autonomy_level": row.autonomy_level, "report": row.report,
        "quiet_hours": match (row.quiet_hours_start, row.quiet_hours_end) {
            (Some(s), Some(e)) => json!([s, e]),
            _ => Value::Null,
        },
        "headroom_gate": row.headroom_gate, "windows": row.windows,
        "created_at": row.created_at, "updated_at": row.updated_at, "last_run": last_run,
    })
}

fn not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "maxx entry not found" })),
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
    tracing::warn!("maxx handler error: {e:#}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": format!("{e:#}") })),
    )
        .into_response()
}
