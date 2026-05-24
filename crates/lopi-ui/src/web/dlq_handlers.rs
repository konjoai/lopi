//! Dead-letter queue REST surface.
//!
//! The pool pushes any task that exhausts its retry budget to the
//! `dead_letter_queue` table (see `lopi-memory::DeadLetterRow`).
//! Operators inspect those rows, manually re-queue them, or
//! permanently discard them through these handlers.
//!
//! Routes:
//! - `GET    /api/tasks/dead-letter`              — newest-first list (limit ?n=200 default 50)
//! - `GET    /api/tasks/dead-letter/:id`          — fetch a single row
//! - `POST   /api/tasks/dead-letter/:id/retry`    — take + re-submit, returns the new `TaskId`
//! - `DELETE /api/tasks/dead-letter/:id`          — permanently discard
//!
//! All sit behind the same Bearer-auth + rate-limit middleware as the
//! rest of `/api/*`.

use super::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_core::Task;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub(super) struct ListParams {
    /// Page size. Defaults to 50, capped at 500 to bound response size.
    #[serde(default)]
    pub n: Option<i64>,
}

pub(super) async fn list_dlq(
    State(s): State<AppState>,
    Query(params): Query<ListParams>,
) -> impl IntoResponse {
    let limit = params.n.unwrap_or(50).clamp(1, 500);
    match s.store.list_dead_letters(limit).await {
        Ok(rows) => {
            let body: Vec<Value> = rows.into_iter().map(dlq_to_json).collect();
            (StatusCode::OK, Json(json!({ "dead_letters": body }))).into_response()
        }
        Err(e) => {
            tracing::warn!("list_dead_letters failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("{e:#}")})),
            )
                .into_response()
        }
    }
}

pub(super) async fn get_dlq(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.get_dead_letter(&id).await {
        Ok(Some(row)) => (StatusCode::OK, Json(dlq_to_json(row))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "dead-letter row not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("{e:#}")})),
        )
            .into_response(),
    }
}

/// `POST /api/tasks/dead-letter/:id/retry` — atomically take a DLQ row
/// and re-enqueue it as a fresh `Task` (new `TaskId`, same goal + repo).
/// Returns `{retried_from, new_task_id, queued}`.
pub(super) async fn retry_dlq(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    let row = match s.store.take_dead_letter(&id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "dead-letter row not found"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("{e:#}")})),
            )
                .into_response();
        }
    };

    let mut task = Task::new(row.goal.clone());
    if let Some(rp) = &row.repo_path {
        task.repo_path = Some(std::path::PathBuf::from(rp));
    }
    let new_id = task.id.0.to_string();
    let duplicate_of = s.pool.submit(task).await.map(|tid| tid.0.to_string());

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "retried_from": id,
            "original_task_id": row.task_id,
            "new_task_id": new_id,
            "queued": duplicate_of.is_none(),
            "duplicate_of": duplicate_of,
        })),
    )
        .into_response()
}

pub(super) async fn delete_dlq(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.delete_dead_letter(&id).await {
        Ok(true) => (StatusCode::OK, Json(json!({"deleted": id}))).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "dead-letter row not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("{e:#}")})),
        )
            .into_response(),
    }
}

fn dlq_to_json(row: lopi_memory::DeadLetterRow) -> Value {
    json!({
        "id": row.id,
        "task_id": row.task_id,
        "goal": row.goal,
        "repo_path": row.repo_path,
        "total_attempts": row.total_attempts,
        "last_error": row.last_error,
        "first_failed_at": row.first_failed_at,
        "dead_at": row.dead_at,
        "source": row.source,
    })
}
