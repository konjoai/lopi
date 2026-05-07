use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, IntoResponse, Json, Response},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use dashmap::DashMap;
use futures::StreamExt as _;
use lopi_core::{AgentEvent, EventBus, Priority, Task, TaskId};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use lopi_ratelimit::TokenBucket;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::CorsLayer;

/// Simple TTL cache — returns the stored value if it was set within `ttl`.
struct TtlCache {
    data: Option<(Instant, Value)>,
    ttl: Duration,
}

impl TtlCache {
    fn new(ttl: Duration) -> Self { Self { data: None, ttl } }

    fn get(&self) -> Option<&Value> {
        self.data.as_ref()
            .filter(|(t, _)| t.elapsed() < self.ttl)
            .map(|(_, v)| v)
    }

    fn set(&mut self, data: Value) { self.data = Some((Instant::now(), data)); }

    fn invalidate(&mut self) { self.data = None; }
}

#[derive(Clone)]
pub struct AppState {
    pub store: MemoryStore,
    pub bus: EventBus<AgentEvent>,
    pub queue: TaskQueue,
    pub pool: Arc<AgentPool>,
    /// Pre-serialized broadcast: each AgentEvent serialized once, shared across all WS/SSE subscribers.
    serialized_tx: Arc<broadcast::Sender<Arc<str>>>,
    tasks_cache: Arc<Mutex<TtlCache>>,
    patterns_cache: Arc<Mutex<TtlCache>>,
    /// Bearer token required on /api/* routes. None = auth disabled (dev mode).
    auth_token: Option<Arc<str>>,
    /// Per-IP token-bucket rate limiter for API endpoints.
    rate_limiter: Arc<DashMap<IpAddr, TokenBucket>>,
}

impl AppState {
    pub fn new(
        store: MemoryStore,
        bus: EventBus<AgentEvent>,
        queue: TaskQueue,
        pool: Arc<AgentPool>,
        auth_token: Option<String>,
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
            serialized_tx,
            tasks_cache: Arc::new(Mutex::new(TtlCache::new(Duration::from_secs(2)))),
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
        .route_layer(middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .route("/", get(index))
        .merge(api)
        .route("/metrics", get(metrics))
        .route("/sse", get(sse_handler))
        .route("/ws", get(ws_handler))
        // Legacy endpoint — kept for compat.
        .route("/ws/tasks", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(
    store: MemoryStore,
    bus: EventBus<AgentEvent>,
    queue: TaskQueue,
    pool: Arc<AgentPool>,
    host: &str,
    port: u16,
    auth_token: Option<String>,
) -> Result<()> {
    let state = AppState::new(store, bus, queue, pool, auth_token);
    let app = build_app(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!("🌐 lopi sail: http://{addr}  ws://{addr}/ws  sse://{addr}/sse  metrics://{addr}/metrics");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // Use connect_info so rate limiter middleware can extract client IPs.
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
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

        if !provided.map(|p| constant_time_eq(p, expected.as_ref())).unwrap_or(false) {
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
/// Falls back to 127.0.0.1 when ConnectInfo is unavailable (e.g., in tests).
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
    let bucket = s.rate_limiter.get(&ip).map(|b| b.clone()).unwrap_or_else(|| {
        let new_bucket = TokenBucket::new(60.0, 1.0);
        s.rate_limiter.insert(ip, new_bucket.clone());
        new_bucket
    });

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
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn index() -> impl IntoResponse {
    Html(include_str!("index.html"))
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "service": "lopi" }))
}

async fn get_stats(State(s): State<AppState>) -> impl IntoResponse {
    let stats = s.pool.stats();
    Json(json!({
        "running": stats.running,
        "queued": stats.queued,
        "succeeded": stats.succeeded,
        "failed": stats.failed,
        "uptime_secs": stats.uptime_secs,
    }))
}

async fn list_tasks(State(s): State<AppState>) -> Json<Value> {
    {
        let cache = s.tasks_cache.lock().await;
        if let Some(cached) = cache.get() {
            return Json(cached.clone());
        }
    }
    let rows = s.store.load_history(100).await.unwrap_or_default();
    let body: Vec<_> = rows.into_iter().map(|t| json!({
        "id": t.id,
        "goal": t.goal,
        "status": t.status,
        "created_at": t.created_at,
        "completed_at": t.completed_at,
    })).collect();
    let value = json!({ "tasks": body });
    s.tasks_cache.lock().await.set(value.clone());
    Json(value)
}

async fn get_task(Path(id): Path<String>, State(s): State<AppState>) -> impl IntoResponse {
    let rows = s.store.load_history(500).await.unwrap_or_default();
    match rows.into_iter().find(|t| t.id.starts_with(&id)) {
        Some(t) => (StatusCode::OK, Json(json!({
            "id": t.id, "goal": t.goal, "status": t.status,
            "created_at": t.created_at, "completed_at": t.completed_at,
        }))).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "task not found" }))).into_response(),
    }
}

async fn cancel_task(Path(id): Path<String>, State(s): State<AppState>) -> impl IntoResponse {
    let rows = s.store.load_history(500).await.unwrap_or_default();
    let row = rows.into_iter().find(|t| t.id.starts_with(&id));
    match row {
        Some(t) => {
            let uuid = match t.id.parse::<uuid::Uuid>() {
                Ok(u) => u,
                Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({"error":"invalid id"}))).into_response(),
            };
            let task_id = TaskId(uuid);
            let cancelled = s.pool.cancel(&task_id).await;
            if cancelled {
                (StatusCode::OK, Json(json!({ "cancelled": true, "id": t.id }))).into_response()
            } else {
                (StatusCode::OK, Json(json!({ "cancelled": false, "reason": "task not running or already complete" }))).into_response()
            }
        }
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "task not found" }))).into_response(),
    }
}

const MAX_GOAL_LENGTH: usize = 2000;

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub goal: String,
    pub repo: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub constraints: Option<Vec<String>>,
    #[serde(default)]
    pub allowed_dirs: Option<Vec<String>>,
    #[serde(default)]
    pub forbidden_dirs: Option<Vec<String>>,
    #[serde(default)]
    pub max_retries: Option<u8>,
}

#[derive(Debug, Serialize)]
pub struct CreateTaskResponse {
    pub id: String,
    pub goal: String,
    pub queued: bool,
    pub duplicate_of: Option<String>,
}

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
    if let Some(repo) = req.repo { task.repo_path = Some(std::path::PathBuf::from(repo)); }
    if let Some(dirs) = req.allowed_dirs { task.allowed_dirs = dirs; }
    if let Some(dirs) = req.forbidden_dirs { task.forbidden_dirs = dirs; }
    if let Some(c) = req.constraints { task.constraints = c; }
    if let Some(r) = req.max_retries { task.max_retries = r; }

    s.tasks_cache.lock().await.invalidate();

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
    let body: Vec<_> = rows.into_iter().map(|p| json!({
        "id": p.id, "goal_keywords": p.goal_keywords,
        "avg_attempts": p.avg_attempts, "success_rate": p.success_rate,
        "last_seen": p.last_seen,
    })).collect();
    let value = json!({ "patterns": body });
    s.patterns_cache.lock().await.set(value.clone());
    Json(value)
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
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    )
}

/// Server-Sent Events — unidirectional push stream of pre-serialized AgentEvents.
async fn sse_handler(State(s): State<AppState>) -> impl IntoResponse {
    let rx = s.serialized_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|r: Result<Arc<str>, _>| async move {
        match r {
            Ok(json) => Some(Ok::<Event, std::convert::Infallible>(
                Event::default().data(json.as_ref()),
            )),
            Err(_) => None, // lagged — skip dropped events
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// WebSocket — on connect, send a state snapshot, then stream pre-serialized AgentEvent JSON.
async fn ws_handler(ws: WebSocketUpgrade, State(s): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, s))
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    // Snapshot: send current task list and stats on connect.
    if let Ok(rows) = state.store.load_history(100).await {
        let snapshot = json!({
            "type": "snapshot",
            "tasks": rows.iter().map(|t| json!({
                "id": t.id, "goal": t.goal, "status": t.status,
                "created_at": t.created_at,
            })).collect::<Vec<_>>(),
            "stats": {
                "running": state.pool.stats().running,
                "queued": state.pool.stats().queued,
                "succeeded": state.pool.stats().succeeded,
                "failed": state.pool.stats().failed,
                "uptime_secs": state.pool.stats().uptime_secs,
            }
        });
        if socket.send(Message::Text(snapshot.to_string())).await.is_err() {
            return;
        }
    }

    // Stream from pre-serialized channel: one JSON string broadcast to all subscribers, O(1) clone.
    let mut rx = state.serialized_tx.subscribe();
    loop {
        match rx.recv().await {
            Ok(json) => {
                if socket.send(Message::Text(json.as_ref().to_string())).await.is_err() {
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("ws subscriber lagged {n} events");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use lopi_orchestrator::AgentPool;
    use std::path::PathBuf;
    use tower::ServiceExt;

    async fn test_app() -> Router {
        test_app_with_auth(None).await
    }

    async fn test_app_with_auth(auth_token: Option<&str>) -> Router {
        let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let queue = TaskQueue::new();
        let pool = Arc::new(AgentPool::new(
            1,
            PathBuf::from("."),
            queue.clone(),
            bus.clone(),
        ));
        let state = AppState::new(store, bus, queue, pool, auth_token.map(|s| s.to_string()));
        build_app(state)
    }

    #[tokio::test]
    async fn health_returns_200() {
        let app = test_app().await;
        let resp = app
            .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn stats_returns_200_with_required_fields() {
        let app = test_app().await;
        let resp = app
            .oneshot(Request::builder().uri("/api/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.get("running").is_some());
        assert!(json.get("queued").is_some());
    }

    #[tokio::test]
    async fn tasks_list_returns_200() {
        let app = test_app().await;
        let resp = app
            .oneshot(Request::builder().uri("/api/tasks").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.get("tasks").is_some());
    }

    #[tokio::test]
    async fn metrics_returns_prometheus_text() {
        let app = test_app().await;
        let resp = app
            .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("text/plain"));
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("lopi_agents_running"));
    }

    #[tokio::test]
    async fn patterns_returns_200() {
        let app = test_app().await;
        let resp = app
            .oneshot(Request::builder().uri("/api/patterns").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.get("patterns").is_some());
    }

    #[tokio::test]
    async fn auth_rejects_missing_token() {
        let app = test_app_with_auth(Some("secret-token")).await;
        let resp = app
            .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_rejects_wrong_token() {
        let app = test_app_with_auth(Some("correct-token")).await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .header("Authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_accepts_correct_token() {
        let app = test_app_with_auth(Some("correct-token")).await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .header("Authorization", "Bearer correct-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_task_rejects_oversized_goal() {
        let app = test_app().await;
        let long_goal = "x".repeat(MAX_GOAL_LENGTH + 1);
        let body = serde_json::to_string(&serde_json::json!({
            "goal": long_goal,
        }))
        .unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/tasks")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn create_task_accepts_valid_goal() {
        let app = test_app().await;
        let body = serde_json::to_string(&serde_json::json!({
            "goal": "fix the flaky test",
        }))
        .unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/tasks")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }
}
