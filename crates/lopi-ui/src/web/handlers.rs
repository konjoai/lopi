//! Route handlers for the lopi web API.
//!
//! Separated from `web/mod.rs` to keep that file within the 500-line budget.
//! All functions are imported into `mod.rs` via `use handlers::*`.

use super::types::{CreateTaskRequest, CreateTaskResponse, MAX_GOAL_LENGTH};
use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_core::{CustomerTier, Priority, ReportChannel, ReportChannelError, Task, TaskId};
use lopi_memory::CheckpointInput;
use lopi_spec::SpecSurface;
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "service": "lopi" }))
}

pub(super) async fn get_stats(State(s): State<AppState>) -> impl IntoResponse {
    let stats = s.pool.stats();
    let (total_tokens_today, total_cost_usd_today) =
        s.store.daily_token_totals().await.unwrap_or_else(|e| {
            tracing::warn!("daily_token_totals query failed: {e}");
            (0, 0.0)
        });
    Json(json!({
        "running": stats.running, "queued": stats.queued,
        "succeeded": stats.succeeded, "failed": stats.failed,
        "uptime_secs": stats.uptime_secs,
        "total_tokens_today": total_tokens_today,
        "total_cost_usd_today": total_cost_usd_today,
    }))
}

pub(super) async fn list_tasks(State(s): State<AppState>) -> Json<Value> {
    let rows = s.store.load_history(100).await.unwrap_or_default();
    let body: Vec<_> = rows
        .into_iter()
        .map(|t| {
            json!({
                "id": t.id, "goal": t.goal, "status": t.status,
                "created_at": t.created_at, "completed_at": t.completed_at,
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
    match rows.into_iter().find(|t| t.id.starts_with(&id)) {
        Some(t) => (
            StatusCode::OK,
            Json(json!({
                "id": t.id, "goal": t.goal, "status": t.status,
                "created_at": t.created_at, "completed_at": t.completed_at,
            })),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "task not found" })),
        )
            .into_response(),
    }
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
    let rows = s.store.load_history(500).await.unwrap_or_default();
    let Some(t) = rows.into_iter().find(|t| t.id.starts_with(&id)) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        )
            .into_response();
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
    decide_plan(&s, &id, lopi_core::PlanDecision::Approve).await
}

/// Phase 11 — reject a paused plan; the agent abandons the task.
pub(super) async fn reject_plan(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    decide_plan(&s, &id, lopi_core::PlanDecision::Reject).await
}

/// Deliver a plan decision to a paused runner, resolving `id` (full UUID or a
/// prefix) to a task and signalling the pool.
async fn decide_plan(
    s: &AppState,
    id: &str,
    decision: lopi_core::PlanDecision,
) -> axum::response::Response {
    let Some(task_id) = resolve_task_id(s, id).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        )
            .into_response();
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
async fn resolve_task_id(s: &AppState, id: &str) -> Option<TaskId> {
    if let Ok(uuid) = id.parse::<uuid::Uuid>() {
        return Some(TaskId(uuid));
    }
    let rows = s.store.load_history(500).await.unwrap_or_default();
    let t = rows.into_iter().find(|t| t.id.starts_with(id))?;
    t.id.parse::<uuid::Uuid>().ok().map(TaskId)
}

/// Apply the loop/verifier/report/override fields exposed on
/// [`CreateTaskRequest`] onto a freshly constructed `Task`. Kept separate
/// from [`create_task`] so the field-mapping contract is unit-testable
/// without an HTTP round-trip.
///
/// # Errors
/// Returns [`ReportChannelError`] when `req.report` names an unknown or
/// currently-unreachable channel (e.g. `"whatsapp"`) — reuses
/// [`ReportChannel::parse`], the same validator `Task`/`ScheduleEntry`
/// already use, rather than a second report-channel parser.
pub(super) fn apply_loop_fields(
    task: &mut Task,
    req: &CreateTaskRequest,
) -> Result<(), ReportChannelError> {
    if let Some(report) = &req.report {
        ReportChannel::parse(report)?;
        task.report = Some(report.clone());
    }
    if let Some(v) = req.verifier_required {
        task.verifier_required = v;
    }
    if let Some(m) = &req.verifier_model {
        task.verifier_model = Some(m.clone());
    }
    if let Some(e) = &req.verifier_effort {
        task.verifier_effort = Some(e.clone());
    }
    if let Some(n) = req.max_iterations {
        task.max_iterations = Some(n);
    }
    if let Some(m) = &req.model {
        task.model = Some(m.clone());
    }
    if let Some(e) = &req.effort {
        task.effort = Some(e.clone());
    }
    Ok(())
}

pub(super) async fn create_task(
    State(s): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    if req.goal.len() > MAX_GOAL_LENGTH {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": format!("goal too long (max {MAX_GOAL_LENGTH} chars)")})),
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
    if let Some(caps) = req.required_capabilities {
        task.required_capabilities = caps;
    }
    task.require_plan_approval = req.require_plan_approval.unwrap_or(false);

    // P2 — refuse pre-submit if no registered agent can satisfy the
    // task's required capabilities. Returns 422 with the offending list
    // so the caller can surface a meaningful error.
    if !s.pool.can_satisfy(&task) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": "no registered agent advertises every required capability",
                "required_capabilities": task.required_capabilities,
                "registered_agent_count": s.pool.capabilities_snapshot().len(),
            })),
        )
            .into_response();
    }

    let task_id = task.id.0.to_string();
    let duplicate_of = s.pool.submit(task).await.map(|id| id.0.to_string());
    let resp = CreateTaskResponse {
        id: task_id,
        goal: req.goal,
        queued: duplicate_of.is_none(),
        duplicate_of,
    };
    (StatusCode::CREATED, Json(resp)).into_response()
}

pub(super) async fn list_patterns(State(s): State<AppState>) -> Json<Value> {
    {
        let cache = s.patterns_cache.lock().await;
        if let Some(cached) = cache.get() {
            return Json(cached.clone());
        }
    }
    let rows = s.store.load_patterns(50).await.unwrap_or_default();
    let body: Vec<_> = rows
        .into_iter()
        .map(|p| {
            json!({
                "id": p.id, "goal_keywords": p.goal_keywords,
                "avg_attempts": p.avg_attempts, "success_rate": p.success_rate,
                "last_seen": p.last_seen,
            })
        })
        .collect();
    let value = json!({ "patterns": body });
    s.patterns_cache.lock().await.set(value.clone());
    Json(value)
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
/// `depends_on` list. An unknown task yields an empty graph (200), not 404 —
/// a task may simply have no recorded DAG yet.
pub(super) async fn get_agent_dag(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
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

/// `GET /api/routing/q-values` — the Q-learning router's learned value table.
pub(super) async fn get_q_values(State(s): State<AppState>) -> impl IntoResponse {
    match s.store.load_q_table().await {
        Ok(rows) => Json(json!({
            "values": rows.iter().map(|r| json!({
                "state": r.state,
                "action": r.action,
                "q": r.q,
                "update_count": r.update_count,
                "updated_at": r.updated_at,
            })).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => {
            tracing::warn!("q-values query failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
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
