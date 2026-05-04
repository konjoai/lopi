use anyhow::Result;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use lopi_memory::MemoryStore;
use serde_json::json;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct AppState {
    store: MemoryStore,
}

pub async fn serve(store: MemoryStore, host: &str, port: u16) -> Result<()> {
    let state = AppState { store };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/tasks", get(list_tasks))
        .route("/api/health", get(health))
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
