use anyhow::Result;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use dashmap::DashMap;
use lopi_core::{AgentEvent, EventBus, Priority, Task, TaskId};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use lopi_ratelimit::TokenBucket;
use lopi_spec::SpecSurface;
use rust_embed::Embed;

/// SvelteKit Forge static build embedded into the lopi binary.
///
/// `web/dist/` is created (empty if needed) by the lopi-ui build script so
/// `cargo build` succeeds even before `npm run build`. When the directory
/// is empty at compile time, the runtime handler serves the placeholder
/// page from `placeholder.html` instead.
#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../web/dist"]
struct WebAssets;

const PLACEHOLDER_HTML: &str = include_str!("../placeholder.html");

use serde_json::{json, Value};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};
use tower_http::cors::CorsLayer;

/// Simple TTL cache — returns the stored value if it was set within `ttl`.
struct TtlCache {
    data: Option<(Instant, Value)>,
    ttl: Duration,
}

impl TtlCache {
    fn new(ttl: Duration) -> Self {
        Self { data: None, ttl }
    }

    fn get(&self) -> Option<&Value> {
        self.data
            .as_ref()
            .filter(|(t, _)| t.elapsed() < self.ttl)
            .map(|(_, v)| v)
    }

    fn set(&mut self, data: Value) {
        self.data = Some((Instant::now(), data));
    }
}

/// Shared application state injected into every axum handler.
#[derive(Clone)]
pub struct AppState {
    /// Persistent `SQLite` memory store for tasks and patterns.
    pub store: MemoryStore,
    /// Event bus for broadcasting `AgentEvent`s to connected clients.
    pub bus: EventBus<AgentEvent>,
    /// Priority task queue shared with the orchestrator pool.
    pub queue: TaskQueue,
    /// Handle to the running agent pool for status queries and cancellation.
    pub pool: Arc<AgentPool>,
    /// Repo root path — used to extract the spec surface on demand.
    pub repo_path: std::path::PathBuf,
    /// Pre-serialized broadcast: each `AgentEvent` serialized once, shared across all WS/SSE subscribers.
    serialized_tx: Arc<broadcast::Sender<Arc<str>>>,
    patterns_cache: Arc<Mutex<TtlCache>>,
    /// Bearer token required on /api/* routes. None = auth disabled (dev mode).
    auth_token: Option<Arc<str>>,
    /// Per-IP token-bucket rate limiter for API endpoints.
    rate_limiter: Arc<DashMap<IpAddr, TokenBucket>>,
}

impl AppState {
    /// Construct a new `AppState`, wiring together the store, event bus, queue, pool, and optional auth token.
    #[must_use]
    pub fn new(
        store: MemoryStore,
        bus: EventBus<AgentEvent>,
        queue: TaskQueue,
        pool: Arc<AgentPool>,
        auth_token: Option<String>,
    ) -> Self {
        Self::new_with_repo(store, bus, queue, pool, auth_token, std::path::PathBuf::from("."))
    }

    /// Variant that also records the repo path for spec surface serving.
    #[must_use]
    pub fn new_with_repo(
        store: MemoryStore,
        bus: EventBus<AgentEvent>,
        queue: TaskQueue,
        pool: Arc<AgentPool>,
        auth_token: Option<String>,
        repo_path: std::path::PathBuf,
    ) -> Self {
        let (serialized_tx, _) = broadcast::channel::<Arc<str>>(512);
        let serialized_tx = Arc::new(serialized_tx);

        // Bridge: subscribe to raw AgentEvent bus, serialize once, re-broadcast as Arc<str>.
        {
            let mut rx = bus.subscribe();
            let tx = serialized_tx.clone();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            if let Ok(json) = serde_json::to_string(&ev) {
                                let _ = tx.send(Arc::from(json.as_str()));
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("serializer bridge lagged {n} events");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        Self {
            store,
            bus,
            queue,
            pool,
            repo_path,
            serialized_tx,
            patterns_cache: Arc::new(Mutex::new(TtlCache::new(Duration::from_secs(30)))),
            auth_token: auth_token.map(|t| Arc::from(t.as_str())),
            rate_limiter: Arc::new(DashMap::new()),
        }
    }
}

/// Build the axum router with all routes wired to `state`.
pub fn build_app(state: AppState) -> Router {
    // /api/* routes — protected by Bearer auth and per-IP rate limiting.
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/:id", get(get_task).delete(cancel_task))
        .route("/api/stats", get(get_stats))
        .route("/api/patterns", get(list_patterns))
        .route("/api/spec", get(get_spec))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .merge(api)
        .route("/metrics", get(metrics))
        .route("/sse", get(sse_handler))
        .route("/ws", get(ws_handler))
        // Legacy endpoint — kept for compat.
        .route("/ws/tasks", get(ws_handler))
        // Static handler catches /, /constellation, /favicon.svg,
        // /_app/**/*, and any SPA route fallthrough. Explicit routes above
        // take precedence.
        .fallback(get(static_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// # Errors
///
/// Returns an error if the TCP listener cannot bind to the address.
pub async fn serve(
    store: MemoryStore,
    bus: EventBus<AgentEvent>,
    queue: TaskQueue,
    pool: Arc<AgentPool>,
    host: &str,
    port: u16,
    auth_token: Option<String>,
) -> Result<()> {
    serve_with_repo(store, bus, queue, pool, host, port, auth_token, std::path::PathBuf::from(".")).await
}

/// Variant that also wires the repo path for `/api/spec` serving.
#[allow(clippy::too_many_arguments)]
pub async fn serve_with_repo(
    store: MemoryStore,
    bus: EventBus<AgentEvent>,
    queue: TaskQueue,
    pool: Arc<AgentPool>,
    host: &str,
    port: u16,
    auth_token: Option<String>,
    repo_path: std::path::PathBuf,
) -> Result<()> {
    let state = AppState::new_with_repo(store, bus, queue, pool, auth_token, repo_path);
    let app = build_app(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!(
        "🌐 lopi sail: http://{addr}  ws://{addr}/ws  sse://{addr}/sse  metrics://{addr}/metrics"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // Use connect_info so rate limiter middleware can extract client IPs.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// Middleware: validate `Authorization: Bearer <token>` on all /api/* routes.
/// Skipped entirely when `auth_token` is not configured (dev mode).
async fn auth_middleware(
    State(s): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if let Some(expected) = &s.auth_token {
        let provided = request
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        if !provided.is_some_and(|p| constant_time_eq(p, expected.as_ref())) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
                .into_response();
        }
    }
    next.run(request).await
}

/// Middleware: per-IP token-bucket rate limiter (60 req/min burst, 1 req/sec refill).
/// Falls back to `127.0.0.1` when `ConnectInfo` is unavailable (e.g., in tests).
async fn rate_limit_middleware(
    State(s): State<AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    use axum::extract::connect_info::ConnectInfo;

    let ip: IpAddr = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0.ip())
        })
        .unwrap_or_else(|| IpAddr::from([127, 0, 0, 1]));

    // Get or create a per-IP bucket: 60-token burst, 1 token/sec refill.
    let bucket = s.rate_limiter.get(&ip).map_or_else(
        || {
            let new_bucket = TokenBucket::new(60.0, 1.0);
            s.rate_limiter.insert(ip, new_bucket.clone());
            new_bucket
        },
        |b| b.clone(),
    );

    if !bucket.try_acquire(1.0).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "rate limit exceeded"})),
        )
            .into_response();
    }

    next.run(req).await
}

/// Constant-time string comparison to prevent timing-based side-channel attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Static asset handler — serves the embedded SvelteKit Forge build.
///
/// Lookup order:
///   1. Direct file match (e.g. `/_app/immutable/chunks/x.js`, `/favicon.svg`)
///   2. Append `.html` for prerendered routes (e.g. `/constellation` →
///      `constellation.html`)
///   3. Fall back to `index.html` (SPA client-side routing for unknown paths)
///   4. Fall back to the bundled placeholder if `web/dist/` is empty
///      (i.e. `npm run build` hasn't been run yet).
async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/').to_string();

    // 1. Direct file
    if !path.is_empty() {
        if let Some(file) = WebAssets::get(&path) {
            return file_response(file, &path);
        }
    }

    // 2. .html fallback for prerendered routes
    if !path.is_empty() && !path.contains('.') {
        let html_path = format!("{}.html", path);
        if let Some(file) = WebAssets::get(&html_path) {
            return file_response(file, &html_path);
        }
    }

    // 3. SPA fallback — index.html handles client-side routing
    if let Some(file) = WebAssets::get("index.html") {
        return file_response(file, "index.html");
    }

    // 4. No SvelteKit build present — show placeholder with build instructions.
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(PLACEHOLDER_HTML))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Serve an embedded file with appropriate Content-Type and Cache-Control.
///
/// Cache strategy:
///   - SvelteKit's `_app/immutable/*` chunks are content-hashed → cache forever
///   - Everything else (HTML, top-level assets) → 5-minute browser cache
fn file_response(file: rust_embed::EmbeddedFile, path: &str) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache_control = if path.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=300"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from(file.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "service": "lopi" }))
}

async fn get_stats(State(s): State<AppState>) -> impl IntoResponse {
    let stats = s.pool.stats();
    let (total_tokens_today, total_cost_usd_today) =
        s.store.daily_token_totals().await.unwrap_or_else(|e| {
            tracing::warn!("daily_token_totals query failed: {e}");
            (0, 0.0)
        });
    Json(json!({
        "running": stats.running,
        "queued": stats.queued,
        "succeeded": stats.succeeded,
        "failed": stats.failed,
        "uptime_secs": stats.uptime_secs,
        "total_tokens_today": total_tokens_today,
        "total_cost_usd_today": total_cost_usd_today,
    }))
}

async fn list_tasks(State(s): State<AppState>) -> Json<Value> {
    let rows = s.store.load_history(100).await.unwrap_or_default();
    let body: Vec<_> = rows
        .into_iter()
        .map(|t| {
            json!({
                "id": t.id,
                "goal": t.goal,
                "status": t.status,
                "created_at": t.created_at,
                "completed_at": t.completed_at,
            })
        })
        .collect();
    Json(json!({ "tasks": body }))
}

async fn get_task(Path(id): Path<String>, State(s): State<AppState>) -> impl IntoResponse {
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

async fn cancel_task(Path(id): Path<String>, State(s): State<AppState>) -> impl IntoResponse {
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
    let cancelled = s.pool.cancel(&task_id).await;
    if cancelled {
        (
            StatusCode::OK,
            Json(json!({ "cancelled": true, "id": t.id })),
        )
            .into_response()
    } else {
        (
            StatusCode::OK,
            Json(json!({ "cancelled": false, "reason": "task not running or already complete" })),
        )
            .into_response()
    }
}

use types::{CreateTaskRequest, CreateTaskResponse, MAX_GOAL_LENGTH};

mod types;

async fn create_task(
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

async fn list_patterns(State(s): State<AppState>) -> Json<Value> {
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
///
/// Loads `.lopi/spec_surface.json` if it exists; falls back to live
/// extraction from the repo. Returns 200 with the surface JSON, or 404
/// if no tests were found.
async fn get_spec(State(s): State<AppState>) -> impl IntoResponse {
    let surface = match SpecSurface::load(&s.repo_path) {
        Ok(Some(cached)) => cached,
        _ => match SpecSurface::extract(&s.repo_path) {
            Ok(live) => live,
            Err(e) => {
                tracing::warn!("spec extract failed: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "spec extraction failed").into_response();
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

/// Prometheus text-format metrics. No external crate required — format is trivial.
async fn metrics(State(s): State<AppState>) -> impl IntoResponse {
    let stats = s.pool.stats();
    let body = format!(
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
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
}

mod streaming;
use streaming::{sse_handler, ws_handler};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
