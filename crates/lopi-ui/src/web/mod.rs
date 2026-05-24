use anyhow::Result;
use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use dashmap::DashMap;
use lopi_core::{AgentEvent, EventBus};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use lopi_ratelimit::TokenBucket;
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
    /// P2 — Durable tool registry. `clone()` is `Arc<RwLock>` under the hood.
    pub tools: lopi_tools::ToolRegistry,
    /// P2 — Constellation router. Cheap to `clone()` — wraps an `Arc<RwLock>`.
    pub constellations: lopi_orchestrator::ConstellationRouter,
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
        Self::new_with_repo(
            store,
            bus,
            queue,
            pool,
            auth_token,
            std::path::PathBuf::from("."),
        )
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

        // Tool registry — empty in-memory store wired to the default path.
        // Callers that want the on-disk registry pre-loaded should call
        // `state.hydrate_tools()` after construction (e.g. inside `serve`).
        let tools = lopi_tools::ToolRegistry::new(lopi_tools::default_registry_path());
        // Constellation router — in-memory; registrations re-created on
        // every `lopi sail` start (intentional — they describe topology,
        // not durable agent state).
        let constellations = lopi_orchestrator::ConstellationRouter::new();

        Self {
            store,
            bus,
            queue,
            pool,
            repo_path,
            tools,
            constellations,
            serialized_tx,
            patterns_cache: Arc::new(Mutex::new(TtlCache::new(Duration::from_secs(30)))),
            auth_token: auth_token.map(|t| Arc::from(t.as_str())),
            rate_limiter: Arc::new(DashMap::new()),
        }
    }

    /// Hydrate the tool registry from its on-disk path. Call this from an
    /// async context (e.g. inside `serve`) before binding the listener so
    /// the first `/tools` request sees previously registered tools.
    ///
    /// # Errors
    /// Returns `Err` only if the registry file exists but is unreadable or
    /// malformed JSON. A missing file is treated as "empty registry".
    pub async fn hydrate_tools(&mut self) -> Result<()> {
        let path = self.tools.path().to_path_buf();
        self.tools = lopi_tools::ToolRegistry::load(&path).await?;
        Ok(())
    }
}

/// Build the axum router with all routes wired to `state`.
pub fn build_app(state: AppState) -> Router {
    // /api/* routes — protected by Bearer auth and per-IP rate limiting.
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/:id", get(get_task).delete(cancel_task))
        .route(
            "/api/agents/:id/checkpoint",
            axum::routing::post(checkpoint_agent),
        )
        .route("/api/stats", get(get_stats))
        .route("/api/patterns", get(list_patterns))
        .route("/api/plans", get(get_plans))
        .route("/api/spec", get(get_spec))
        .route("/api/quality/trend", get(get_quality_trend))
        .route(
            "/api/tools",
            get(list_tools_handler).post(register_tool_handler),
        )
        .route(
            "/api/tools/:name",
            get(get_tool_handler).delete(delete_tool_handler),
        )
        .route("/api/cache/stats", get(cache_stats_handler))
        .route("/api/cache", axum::routing::delete(clear_cache_handler))
        .route(
            "/api/cache/agent/:agent",
            axum::routing::delete(invalidate_agent_cache_handler),
        )
        .route(
            "/api/constellations",
            get(list_constellations_handler).post(register_constellation_handler),
        )
        .route(
            "/api/constellation/:name/dispatch",
            axum::routing::post(dispatch_constellation_handler),
        )
        .route(
            "/api/constellation/:name/stats",
            get(constellation_stats_handler),
        )
        .route("/api/tasks/dead-letter", get(dlq_handlers::list_dlq))
        .route(
            "/api/tasks/dead-letter/:id",
            get(dlq_handlers::get_dlq).delete(dlq_handlers::delete_dlq),
        )
        .route(
            "/api/tasks/dead-letter/:id/retry",
            axum::routing::post(dlq_handlers::retry_dlq),
        )
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
    serve_with_repo(
        store,
        bus,
        queue,
        pool,
        host,
        port,
        auth_token,
        std::path::PathBuf::from("."),
    )
    .await
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
    let mut state = AppState::new_with_repo(store, bus, queue, pool, auth_token, repo_path);
    if let Err(e) = state.hydrate_tools().await {
        // Hydrate failed — keep the empty in-memory registry. Runtime
        // /tools registrations still persist; we just lose previously
        // saved entries until someone re-registers them.
        tracing::warn!(error = %e, "tool registry hydrate failed; starting empty");
    }
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

mod cache_handlers;
mod constellation_handlers;
mod dlq_handlers;
mod handlers;
mod tools_handlers;
use cache_handlers::{cache_stats_handler, clear_cache_handler, invalidate_agent_cache_handler};
use constellation_handlers::{
    constellation_stats_handler, dispatch_constellation_handler, list_constellations_handler,
    register_constellation_handler,
};
use handlers::{
    cancel_task, checkpoint_agent, create_task, get_plans, get_quality_trend, get_spec, get_stats,
    get_task, health, list_patterns, list_tasks, metrics,
};
use tools_handlers::{
    delete_tool_handler, get_tool_handler, list_tools_handler, register_tool_handler,
};
mod streaming;
pub(crate) mod types;
use streaming::{sse_handler, ws_handler};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
