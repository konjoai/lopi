//! Route handlers for the lopi web API.
//!
//! Separated from `web/mod.rs` to keep that file within the 500-line budget.
//! All functions are imported into `mod.rs` via `use handlers::*`.

use super::types::{CreateTaskRequest, CreateTaskResponse, MAX_GOAL_LENGTH};
use super::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_core::{CustomerTier, Priority, Task, TaskId};
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
    task.priority = match req.priority.as_deref() {
        Some("low") => Priority::Low,
        Some("high") => Priority::High,
        Some("critical") => Priority::Critical,
        _ => Priority::Normal,
    };
    // Repo: empty / whitespace / missing → fall back to sail's `--repo`
    // default (canonicalised in `AppState.repo_path`). Prevents the
    // orchestrator from trying to open a git repo at `""`.
    let req_repo = req.repo.as_deref().map(str::trim).filter(|r| !r.is_empty());
    task.repo_path = Some(
        req_repo
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| s.repo_path.clone()),
    );
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
    task.base_branch = req
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(str::to_owned);
    task.model = req
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty() && *m != "auto")
        .map(str::to_owned);
    let effort = req
        .effort
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_ascii_lowercase);
    if let Some(ref e) = effort {
        // Only nudge max_retries when the request didn't pin it explicitly.
        if req.max_retries.is_none() {
            task.max_retries = match e.as_str() {
                "low" => 1,
                "medium" => 3,
                "high" => 5,
                "max" => 8,
                _ => task.max_retries,
            };
        }
    }
    task.effort = effort;

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

/// `GET /api/repos` — list git repositories the user can target.
///
/// Scans the sail working repo and `$HOME` to a bounded depth, returning
/// every directory that contains a `.git/` child. Used by the pane "repo"
/// dropdown so the user can switch targets without typing a path.
pub(super) async fn list_repos(State(s): State<AppState>) -> Json<Value> {
    let roots = scan_roots(&s.repo_path);
    let repos = tokio::task::spawn_blocking(move || scan_git_repos(&roots))
        .await
        .unwrap_or_default();
    let body: Vec<_> = repos
        .into_iter()
        .map(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string());
            json!({ "path": p.display().to_string(), "name": name })
        })
        .collect();
    Json(json!({ "repos": body }))
}

/// `GET /api/repos/branches?path=<repo>` — list local branches in a repo.
///
/// Used by the pane "base branch" dropdown so the user can pick from the
/// repo's actual branches once they've selected a repo.
pub(super) async fn list_branches(
    State(s): State<AppState>,
    Query(q): Query<RepoQuery>,
) -> Json<Value> {
    let raw = q.path.unwrap_or_default();
    let trimmed = raw.trim();
    let repo_path = if trimmed.is_empty() {
        s.repo_path.clone()
    } else {
        std::path::PathBuf::from(trimmed)
    };
    let branches = tokio::task::spawn_blocking(move || read_local_branches(&repo_path))
        .await
        .unwrap_or_default();
    Json(json!({ "branches": branches }))
}

#[derive(Debug, Deserialize)]
pub(super) struct RepoQuery {
    pub path: Option<String>,
}

fn read_local_branches(repo_path: &std::path::Path) -> Vec<String> {
    let Ok(repo) = git2::Repository::open(repo_path) else {
        return Vec::new();
    };
    let head_name = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(str::to_owned));
    let Ok(branches) = repo.branches(Some(git2::BranchType::Local)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = branches
        .filter_map(std::result::Result::ok)
        .filter_map(|(branch, _)| branch.name().ok().flatten().map(str::to_owned))
        .collect();
    names.sort();
    if let Some(h) = head_name {
        if let Some(idx) = names.iter().position(|n| n == &h) {
            let v = names.remove(idx);
            names.insert(0, v);
        }
    }
    names
}

fn scan_roots(sail_repo: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(abs) = std::fs::canonicalize(sail_repo) {
        roots.push(abs);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        if !roots.iter().any(|r| r == &home) {
            roots.push(home);
        }
    }
    roots
}

fn scan_git_repos(roots: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    const MAX_DEPTH: u8 = 4;
    const SKIP_DIRS: &[&str] = &[
        "node_modules",
        "target",
        ".cargo-target",
        "dist",
        "build",
        ".svelte-kit",
        "Library",
        ".Trash",
        ".cache",
        ".npm",
        ".cargo",
        ".rustup",
    ];
    let mut found: Vec<std::path::PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    for root in roots {
        walk(root, 0, MAX_DEPTH, SKIP_DIRS, &mut found, &mut seen);
    }
    found.sort();
    found
}

fn walk(
    dir: &std::path::Path,
    depth: u8,
    max_depth: u8,
    skip: &[&str],
    out: &mut Vec<std::path::PathBuf>,
    seen: &mut std::collections::HashSet<std::path::PathBuf>,
) {
    if depth > max_depth || !seen.insert(dir.to_path_buf()) {
        return;
    }
    if dir.join(".git").exists() {
        out.push(dir.to_path_buf());
        return; // Don't descend into a repo — nested clones aren't useful here.
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if name_s.starts_with('.') || skip.iter().any(|s| *s == name_s) {
            continue;
        }
        walk(&entry.path(), depth + 1, max_depth, skip, out, seen);
    }
}
