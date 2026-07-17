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
use std::collections::HashMap;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use lopi_memory::{TaskRow, TaskStatusCounts};

use super::AppState;

/// Build the WS `"snapshot"` payload sent on connect. Pulled out of
/// [`handle_ws`] so the JSON shape (per-task cost lookup, status counts) is
/// unit-testable without a live socket upgrade.
fn build_snapshot(
    rows: &[TaskRow],
    costs: &HashMap<String, f64>,
    counts: TaskStatusCounts,
    uptime_secs: u64,
) -> serde_json::Value {
    json!({
        "type": "snapshot",
        "tasks": rows.iter().map(|t| json!({
            "id": t.id, "goal": t.goal, "status": t.status,
            "created_at": t.created_at,
            "cost": costs.get(&t.id).copied().unwrap_or(0.0),
        })).collect::<Vec<_>>(),
        "stats": {
            "running": counts.running,
            "queued": counts.queued,
            "succeeded": counts.succeeded,
            "failed": counts.failed,
            "uptime_secs": uptime_secs,
        }
    })
}

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
        // Per-task cost so the client hydrates real spend for already-finished
        // tasks (Verify-1 F6: /budget + Overview cost read the client store and
        // showed $0). Counts come from the DB (`status_counts`), not the
        // per-pool `pool.stats()` that undercounts in multi-repo mode (F3/F4).
        let costs = state.store.task_costs().await.unwrap_or_default();
        let counts = state.store.status_counts().await.unwrap_or_default();
        let snapshot = build_snapshot(&rows, &costs, counts, state.pool.stats().uptime_secs);
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn row(id: &str, status: &str) -> TaskRow {
        TaskRow {
            id: id.to_string(),
            goal: format!("goal for {id}"),
            status: status.to_string(),
            created_at: "2026-07-16T00:00:00Z".to_string(),
            completed_at: None,
            client_ref: None,
        }
    }

    fn counts() -> TaskStatusCounts {
        TaskStatusCounts {
            running: 1,
            queued: 2,
            succeeded: 3,
            failed: 4,
        }
    }

    #[test]
    fn build_snapshot_looks_up_cost_per_task_id() {
        let rows = vec![row("t1", "done"), row("t2", "running")];
        let costs = HashMap::from([("t1".to_string(), 1.25)]);
        let snapshot = build_snapshot(&rows, &costs, counts(), 42);
        let tasks = snapshot["tasks"].as_array().unwrap();
        assert_eq!(tasks[0]["id"], "t1");
        assert_eq!(tasks[0]["cost"], 1.25);
        assert_eq!(
            tasks[1]["cost"], 0.0,
            "a task with no entry in the cost map defaults to 0.0, not null/missing"
        );
    }

    #[test]
    fn build_snapshot_carries_status_counts_and_uptime_through() {
        let snapshot = build_snapshot(&[], &HashMap::new(), counts(), 42);
        assert_eq!(snapshot["type"], "snapshot");
        assert_eq!(snapshot["stats"]["running"], 1);
        assert_eq!(snapshot["stats"]["queued"], 2);
        assert_eq!(snapshot["stats"]["succeeded"], 3);
        assert_eq!(snapshot["stats"]["failed"], 4);
        assert_eq!(snapshot["stats"]["uptime_secs"], 42);
        assert_eq!(
            snapshot["tasks"].as_array().unwrap().len(),
            0,
            "no task rows means an empty tasks array, not an omitted field"
        );
    }
}
