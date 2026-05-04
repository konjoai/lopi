use anyhow::Result;
use axum::{
    extract::{Form, State},
    response::IntoResponse,
    routing::post,
    Router,
};
use lopi_core::{Task, TaskSource};
use lopi_orchestrator::TaskQueue;
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Clone)]
pub struct WhatsappState {
    pub queue: TaskQueue,
}

#[derive(Debug, Deserialize)]
pub struct TwilioInbound {
    #[serde(rename = "Body")]
    pub body: String,
    #[serde(rename = "From")]
    pub from: Option<String>,
}

pub async fn serve(queue: TaskQueue, addr: SocketAddr) -> Result<()> {
    let state = WhatsappState { queue };
    let app = Router::new()
        .route("/webhook/whatsapp", post(handle))
        .with_state(state);
    tracing::info!("📱 lopi whatsapp webhook on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle(
    State(s): State<WhatsappState>,
    Form(payload): Form<TwilioInbound>,
) -> impl IntoResponse {
    let body = payload.body.trim();
    if let Some(goal) = body.strip_prefix("/task ") {
        let mut t = Task::new(goal);
        t.source = TaskSource::Webhook { repo: "whatsapp".into(), event: "message".into() };
        s.queue.push(t).await;
    }
    // Twilio expects 200 with TwiML; an empty 200 is fine.
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response/>"
}
