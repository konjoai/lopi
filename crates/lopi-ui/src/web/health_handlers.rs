//! Agent health monitoring REST surface.
//!
//! Routes:
//! - `POST /api/agents/:id/heartbeat` — agent self-reports liveness;
//!   returns the current snapshot.
//! - `GET  /api/agents/:id/health`     — snapshot for one agent (404 if
//!   the agent has never reported).
//! - `GET  /api/agents/health/summary` — fleet-wide rollup.
//!
//! All sit behind the same Bearer-auth + rate-limit middleware as the
//! rest of `/api/*`.

use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde_json::{json, Value};

pub(super) async fn heartbeat(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    s.health.heartbeat(&id).await;
    match s.health.snapshot(&id).await {
        Some(snap) => (StatusCode::OK, Json(snapshot_to_json(&snap))).into_response(),
        // Should never happen — heartbeat creates the entry — but encode
        // it defensively rather than panic.
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "heartbeat recorded but snapshot missing"})),
        )
            .into_response(),
    }
}

pub(super) async fn get_health(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.health.snapshot(&id).await {
        Some(snap) => (StatusCode::OK, Json(snapshot_to_json(&snap))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no heartbeat recorded for this agent"})),
        )
            .into_response(),
    }
}

pub(super) async fn health_summary(State(s): State<AppState>) -> impl IntoResponse {
    let sum = s.health.summary();
    (
        StatusCode::OK,
        Json(json!({
            "total":    sum.total,
            "healthy":  sum.healthy,
            "degraded": sum.degraded,
            "dead":     sum.dead,
        })),
    )
}

fn snapshot_to_json(snap: &lopi_orchestrator::HealthSnapshot) -> Value {
    json!({
        "agent_id":             snap.agent_id,
        "status":               snap.status.as_str(),
        "last_seen":            snap.last_seen,
        "error_rate_1h":        snap.error_rate_1h,
        "avg_latency_ms":       snap.avg_latency_ms,
        "consecutive_failures": snap.consecutive_failures,
        "samples":              snap.samples,
    })
}
