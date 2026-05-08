// SSE and WebSocket streaming handlers.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
};
use futures::StreamExt as _;
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use super::AppState;

/// Server-Sent Events — unidirectional push stream of pre-serialized `AgentEvent`s.
pub(super) async fn sse_handler(State(s): State<AppState>) -> impl IntoResponse {
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

/// WebSocket — on connect, send a state snapshot, then stream pre-serialized `AgentEvent` JSON.
pub(super) async fn ws_handler(
    ws: WebSocketUpgrade,
    State(s): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, s))
}

pub(super) async fn handle_ws(mut socket: WebSocket, state: AppState) {
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
        if socket
            .send(Message::Text(snapshot.to_string()))
            .await
            .is_err()
        {
            return;
        }
    }

    // Stream from pre-serialized channel: one JSON string broadcast to all subscribers, O(1) clone.
    let mut rx = state.serialized_tx.subscribe();
    loop {
        match rx.recv().await {
            Ok(json) => {
                if socket
                    .send(Message::Text(json.as_ref().to_string()))
                    .await
                    .is_err()
                {
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
