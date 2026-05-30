//! Per-agent rate-limit REST surface.
//!
//! Routes:
//! - `POST   /api/agents/:id/rate-limit` — register or replace limits.
//!   Body: `{max_per_minute: u32, max_concurrent: u32}`. Returns 201
//!   on insert, 422 if `max_per_minute == 0`.
//! - `GET    /api/agents/:id/rate-limit` — snapshot the limits + the
//!   current in-flight count. 404 if the agent was never registered.
//! - `DELETE /api/agents/:id/rate-limit` — deregister.
//!
//! Behind the same Bearer-auth + per-IP rate-limit middleware as the
//! rest of `/api/*`. Note: this is per-agent task throttling, distinct
//! from the HTTP-side limiter that gates all /api/* requests.

use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_orchestrator::AgentRateLimit;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub(super) struct RateLimitBody {
    pub max_per_minute: u32,
    /// 0 = no concurrency cap, only the per-minute bucket applies.
    #[serde(default)]
    pub max_concurrent: u32,
}

pub(super) async fn register_rate_limit(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(body): Json<RateLimitBody>,
) -> impl IntoResponse {
    let limit = AgentRateLimit {
        max_per_minute: body.max_per_minute,
        max_concurrent: body.max_concurrent,
    };
    if !s.pool.register_agent_rate_limit(&id, limit) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "max_per_minute must be > 0"})),
        )
            .into_response();
    }
    let snap = s.pool.agent_rate_limit(&id);
    (
        StatusCode::CREATED,
        Json(snap.map_or_else(
            || json!({"agent_id": id.clone(), "registered": true}),
            snapshot_to_json,
        )),
    )
        .into_response()
}

pub(super) async fn get_rate_limit(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.pool.agent_rate_limit(&id) {
        Some(snap) => (StatusCode::OK, Json(snapshot_to_json(snap))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no rate limit registered for this agent"})),
        )
            .into_response(),
    }
}

pub(super) async fn delete_rate_limit(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    if s.pool.deregister_agent_rate_limit(&id) {
        (StatusCode::OK, Json(json!({"deregistered": id}))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no rate limit registered for this agent"})),
        )
            .into_response()
    }
}

fn snapshot_to_json(s: lopi_orchestrator::AgentRateLimitSnapshot) -> Value {
    json!({
        "agent_id":       s.agent_id,
        "max_per_minute": s.max_per_minute,
        "max_concurrent": s.max_concurrent,
        "in_flight":      s.in_flight,
    })
}
