//! P2 — REST handlers for the constellation router.
//!
//! Routes:
//! - `POST /api/constellations`               — create/replace, returns 201/200
//! - `GET  /api/constellations`               — list every constellation
//! - `POST /api/constellation/:name/dispatch` — pick an agent for a task,
//!   body: `{ "required_tags": ["..."]? }` (optional); returns
//!   `{ agent_id, strategy, at, required_tags }`
//! - `GET  /api/constellation/:name/stats`    — per-member load + recent decisions
//!
//! Behind the same Bearer auth + per-IP rate limit as the rest of `/api`.

use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_orchestrator::{Constellation, RoutingError};
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) async fn register_constellation_handler(
    State(s): State<AppState>,
    Json(c): Json<Constellation>,
) -> impl IntoResponse {
    let replaced = s.constellations.register(c.clone()).await;
    let status = if replaced { StatusCode::OK } else { StatusCode::CREATED };
    (
        status,
        Json(json!({ "name": c.name, "replaced": replaced })),
    )
        .into_response()
}

pub(super) async fn list_constellations_handler(State(s): State<AppState>) -> Json<Value> {
    let mut list = s.constellations.list().await;
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Json(json!({ "constellations": list }))
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct DispatchBody {
    /// Caller-supplied extra required tags. Intersected with the
    /// strategy's own filter (for `TagMatch`) or used as a hard filter
    /// for the other strategies.
    #[serde(default)]
    pub required_tags: Vec<String>,
}

pub(super) async fn dispatch_constellation_handler(
    Path(name): Path<String>,
    State(s): State<AppState>,
    body: Option<Json<DispatchBody>>,
) -> impl IntoResponse {
    let req = body.map(|Json(b)| b).unwrap_or_default();
    match s
        .constellations
        .dispatch(&name, &req.required_tags)
        .await
    {
        Ok(decision) => (StatusCode::OK, Json(json!(decision))).into_response(),
        Err(e) => routing_error_response(e),
    }
}

pub(super) async fn constellation_stats_handler(
    Path(name): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.constellations.stats(&name).await {
        Some(stats) => (StatusCode::OK, Json(json!(stats))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("unknown constellation: {name}") })),
        )
            .into_response(),
    }
}

fn routing_error_response(e: RoutingError) -> axum::response::Response {
    use RoutingError::{Empty, NoEligibleMember, UnknownConstellation};
    let (status, msg) = match &e {
        UnknownConstellation(_) | Empty(_) => (StatusCode::NOT_FOUND, format!("{e}")),
        NoEligibleMember => (StatusCode::CONFLICT, format!("{e}")),
    };
    (status, Json(json!({ "error": msg }))).into_response()
}
