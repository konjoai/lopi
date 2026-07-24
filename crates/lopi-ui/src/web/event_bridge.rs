//! Bridges the raw `AgentEvent` bus into `AppState`'s pre-serialized
//! broadcast, so every WS/SSE subscriber pays the JSON-serialization cost
//! once instead of once per subscriber. Split out of `AppState::new_with_repo`
//! purely to keep `mod.rs` within the 500-line file-size gate — no
//! behavioral change.

use lopi_core::{AgentEvent, EventBus};
use lopi_memory::MemoryStore;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Subscribe to `bus`, serialize each event once, and re-broadcast on
/// `tx`. Side-effect: mirrors every [`AgentEvent::LogLine`] into the
/// `task_logs` SQLite table so the per-task SSE endpoint has a historical
/// tail and the web UI can render progress retroactively.
pub fn spawn(bus: &EventBus<AgentEvent>, tx: Arc<broadcast::Sender<Arc<str>>>, store: MemoryStore) {
    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        let mut log_counter: u64 = 0;
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if let Ok(json) = serde_json::to_string(&ev) {
                        let _ = tx.send(Arc::from(json.as_str()));
                    }
                    if let AgentEvent::LogLine {
                        task_id,
                        line,
                        level,
                        ts,
                    } = &ev
                    {
                        persist_log_line(&store, &mut log_counter, task_id, level, *ts, line).await;
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

/// Persist one `LogLine` event and amortize log pruning (every 64 inserts)
/// so it isn't a per-line `DELETE` scan.
async fn persist_log_line(
    store: &MemoryStore,
    log_counter: &mut u64,
    task_id: &lopi_core::TaskId,
    level: &lopi_core::LogLevel,
    ts: chrono::DateTime<chrono::Utc>,
    line: &str,
) {
    let tid = task_id.0.to_string();
    let lvl = match level {
        lopi_core::LogLevel::Info => "info",
        lopi_core::LogLevel::Warn => "warn",
        lopi_core::LogLevel::Error => "error",
        lopi_core::LogLevel::Debug => "debug",
    };
    if let Err(e) = store.record_task_log(&tid, ts, lvl, line).await {
        tracing::warn!("task_log persist failed: {e}");
    }
    *log_counter = log_counter.wrapping_add(1);
    if log_counter.is_multiple_of(64) {
        if let Err(e) = store.prune_task_logs(&tid).await {
            tracing::warn!("task_log prune failed: {e}");
        }
    }
}
