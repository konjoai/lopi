use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures::StreamExt as _;
use lopi_core::{AgentEvent, EventBus, Priority, Task, TaskId};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;
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
}

pub async fn serve(
    store: MemoryStore,
    bus: EventBus<AgentEvent>,
    queue: TaskQueue,
    pool: Arc<AgentPool>,
    host: &str,
    port: u16,
) -> Result<()> {
    let (serialized_tx, _) = broadcast::channel::<Arc<str>>(512);
    let serialized_tx = Arc::new(serialized_tx);

    // Bridge: subscribe to raw AgentEvent bus, serialize once, re-broadcast as Arc<str>.
    // One serialization per event regardless of how many WS/SSE connections are open.
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

    let state = AppState {
        store,
        bus,
        queue,
        pool,
        serialized_tx,
        tasks_cache: Arc::new(Mutex::new(TtlCache::new(Duration::from_secs(2)))),
        patterns_cache: Arc::new(Mutex::new(TtlCache::new(Duration::from_secs(30)))),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/:id", get(get_task).delete(cancel_task))
        .route("/api/stats", get(get_stats))
        .route("/api/patterns", get(list_patterns))
        .route("/metrics", get(metrics))
        .route("/sse", get(sse_handler))
        .route("/ws", get(ws_handler))
        // Legacy endpoint — kept for compat.
        .route("/ws/tasks", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!("🌐 lopi sail: http://{addr}  ws://{addr}/ws  sse://{addr}/sse  metrics://{addr}/metrics");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
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
    (StatusCode::CREATED, Json(resp))
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
