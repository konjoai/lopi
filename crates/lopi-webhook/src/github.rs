use anyhow::Result;
use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use lopi_core::{Priority, Task, TaskSource};
use lopi_orchestrator::TaskQueue;
use serde_json::Value;
use std::net::SocketAddr;

#[derive(Clone)]
struct WebhookState {
    queue: TaskQueue,
}

pub async fn serve(queue: TaskQueue, addr: SocketAddr) -> Result<()> {
    let state = WebhookState { queue };
    let app = Router::new()
        .route("/webhook/github", post(handle))
        .with_state(state);
    tracing::info!("🪝 lopi github webhook on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle(
    State(s): State<WebhookState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let event = headers
        .get("X-GitHub-Event")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let repo = payload
        .get("repository")
        .and_then(|r| r.get("full_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // CI failure → inject a high-priority fix task.
    let conclusion = payload
        .get("workflow_run")
        .or_else(|| payload.get("check_run"))
        .and_then(|w| w.get("conclusion"))
        .and_then(|c| c.as_str());

    if matches!(conclusion, Some("failure") | Some("timed_out")) {
        let mut t = Task::new(format!("Investigate and fix CI failure on {repo}"));
        t.priority = Priority::High;
        t.source = TaskSource::Webhook { repo: repo.clone(), event: event.clone() };
        s.queue.push(t).await;
        tracing::info!("queued CI fix task for {repo}");
    }

    "ok"
}
