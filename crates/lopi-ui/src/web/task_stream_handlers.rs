//! Per-task SSE stream + historical log reader.
//!
//! Routes:
//! - `GET /api/tasks/:id/stream`    — Server-Sent Events scoped to one
//!   task. Subscribes to the raw `AgentEvent` bus, filters every event
//!   by `task_id`, serializes the survivors inline. Lagging clients
//!   drop frames (the existing `serialized_tx` bridge tolerates the
//!   same loss for global subscribers).
//! - `GET /api/tasks/:id/logs?n=N`  — historical tail from the
//!   `task_logs` ring buffer, oldest-first, clamped to N ≤ 5000.
//! - `GET /api/logs?n=N`            — global tail across all tasks,
//!   oldest-first, same clamp. Backs the dashboard Logs tab.

use super::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json,
    },
};
use futures::StreamExt as _;
use lopi_core::AgentEvent;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_stream::wrappers::BroadcastStream;

#[derive(Debug, Deserialize)]
pub(super) struct LogsParams {
    /// Page size. Defaults to 200, clamped by the store to [1, 5000].
    #[serde(default)]
    pub n: Option<i64>,
}

/// `GET /api/tasks/:id/stream` — server-sent events scoped to `:id`.
pub(super) async fn stream_task(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    // Validate the task id up front so a malformed request can't burn
    // a broadcast subscriber slot for nothing.
    let Ok(uuid) = id.parse::<uuid::Uuid>() else {
        return Json(json!({"error": "task id must be a uuid"})).into_response();
    };
    let target_id = lopi_core::TaskId(uuid);
    let rx = s.bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |r: Result<AgentEvent, _>| async move {
        match r {
            Ok(ev) => {
                let event_tid = event_task_id(&ev)?;
                if event_tid != target_id {
                    return None;
                }
                serde_json::to_string(&ev)
                    .ok()
                    .map(|s| Ok::<Event, std::convert::Infallible>(Event::default().data(s)))
            }
            // lagged — skip the dropped events; the historical /logs
            // endpoint still has them.
            Err(_) => None,
        }
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `GET /api/tasks/:id/logs?n=N` — historical tail, oldest first.
pub(super) async fn get_logs(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Query(params): Query<LogsParams>,
) -> impl IntoResponse {
    let n = params.n.unwrap_or(200);
    match s.store.load_task_logs(&id, n).await {
        Ok(rows) => {
            let body = log_rows_to_json(rows);
            (StatusCode::OK, Json(json!({ "task_id": id, "logs": body }))).into_response()
        }
        Err(e) => {
            tracing::warn!("load_task_logs failed: {e}");
            logs_internal_error(e)
        }
    }
}

/// `GET /api/logs?n=N` — global historical tail across all tasks,
/// oldest first.
pub(super) async fn get_recent_logs(
    State(s): State<AppState>,
    Query(params): Query<LogsParams>,
) -> impl IntoResponse {
    let n = params.n.unwrap_or(200);
    match s.store.load_recent_task_logs(n).await {
        Ok(rows) => {
            let body = log_rows_to_json(rows);
            (StatusCode::OK, Json(json!({ "logs": body }))).into_response()
        }
        Err(e) => {
            tracing::warn!("load_recent_task_logs failed: {e}");
            logs_internal_error(e)
        }
    }
}

/// Shared row→JSON mapping for both the per-task and global log-tail routes.
fn log_rows_to_json(rows: Vec<lopi_memory::TaskLogRow>) -> Vec<Value> {
    rows.into_iter()
        .map(|r| {
            json!({
                "id":      r.id,
                "task_id": r.task_id,
                "ts":      r.ts,
                "level":   r.level,
                "line":    r.line,
            })
        })
        .collect()
}

/// Shared 500 body for a store-layer failure, formatted with the error's
/// full context chain (`{:#}`).
fn logs_internal_error(e: impl std::fmt::Display) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": format!("{e:#}")})),
    )
        .into_response()
}

/// Extract a `TaskId` from any `AgentEvent` variant that carries one.
/// Returns `None` for global events (`PoolStats`, `BudgetExceeded` with
/// no task scope) so the stream doesn't emit unrelated traffic.
fn event_task_id(ev: &AgentEvent) -> Option<lopi_core::TaskId> {
    match ev {
        AgentEvent::TaskQueued { task_id, .. }
        | AgentEvent::TaskStarted { task_id, .. }
        | AgentEvent::StatusChanged { task_id, .. }
        | AgentEvent::LogLine { task_id, .. }
        | AgentEvent::ScoreUpdated { task_id, .. }
        | AgentEvent::TaskCompleted { task_id, .. }
        | AgentEvent::TaskCancelled { task_id }
        | AgentEvent::PlanProposed { task_id, .. }
        | AgentEvent::TurnMetrics { task_id, .. }
        | AgentEvent::ToolCall { task_id, .. }
        | AgentEvent::ToolResult { task_id, .. }
        | AgentEvent::TokenDelta { task_id, .. }
        | AgentEvent::ApiRetry { task_id, .. }
        | AgentEvent::Cost { task_id, .. }
        | AgentEvent::Phase { task_id, .. }
        | AgentEvent::ReportReady { task_id, .. } => Some(*task_id),
        AgentEvent::PoolStats { .. } => None,
        AgentEvent::BudgetExceeded { task_id, .. } => *task_id,
        AgentEvent::VerifierVerdict { task_id, .. } => Some(*task_id),
    }
}
