//! Audit log REST surface.
//!
//! `GET /api/audit?since_id=&action=&subject_type=&subject_id=&n=`
//! cursor-paginates the append-only `audit_log` table. Query params
//! are forwarded straight to `MemoryStore::query_audit`; the response
//! includes a `next_cursor` so tail loops can drive the API without
//! tracking row count themselves.

use super::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_memory::AuditQuery;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub(super) struct AuditParams {
    /// Only return rows with `id > since_id`. Defaults to 0.
    #[serde(default)]
    pub since_id: Option<i64>,
    /// Filter to a single action label.
    #[serde(default)]
    pub action: Option<String>,
    /// Filter to a single subject kind.
    #[serde(default)]
    pub subject_type: Option<String>,
    /// Filter to a single subject id.
    #[serde(default)]
    pub subject_id: Option<String>,
    /// Page size. Defaults to 100; clamped by the store to [1, 1000].
    #[serde(default)]
    pub n: Option<i64>,
}

pub(super) async fn query_audit(
    State(s): State<AppState>,
    Query(p): Query<AuditParams>,
) -> impl IntoResponse {
    let q = AuditQuery {
        since_id: p.since_id.unwrap_or(0),
        action: p.action,
        subject_type: p.subject_type,
        subject_id: p.subject_id,
        limit: p.n.unwrap_or(100),
    };
    match s.store.query_audit(&q).await {
        Ok(rows) => {
            let next_cursor = rows.last().map(|r| r.id).unwrap_or(q.since_id);
            let body: Vec<Value> = rows
                .into_iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "ts": r.ts,
                        "action": r.action,
                        "subject_type": r.subject_type,
                        "subject_id": r.subject_id,
                        "actor": r.actor,
                        "payload": r.payload,
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(json!({
                    "events": body,
                    "next_cursor": next_cursor,
                })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!("audit query failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("{e:#}")})),
            )
                .into_response()
        }
    }
}
