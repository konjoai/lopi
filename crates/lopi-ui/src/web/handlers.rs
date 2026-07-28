//! Route handlers for the lopi web API.
//!
//! Separated from `web/mod.rs` to keep that file within the 500-line budget.
//! All functions are imported into `mod.rs` via `use handlers::*`.

use super::demo_guard;
use super::task_fields;
use super::types::{CreateTaskRequest, CreateTaskResponse};
use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_core::{Priority, Provenance, Task, TaskId};
use lopi_memory::CheckpointInput;
use lopi_spec::SpecSurface;
use serde::Deserialize;
use serde_json::{json, Value};

// Re-exported so external test modules (`crate::web::handlers::apply_loop_fields`
// et al.) keep working unchanged after this sprint split the field-mapping
// helpers out into their own module to stay under the file-size gate.
pub(super) use task_fields::{apply_loop_fields, validate_goal};
// Only referenced by name (`crate::web::handlers::ApplyLoopFieldsError`) from
// test modules — `create_task` below only calls `.to_string()` on the error,
// so a non-test build never names the type itself.
#[cfg(test)]
pub(super) use task_fields::ApplyLoopFieldsError;

pub(super) async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "service": "lopi" }))
}

pub(super) async fn get_stats(State(s): State<AppState>) -> impl IntoResponse {
    // Lifecycle counts come from the durable store, not `pool.stats()`: those
    // in-memory counters are per-pool, so in multi-repo mode the primary pool
    // misses every task dispatched to an extra repo (Verify-1 F3/F4 — "N live"
    // read 1 while 2 ran, `succeeded` 3 against 7). The DB is shared across all
    // pools. `uptime_secs` stays sourced from the pool — it is a server-lifetime
    // clock, not a per-task tally.
    let counts = s.store.status_counts().await.unwrap_or_else(|e| {
        tracing::warn!("status_counts query failed: {e}");
        Default::default()
    });
    let uptime_secs = s.pool.stats().uptime_secs;
    let (total_tokens_today, total_cost_usd_today) =
        s.store.daily_token_totals().await.unwrap_or_else(|e| {
            tracing::warn!("daily_token_totals query failed: {e}");
            (0, 0.0)
        });
    let synthetic = s.store.is_synthetic().await.unwrap_or(false);
    // See `docs/MEASUREMENT.md`: `total_cost_usd_today`/`total_tokens_today`
    // are counted by lopi itself from its own `turn_metrics` rows — never
    // named bare `"provenance"`, which already carries `TaskRow::provenance()`'s
    // unrelated trust meaning elsewhere on the wire.
    let measurement_provenance = serde_json::to_value(Provenance::measured(
        "turn_metrics table — lopi's own agent runs, this process",
    ))
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, "measurement_provenance serialization failed");
        Value::Null
    });
    Json(json!({
        "running": counts.running, "queued": counts.queued,
        "succeeded": counts.succeeded, "failed": counts.failed,
        "uptime_secs": uptime_secs,
        "total_tokens_today": total_tokens_today,
        "total_cost_usd_today": total_cost_usd_today,
        "synthetic": synthetic,
        "measurement_provenance": measurement_provenance,
    }))
}

pub(super) async fn list_tasks(State(s): State<AppState>) -> Json<Value> {
    let rows = s.store.load_history(100).await.unwrap_or_default();
    let costs = s.store.task_costs().await.unwrap_or_default();
    let body: Vec<_> = rows
        .into_iter()
        .map(|t| {
            let cost = costs.get(&t.id).copied().unwrap_or(0.0);
            json!({
                "id": t.id, "goal": t.goal, "status": t.status,
                "created_at": t.created_at, "completed_at": t.completed_at,
                "client_ref": t.client_ref, "cost": cost, "repo": t.repo,
                "provenance": t.provenance(),
            })
        })
        .collect();
    Json(json!({ "tasks": body }))
}

pub(super) async fn get_task(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    let rows = s.store.load_history(500).await.unwrap_or_default();
    match find_by_id_prefix(rows, &id, |t| t.id.as_str()) {
        PrefixMatch::Unique(t) => {
            let cost = s
                .store
                .task_costs()
                .await
                .unwrap_or_default()
                .get(&t.id)
                .copied()
                .unwrap_or(0.0);
            (
                StatusCode::OK,
                Json(json!({
                    "id": t.id, "goal": t.goal, "status": t.status,
                    "created_at": t.created_at, "completed_at": t.completed_at,
                    "client_ref": t.client_ref, "cost": cost, "repo": t.repo,
                    "provenance": t.provenance(),
                })),
            )
                .into_response()
        }
        PrefixMatch::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "task not found" })),
        )
            .into_response(),
        PrefixMatch::Ambiguous => ambiguous_prefix_response(),
    }
}

/// Outcome of a lookup by a full id or a possibly-partial prefix.
enum PrefixMatch<T> {
    /// No entity's id starts with the given prefix.
    NotFound,
    /// Exactly one match — the unambiguous case.
    Unique(T),
    /// More than one entity's id starts with the given prefix. The caller
    /// must not silently act on an arbitrary one of them (`Iterator::find`'s
    /// usual first-match behavior) — surface this to the client instead.
    Ambiguous,
}

/// Find the row in `rows` whose `id_of` value starts with `prefix`,
/// distinguishing a unique match from an ambiguous one.
fn find_by_id_prefix<T>(rows: Vec<T>, prefix: &str, id_of: impl Fn(&T) -> &str) -> PrefixMatch<T> {
    let mut matches = rows.into_iter().filter(|t| id_of(t).starts_with(prefix));
    let Some(first) = matches.next() else {
        return PrefixMatch::NotFound;
    };
    if matches.next().is_some() {
        return PrefixMatch::Ambiguous;
    }
    PrefixMatch::Unique(first)
}

/// `409` response for an id prefix that matches more than one task.
fn ambiguous_prefix_response() -> axum::response::Response {
    (
        StatusCode::CONFLICT,
        Json(json!({ "error": "id prefix matches more than one task" })),
    )
        .into_response()
}

/// Body for `POST /api/agents/:id/checkpoint`. All fields optional — only
/// `state` is required because every checkpoint must carry a phase label.
#[derive(Debug, Deserialize)]
pub(super) struct CheckpointBody {
    /// Required. Lowercase phase: planning / implementing / testing /
    /// scoring / done / errored.
    pub state: String,
    /// Optional attempt number (defaults to 0 — the runner usually sets it).
    #[serde(default)]
    pub attempt: u8,
    pub last_plan: Option<String>,
    pub last_score: Option<String>,
    pub repo_path: Option<String>,
    pub context_hash: Option<String>,
}

/// P1.3 — durable checkpoint on demand. Persists an `agent_checkpoints`
/// row keyed by `task_id` so `lopi resume --agent-id` can recover.
pub(super) async fn checkpoint_agent(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(body): Json<CheckpointBody>,
) -> impl IntoResponse {
    let Ok(uuid) = id.parse::<uuid::Uuid>() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "agent id must be a uuid"})),
        )
            .into_response();
    };
    let mut input = CheckpointInput::new(TaskId(uuid), body.attempt, body.state);
    input.last_plan = body.last_plan;
    input.last_score = body.last_score;
    input.repo_path = body.repo_path;
    input.context_hash = body.context_hash;
    match s.store.save_checkpoint(&input).await {
        Ok(checkpoint_id) => (
            StatusCode::CREATED,
            Json(json!({ "checkpoint_id": checkpoint_id, "task_id": id })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}

pub(super) async fn cancel_task(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    if demo_guard::is_demo(&s).await {
        return demo_guard::refuse_mutation("cancel a task").into_response();
    }
    let rows = s.store.load_history(500).await.unwrap_or_default();
    let t = match find_by_id_prefix(rows, &id, |t| t.id.as_str()) {
        PrefixMatch::Unique(t) => t,
        PrefixMatch::NotFound => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
                .into_response();
        }
        PrefixMatch::Ambiguous => return ambiguous_prefix_response(),
    };
    let Ok(uuid) = t.id.parse::<uuid::Uuid>() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid id"})),
        )
            .into_response();
    };
    let task_id = TaskId(uuid);
    // First cancel any running execution, then permanently delete the task
    // and its dependent rows. Otherwise the snapshot endpoint would resurrect
    // the closed session on the next dashboard reload.
    let cancelled = s.pool.cancel(&task_id).await;
    let deleted = match s.store.delete_task(&task_id).await {
        Ok(removed) => removed,
        Err(e) => {
            tracing::warn!(error = %e, task_id = %t.id, "delete_task failed");
            false
        }
    };
    (
        StatusCode::OK,
        Json(json!({
            "id": t.id,
            "cancelled": cancelled,
            "deleted": deleted,
        })),
    )
        .into_response()
}

/// Phase 11 — approve a paused plan; the agent proceeds to implementation.
pub(super) async fn approve_plan(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    if demo_guard::is_demo(&s).await {
        return demo_guard::refuse_mutation("approve a plan").into_response();
    }
    decide_plan(&s, &id, lopi_core::PlanDecision::Approve).await
}

/// Phase 11 — reject a paused plan; the agent abandons the task.
pub(super) async fn reject_plan(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    if demo_guard::is_demo(&s).await {
        return demo_guard::refuse_mutation("reject a plan").into_response();
    }
    decide_plan(&s, &id, lopi_core::PlanDecision::Reject).await
}

/// Deliver a plan decision to a paused runner, resolving `id` (full UUID or a
/// prefix) to a task and signalling the pool.
async fn decide_plan(
    s: &AppState,
    id: &str,
    decision: lopi_core::PlanDecision,
) -> axum::response::Response {
    let task_id = match resolve_task_id(s, id).await {
        PrefixMatch::Unique(task_id) => task_id,
        PrefixMatch::NotFound => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
                .into_response();
        }
        PrefixMatch::Ambiguous => return ambiguous_prefix_response(),
    };
    if s.pool.decide_plan(&task_id, decision).await {
        (
            StatusCode::OK,
            Json(json!({"task_id": task_id.0.to_string(), "decision": decision})),
        )
            .into_response()
    } else {
        (
            StatusCode::CONFLICT,
            Json(json!({"error": "task is not awaiting plan approval"})),
        )
            .into_response()
    }
}

/// Resolve a task id from a full UUID or a unique prefix (history fallback).
async fn resolve_task_id(s: &AppState, id: &str) -> PrefixMatch<TaskId> {
    if let Ok(uuid) = id.parse::<uuid::Uuid>() {
        return PrefixMatch::Unique(TaskId(uuid));
    }
    let rows = s.store.load_history(500).await.unwrap_or_default();
    match find_by_id_prefix(rows, id, |t| t.id.as_str()) {
        PrefixMatch::Unique(t) => match t.id.parse::<uuid::Uuid>() {
            Ok(uuid) => PrefixMatch::Unique(TaskId(uuid)),
            Err(_) => PrefixMatch::NotFound,
        },
        PrefixMatch::NotFound => PrefixMatch::NotFound,
        PrefixMatch::Ambiguous => PrefixMatch::Ambiguous,
    }
}

pub(super) async fn create_task(
    State(s): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    if demo_guard::is_demo(&s).await {
        return demo_guard::refuse_mutation("create a task").into_response();
    }
    if let Err(reason) = validate_goal(&req.goal) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": reason })),
        )
            .into_response();
    }

    let mut task = Task::new(req.goal.clone());
    if let Err(e) = apply_loop_fields(&mut task, &req) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": e.to_string()})),
        )
            .into_response();
    }
    task.priority = match req.priority.as_deref() {
        Some("low") => Priority::Low,
        Some("high") => Priority::High,
        Some("critical") => Priority::Critical,
        _ => Priority::Normal,
    };
    if let Some(repo) = req.repo {
        task.repo_path = Some(std::path::PathBuf::from(repo));
    }
    if let Some(dirs) = req.allowed_dirs {
        task.allowed_dirs = dirs;
    }
    if let Some(dirs) = req.forbidden_dirs {
        task.forbidden_dirs = dirs;
    }
    if let Some(c) = req.constraints {
        task.constraints = c;
    }
    if let Some(r) = req.max_retries {
        task.max_retries = r;
    }
    task.require_plan_approval = req.require_plan_approval.unwrap_or(false);
    task.client_ref = req.client_ref.clone();

    let task_id = task.id.0.to_string();
    let client_ref = task.client_ref.clone();
    let duplicate_of = s.pool.submit(task).await.map(|id| id.0.to_string());
    let resp = CreateTaskResponse {
        id: task_id,
        goal: req.goal,
        queued: duplicate_of.is_none(),
        duplicate_of,
        client_ref,
    };
    (StatusCode::CREATED, Json(resp)).into_response()
}

/// `GET /api/spec` — returns the cached or freshly-extracted spec surface.
pub(super) async fn get_spec(State(s): State<AppState>) -> impl IntoResponse {
    let surface = match SpecSurface::load(&s.repo_path) {
        Ok(Some(cached)) => cached,
        _ => match SpecSurface::extract(&s.repo_path) {
            Ok(live) => live,
            Err(e) => {
                tracing::warn!("spec extract failed: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "spec extraction failed")
                    .into_response();
            }
        },
    };
    Json(serde_json::json!({
        "count": surface.len(),
        "rust_files_scanned": surface.rust_files_scanned,
        "python_files_scanned": surface.python_files_scanned,
        "extracted_at": surface.extracted_at,
        "items": surface.items,
    }))
    .into_response()
}
