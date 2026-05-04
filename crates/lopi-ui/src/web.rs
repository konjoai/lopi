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
use lopi_core::{EventBus, Priority, Task, TaskStatus};
use lopi_memory::MemoryStore;
use lopi_orchestrator::TaskQueue;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub store: MemoryStore,
    pub events: EventBus<TaskStatus>,
    pub queue: TaskQueue,
}

impl AppState {
    pub fn new(store: MemoryStore, events: EventBus<TaskStatus>, queue: TaskQueue) -> Self {
        Self { store, events, queue }
    }
}

pub async fn serve(
    store: MemoryStore,
    events: EventBus<TaskStatus>,
    queue: TaskQueue,
    host: &str,
    port: u16,
) -> Result<()> {
    let state = AppState::new(store, events, queue);
    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/:id", get(get_task))
        .route("/api/patterns", get(list_patterns))
        .route("/ws/tasks", get(ws_tasks))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!("🌐 lopi sail: listening on http://{addr}");
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

async fn get_task(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    let rows = s.store.load_history(500).await.unwrap_or_default();
    match rows.into_iter().find(|t| t.id.starts_with(&id)) {
        Some(t) => (StatusCode::OK, Json(json!({
            "id": t.id,
            "goal": t.goal,
            "status": t.status,
            "created_at": t.created_at,
            "completed_at": t.completed_at,
        }))).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "task not found" }))).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub goal: String,
    #[serde(default)]
    pub priority: Option<String>,
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
    if let Some(dirs) = req.allowed_dirs {
        task.allowed_dirs = dirs;
    }
    if let Some(dirs) = req.forbidden_dirs {
        task.forbidden_dirs = dirs;
    }
    if let Some(r) = req.max_retries {
        task.max_retries = r;
    }

    let task_id = task.id.0.to_string();
    s.store.save_task(&task, "queued").await.ok();
    let duplicate_of = s.queue.push(task).await.map(|id| id.0.to_string());

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
        "id": p.id,
        "goal_keywords": p.goal_keywords,
        "avg_attempts": p.avg_attempts,
        "success_rate": p.success_rate,
        "last_seen": p.last_seen,
    })).collect();
    Json(json!({ "patterns": body }))
}

async fn ws_tasks(ws: WebSocketUpgrade, State(s): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, s.events))
}

async fn handle_ws(mut socket: WebSocket, bus: EventBus<TaskStatus>) {
    let mut rx = bus.subscribe();
    loop {
        match rx.recv().await {
            Ok(status) => {
                let payload = match serde_json::to_string(&status) {
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
