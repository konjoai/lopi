//! P2 — REST handlers for the tool registry.
//!
//! Routes:
//! - `GET    /api/tools`        — list every registered tool
//! - `POST   /api/tools`        — register/upsert a tool
//! - `GET    /api/tools/:name`  — fetch one by name
//! - `DELETE /api/tools/:name`  — remove
//!
//! All endpoints sit behind the existing Bearer auth + per-IP rate limit
//! middleware in `mod.rs`.

use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_tools::{RegistryError, ToolSpec};
use serde_json::{json, Value};

pub(super) async fn list_tools_handler(State(s): State<AppState>) -> Json<Value> {
    let mut tools = s.tools.list().await;
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    Json(json!({ "tools": tools }))
}

pub(super) async fn register_tool_handler(
    State(s): State<AppState>,
    Json(spec): Json<ToolSpec>,
) -> impl IntoResponse {
    match s.tools.register(spec.clone()).await {
        Ok(()) => (
            StatusCode::CREATED,
            Json(json!({ "registered": spec.name })),
        )
            .into_response(),
        Err(e) => registry_error_response(e),
    }
}

pub(super) async fn get_tool_handler(
    Path(name): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.tools.get(&name).await {
        Some(spec) => (StatusCode::OK, Json(json!(spec))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("unknown tool: {name}") })),
        )
            .into_response(),
    }
}

pub(super) async fn delete_tool_handler(
    Path(name): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    if s.tools.deregister(&name).await {
        (StatusCode::OK, Json(json!({ "deregistered": name }))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("unknown tool: {name}") })),
        )
            .into_response()
    }
}

fn registry_error_response(e: RegistryError) -> axum::response::Response {
    use RegistryError::{InvalidName, InvalidParameters, Io, Serde};
    let (status, msg) = match &e {
        InvalidName(_) | InvalidParameters(_) => (StatusCode::UNPROCESSABLE_ENTITY, format!("{e}")),
        Io(_) | Serde(_) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")),
    };
    (status, Json(json!({ "error": msg }))).into_response()
}
