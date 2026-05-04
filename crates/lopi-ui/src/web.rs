use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use lopi_core::{AgentEvent, EventBus, Priority, Task, TaskId};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub store: MemoryStore,
    pub bus: EventBus<AgentEvent>,
    pub queue: TaskQueue,
    pub pool: Arc<AgentPool>,
}

pub async fn serve(
    store: MemoryStore,
    bus: EventBus<AgentEvent>,
    queue: TaskQueue,
    pool: Arc<AgentPool>,
    host: &str,
    port: u16,
) -> Result<()> {
    let state = AppState { store, bus, queue, pool };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/:id", get(get_task).delete(cancel_task))
        .route("/api/stats", get(get_stats))
        .route("/api/patterns", get(list_patterns))
        .route("/ws", get(ws_handler))
        // Legacy endpoint — kept for compat.
        .route("/ws/tasks", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!("🌐 lopi sail: http://{addr}  ws://{addr}/ws");
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

async fn list_tasks(State(s): State<AppState>) -> impl IntoResponse {
    let rows = s.store.load_history(100).await.unwrap_or_default();
    let body: Vec<_> = rows.into_iter().map(|t| json!({
        "id": t.id,
        "goal": t.goal,
        "status": t.status,
        "created_at": t.created_at,
        "completed_at": t.completed_at,
    })).collect();
    Json(json!({ "tasks": body }))
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
    // Parse the ID prefix to a full TaskId.
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
    if let Some(dirs) = req.allowed_dirs { task.allowed_dirs = dirs; }
    if let Some(dirs) = req.forbidden_dirs { task.forbidden_dirs = dirs; }
    if let Some(c) = req.constraints { task.constraints = c; }
    if let Some(r) = req.max_retries { task.max_retries = r; }

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

async fn list_patterns(State(s): State<AppState>) -> impl IntoResponse {
    let rows = s.store.load_patterns(50).await.unwrap_or_default();
    let body: Vec<_> = rows.into_iter().map(|p| json!({
        "id": p.id, "goal_keywords": p.goal_keywords,
        "avg_attempts": p.avg_attempts, "success_rate": p.success_rate,
        "last_seen": p.last_seen,
    })).collect();
    Json(json!({ "patterns": body }))
}

/// WebSocket — on connect, send a state snapshot, then stream `AgentEvent` as JSON.
async fn ws_handler(ws: WebSocketUpgrade, State(s): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, s))
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    // Snapshot: send current task list first.
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

    let mut rx = state.bus.subscribe();
    loop {
        match rx.recv().await {
            Ok(ev) => {
                let payload = match serde_json::to_string(&ev) {
                    Ok(j) => j,
                    Err(_) => continue,
                };
                if socket.send(Message::Text(payload)).await.is_err() {
                    return;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("ws subscriber lagged {n} events");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}
