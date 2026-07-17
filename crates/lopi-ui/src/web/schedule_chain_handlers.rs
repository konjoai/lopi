//! Stack-Chain-1 — whole-stack cron chain REST surface.
//!
//! Sibling to [`super::schedule_handlers`], but a `schedules` row can only
//! carry one goal — this is the backend for the stack control dock's
//! "schedule the entire stack" cron, which needs to fire an ORDERED SEQUENCE
//! of independent goals (one per stack card), not a single one.
//!
//! Persists in `schedule_chains` / `schedule_chain_steps`
//! (`lopi_memory::ScheduleChainRow`); the live
//! [`ChainScheduleManager`](lopi_orchestrator::ChainScheduleManager)
//! registers enabled chains as cron jobs and drives each fire step-by-step,
//! entirely server-side (see that module's docs for the restart-resume
//! contract). These handlers keep the store and the live job in sync, same
//! as `schedule_handlers`.
//!
//! Routes (all behind the shared Bearer-auth + rate-limit middleware):
//! - `GET    /api/schedule-chains`            — list with next-runs + last-run
//! - `POST   /api/schedule-chains`            — create
//! - `GET    /api/schedule-chains/:id`        — one chain + run history
//! - `PUT    /api/schedule-chains/:id`        — edit (replaces steps)
//! - `DELETE /api/schedule-chains/:id`        — delete
//! - `POST   /api/schedule-chains/:id/enable` — enable + register
//! - `POST   /api/schedule-chains/:id/disable`— disable + unregister
//! - `POST   /api/schedule-chains/:id/run-now`— fire immediately

use super::types::MAX_GOAL_LENGTH;
use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_memory::{ChainStepInput, ScheduleChainInput, ScheduleChainRow};
use lopi_orchestrator::next_run_times;
use serde::Deserialize;
use serde_json::{json, Value};

/// One step in a create/update body — mirrors a single stack card.
#[derive(Debug, Deserialize)]
pub(super) struct ChainStepBody {
    pub goal: String,
    #[serde(default)]
    pub allowed_dirs: Option<Vec<String>>,
    #[serde(default)]
    pub forbidden_dirs: Option<Vec<String>>,
}

/// Create/update body. `enabled` defaults to `true` on create.
#[derive(Debug, Deserialize)]
pub(super) struct ScheduleChainBody {
    pub name: String,
    pub cron: String,
    pub steps: Vec<ChainStepBody>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub autonomy_level: Option<String>,
    /// `stop` / `continue` / `backoff` — mirrors the client `OnFail` union.
    #[serde(default)]
    pub on_fail: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

impl ScheduleChainBody {
    /// Validate inputs at the API boundary. Returns an error message on failure.
    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("name must not be empty".into());
        }
        if next_run_times(&self.cron, 1).is_empty() {
            return Err("invalid cron expression (expected 5 fields)".into());
        }
        if self.steps.is_empty() {
            return Err("chain must have at least one step".into());
        }
        for (i, step) in self.steps.iter().enumerate() {
            if step.goal.trim().is_empty() {
                return Err(format!("step {i} goal must not be empty"));
            }
            if step.goal.len() > MAX_GOAL_LENGTH {
                return Err(format!("step {i} goal exceeds {MAX_GOAL_LENGTH} chars"));
            }
        }
        Ok(())
    }

    fn into_input(self, id: Option<String>) -> ScheduleChainInput {
        ScheduleChainInput {
            id,
            name: self.name,
            cron: self.cron,
            repo: self.repo,
            priority: self.priority.unwrap_or_else(|| "normal".into()),
            autonomy_level: self.autonomy_level.unwrap_or_default(),
            on_fail: self.on_fail.unwrap_or_else(|| "stop".into()),
            enabled: self.enabled.unwrap_or(true),
            steps: self
                .steps
                .into_iter()
                .map(|s| ChainStepInput {
                    goal: s.goal,
                    allowed_dirs: s.allowed_dirs.unwrap_or_default(),
                    forbidden_dirs: s.forbidden_dirs.unwrap_or_default(),
                })
                .collect(),
        }
    }
}

pub(super) async fn list_chains(State(s): State<AppState>) -> impl IntoResponse {
    match s.store.list_schedule_chains().await {
        Ok(rows) => {
            let mut body = Vec::with_capacity(rows.len());
            for row in rows {
                body.push(chain_to_json(&s, row).await);
            }
            (StatusCode::OK, Json(json!({ "chains": body }))).into_response()
        }
        Err(e) => server_error(&e),
    }
}

pub(super) async fn create_chain(
    State(s): State<AppState>,
    Json(body): Json<ScheduleChainBody>,
) -> impl IntoResponse {
    if let Err(msg) = body.validate() {
        return unprocessable(&msg);
    }
    persist_and_register(&s, body.into_input(None), StatusCode::CREATED).await
}

pub(super) async fn get_chain(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.get_schedule_chain(&id).await {
        Ok(Some(row)) => {
            let runs = s.store.list_chain_runs(&id, 20).await.unwrap_or_default();
            let mut value = chain_to_json(&s, row).await;
            value["runs"] = json!(runs);
            (StatusCode::OK, Json(value)).into_response()
        }
        Ok(None) => not_found(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn update_chain(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(body): Json<ScheduleChainBody>,
) -> impl IntoResponse {
    if let Err(msg) = body.validate() {
        return unprocessable(&msg);
    }
    match s.store.get_schedule_chain(&id).await {
        Ok(Some(_)) => persist_and_register(&s, body.into_input(Some(id)), StatusCode::OK).await,
        Ok(None) => not_found(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn delete_chain(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.delete_schedule_chain(&id).await {
        Ok(true) => {
            if let Err(e) = s.schedule_chains.unregister(&id).await {
                tracing::warn!("unregister chain after delete failed: {e:#}");
            }
            (StatusCode::OK, Json(json!({ "deleted": id }))).into_response()
        }
        Ok(false) => not_found(),
        Err(e) => server_error(&e),
    }
}

pub(super) async fn enable_chain(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    set_enabled(&s, &id, true).await
}

pub(super) async fn disable_chain(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    set_enabled(&s, &id, false).await
}

pub(super) async fn run_now(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.get_schedule_chain(&id).await {
        Ok(Some(row)) => match s.schedule_chains.run_now(row.into()).await {
            Ok(run_id) => (
                StatusCode::ACCEPTED,
                Json(json!({ "chain_id": id, "run_id": run_id, "queued": run_id.is_some() })),
            )
                .into_response(),
            Err(e) => server_error(&e),
        },
        Ok(None) => not_found(),
        Err(e) => server_error(&e),
    }
}

/// Upsert a chain then (un)register its live job to match `enabled`.
async fn persist_and_register(
    s: &AppState,
    input: ScheduleChainInput,
    ok: StatusCode,
) -> axum::response::Response {
    let row = match s.store.upsert_schedule_chain(&input).await {
        Ok(r) => r,
        Err(e) => return server_error(&e),
    };
    sync_job(s, &row).await;
    let value = chain_to_json(s, row).await;
    (ok, Json(value)).into_response()
}

/// Flip a chain's enabled flag and bring its live job into line.
async fn set_enabled(s: &AppState, id: &str, enabled: bool) -> axum::response::Response {
    match s.store.set_schedule_chain_enabled(id, enabled).await {
        Ok(true) => {
            if let Ok(Some(row)) = s.store.get_schedule_chain(id).await {
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
async fn sync_job(s: &AppState, row: &ScheduleChainRow) {
    let result = if row.enabled {
        s.schedule_chains
            .register(row.clone().into())
            .await
            .map(|_| ())
    } else {
        s.schedule_chains.unregister(&row.id).await
    };
    if let Err(e) = result {
        tracing::warn!(chain = %row.id, "chain scheduler sync failed: {e:#}");
    }
}

/// Serialize a chain plus its computed next-run times and last run.
async fn chain_to_json(s: &AppState, row: ScheduleChainRow) -> Value {
    let next_runs: Vec<String> = next_run_times(&row.cron, 3)
        .into_iter()
        .map(|t| t.to_rfc3339())
        .collect();
    let last_run = s
        .store
        .list_chain_runs(&row.id, 1)
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    json!({
        "id": row.id, "name": row.name, "cron": row.cron,
        "repo": row.repo, "priority": row.priority,
        "autonomy_level": row.autonomy_level, "on_fail": row.on_fail,
        "enabled": row.enabled, "steps": row.steps,
        "created_at": row.created_at, "updated_at": row.updated_at,
        "next_runs": next_runs, "last_run": last_run,
    })
}

fn not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "schedule chain not found" })),
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
    tracing::warn!("schedule chain handler error: {e:#}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": format!("{e:#}") })),
    )
        .into_response()
}
