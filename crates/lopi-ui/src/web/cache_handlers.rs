//! P2 — REST handlers for the result cache.
//!
//! Routes:
//! - `GET    /api/cache/stats`          — aggregate hit-rate + size
//! - `DELETE /api/cache`                — wipe everything, returns `{deleted: N}`
//! - `DELETE /api/cache/agent/:agent`   — purge one agent's entries
//!
//! Behind the same Bearer auth + per-IP rate limit as the rest of `/api`.

use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde_json::{json, Value};

pub(super) async fn cache_stats_handler(State(s): State<AppState>) -> impl IntoResponse {
    match s.store.cache_stats().await {
        Ok(stats) => (StatusCode::OK, Json(json!(stats))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}

pub(super) async fn clear_cache_handler(State(s): State<AppState>) -> Json<Value> {
    let deleted = s.store.clear_cache().await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "clear_cache failed");
        0
    });
    Json(json!({ "deleted": deleted }))
}

pub(super) async fn invalidate_agent_cache_handler(
    Path(agent_id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.invalidate_cache_for_agent(&agent_id).await {
        Ok(deleted) => (
            StatusCode::OK,
            Json(json!({ "agent_id": agent_id, "deleted": deleted })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}
