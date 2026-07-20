//! Observability + read-only reporting route handlers for the lopi web API.
//!
//! Split out of `handlers.rs` to keep that module within the 500-line budget:
//! the quality trend, DAG trace, Q-value table, Prometheus metrics, and pricing
//! endpoints form one cohesive read-only surface. Imported into `mod.rs`
//! alongside the core `handlers` module.

use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_core::CustomerTier;
use serde_json::{json, Value};

/// `GET /api/quality/trend?repo=<path>&limit=<n>` — quality check run history.
pub(super) async fn get_quality_trend(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    let repo_str = params
        .get("repo")
        .cloned()
        .unwrap_or_else(|| s.repo_path.to_string_lossy().to_string());
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    match s.store.load_quality_trend(&repo_str, limit).await {
        Ok(rows) => Json(json!({
            "repo": repo_str,
            "runs": rows.iter().map(|r| json!({
                "id": r.id,
                "spec_items": r.spec_items,
                "passing": r.passing,
                "failing": r.failing,
                "gaps": r.gaps,
                "score": r.score,
                "run_at": r.run_at,
            })).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => {
            tracing::warn!("quality_trend query failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

/// `GET /api/agents/:id/dag` — the DAG-structured execution trace for a task.
///
/// Returns `{ task_id, nodes, edges }`; edges are derived from each node's
/// `depends_on` list. A *known* task with no recorded DAG yet still yields an
/// empty graph (200); a *bogus* id is a 404 (Verify-1 F8 — previously both
/// returned 200, which the audit flagged as a body/status mismatch).
pub(super) async fn get_agent_dag(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match s.store.task_exists(&id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "unknown task id", "task_id": id})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::warn!("task_exists failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
        }
    }
    match s.store.load_dag_nodes(&id).await {
        Ok(rows) => Json(dag_graph_json(&id, &rows)).into_response(),
        Err(e) => {
            tracing::warn!("agent dag query failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

/// Shape DAG node rows into the `{ task_id, nodes, edges }` JSON graph. Edges
/// are derived from each node's `depends_on` list (`dep → kind`).
fn dag_graph_json(task_id: &str, rows: &[lopi_memory::DagNodeRow]) -> Value {
    let mut edges = Vec::new();
    let nodes: Vec<Value> = rows
        .iter()
        .map(|r| {
            let deps: Vec<String> = serde_json::from_str(&r.depends_on_json).unwrap_or_default();
            for dep in &deps {
                edges.push(json!({ "from": dep, "to": r.kind }));
            }
            json!({
                "kind": r.kind,
                "status": r.status,
                "depends_on": deps,
                "output_hash": r.output_hash,
                "idempotency_key": r.idempotency_key,
                "updated_at": r.updated_at,
            })
        })
        .collect();
    json!({ "task_id": task_id, "nodes": nodes, "edges": edges })
}

/// Prometheus text-format metrics.
pub(super) async fn metrics(State(s): State<AppState>) -> impl IntoResponse {
    let stats = s.pool.stats();
    let mut body = format!(
        "# HELP lopi_agents_running Currently running agents\n\
         # TYPE lopi_agents_running gauge\n\
         lopi_agents_running {running}\n\
         # HELP lopi_agents_queued Tasks waiting in queue\n\
         # TYPE lopi_agents_queued gauge\n\
         lopi_agents_queued {queued}\n\
         # HELP lopi_tasks_succeeded_total Tasks completed successfully\n\
         # TYPE lopi_tasks_succeeded_total counter\n\
         lopi_tasks_succeeded_total {succeeded}\n\
         # HELP lopi_tasks_failed_total Tasks that failed after all retries\n\
         # TYPE lopi_tasks_failed_total counter\n\
         lopi_tasks_failed_total {failed}\n\
         # HELP lopi_uptime_seconds Seconds since lopi sail started\n\
         # TYPE lopi_uptime_seconds counter\n\
         lopi_uptime_seconds {uptime}\n",
        running = stats.running,
        queued = stats.queued,
        succeeded = stats.succeeded,
        failed = stats.failed,
        uptime = stats.uptime_secs,
    );

    // P1.4 — Schema-validation violations counter, label-keyed by failure
    // kind (type / required / enum / property). One HELP/TYPE preamble
    // followed by one line per label that has fired at least once.
    let violations = lopi_core::schema_violations_snapshot();
    if !violations.is_empty() {
        body.push_str("# HELP lopi_schema_violations_total Output schema validation failures\n");
        body.push_str("# TYPE lopi_schema_violations_total counter\n");
        for (kind, count) in violations {
            // `kind` is from a closed enum (Type/Required/EnumMismatch/Property),
            // so no escaping is necessary, but we still wrap it defensively.
            body.push_str(&format!(
                "lopi_schema_violations_total{{kind=\"{kind}\"}} {count}\n"
            ));
        }
    }

    match s.store.count_audit().await {
        Ok(audit_total) => {
            body.push_str("# HELP lopi_audit_log_total Rows recorded in the audit log\n");
            body.push_str("# TYPE lopi_audit_log_total counter\n");
            body.push_str(&format!("lopi_audit_log_total {audit_total}\n"));
        }
        Err(e) => tracing::warn!("count_audit failed: {e}"),
    }

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
}

/// `GET /api/plans` — return available subscription tiers.
///
/// Returns a static list of all `CustomerTier` variants with their pricing
/// and feature sets. Clients (the onboarding page, the Forge header) use
/// this to render pricing tables without hardcoding values in the frontend.
pub(super) async fn get_plans() -> Json<Value> {
    let plans: Vec<Value> = [
        CustomerTier::Free,
        CustomerTier::Starter,
        CustomerTier::Growth,
        CustomerTier::Enterprise,
    ]
    .iter()
    .map(|&tier| {
        json!({
            "id": tier.as_str(),
            "name": tier.display_name(),
            "price_usd_per_month": tier.price_usd_cents_per_month() / 100,
            "max_agents": tier.max_agents(),
            "features": tier.features(),
        })
    })
    .collect();
    Json(json!({ "plans": plans }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::dag_graph_json;
    use lopi_memory::DagNodeRow;

    fn row(kind: &str, depends_on_json: &str) -> DagNodeRow {
        DagNodeRow {
            task_id: "t".into(),
            kind: kind.into(),
            status: "pending".into(),
            depends_on_json: depends_on_json.into(),
            output_hash: None,
            idempotency_key: None,
            updated_at: "now".into(),
        }
    }

    #[test]
    fn dag_graph_derives_edges_from_depends_on() {
        let rows = vec![row("plan", "[]"), row("implement", "[\"plan\"]")];
        let g = dag_graph_json("t1", &rows);
        assert_eq!(g["task_id"], "t1");
        assert_eq!(g["nodes"].as_array().unwrap().len(), 2);
        let edges = g["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["from"], "plan");
        assert_eq!(edges[0]["to"], "implement");
    }

    #[test]
    fn dag_graph_empty_for_no_rows() {
        let g = dag_graph_json("t1", &[]);
        assert!(g["nodes"].as_array().unwrap().is_empty());
        assert!(g["edges"].as_array().unwrap().is_empty());
    }
}
