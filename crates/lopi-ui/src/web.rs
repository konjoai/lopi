use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use lopi_core::{EventBus, TaskStatus};
use lopi_memory::MemoryStore;
use serde_json::json;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub store: MemoryStore,
    pub events: EventBus<TaskStatus>,
}

impl AppState {
    pub fn new(store: MemoryStore, events: EventBus<TaskStatus>) -> Self {
        Self { store, events }
    }
}

pub async fn serve(store: MemoryStore, events: EventBus<TaskStatus>, host: &str, port: u16) -> Result<()> {
    let state = AppState::new(store, events);
    let app = Router::new()
        .route("/", get(index))
        .route("/api/tasks", get(list_tasks))
        .route("/api/health", get(health))
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

/// WebSocket upgrade → streams `TaskStatus` events as JSON to each connected client.
async fn ws_tasks(
    ws: WebSocketUpgrade,
    State(s): State<AppState>,
) -> impl IntoResponse {
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
                    // Client disconnected.
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
