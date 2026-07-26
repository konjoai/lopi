//! Bridges the raw `AgentEvent` bus into `AppState`'s pre-serialized
//! broadcast, so every WS/SSE subscriber pays the JSON-serialization cost
//! once instead of once per subscriber. Split out of `AppState::new_with_repo`
//! purely to keep `mod.rs` within the 500-line file-size gate — no
//! behavioral change.
//!
//! Sprint F3 — log persistence no longer shares an await chain with the
//! live broadcast (`KT-3.1` reproduced the lag this caused). The bridge
//! hands each `LogLine` off to a bounded channel and moves on; a separate
//! drain task owns every `MemoryStore` write: batched inserts and a
//! time-based prune sweep, both off the hot path. See `LEDGER.md`'s F3
//! entry for the one-way door this opens: log persistence is now
//! best-effort under sustained overload, on purpose — the live stream has
//! no replay path, while `task_logs` is already pruned to `MAX_PER_TASK`
//! and lossy by design (`KT-3.3`).

use lopi_core::{AgentEvent, EventBus, LogLevel};
use lopi_memory::{MemoryStore, TaskLogInsert};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

/// Bound on the bridge→drain-task handoff channel. Sized well above one
/// batch (`BATCH_ROWS`) so a burst is absorbed without dropping; beyond
/// this, persistence sheds load rather than block the live stream
/// (Phase 4) — tuned against `KT-3.1`'s measured line rate, not a round
/// number chosen up front.
const PERSIST_CHANNEL_CAPACITY: usize = 4_096;
/// Row-count flush trigger for the drain task's batched insert.
const BATCH_ROWS: usize = 100;
/// Time flush trigger — bounds worst-case persisted-log staleness under low
/// volume, so a trickle of lines doesn't wait indefinitely for `BATCH_ROWS`
/// to fill.
const BATCH_INTERVAL: Duration = Duration::from_millis(50);
/// How often the drain task prunes each task it has written to. Replaces
/// the old every-64-inserts counter (Phase 3): pruning is off the
/// broadcast hot path now, so the count-based trigger no longer buys
/// anything a timer doesn't.
const PRUNE_SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// Log lines dropped because the persistence channel was full. Under
/// pressure, log persistence degrades before live events do: the live
/// stream has no replay path, while `task_logs` is already capped at
/// `MAX_PER_TASK` and lossy by design (Phase 4 / `KT-3.3`). Surfaced via
/// `/metrics` as `lopi_task_log_persist_dropped_total`.
static PERSIST_DROPPED: AtomicU64 = AtomicU64::new(0);

/// Current count of log lines dropped by the persistence handoff channel.
#[must_use]
pub(crate) fn persist_dropped_count() -> u64 {
    PERSIST_DROPPED.load(Ordering::Relaxed)
}

/// Subscribe to `bus`, serialize each event once, and re-broadcast on
/// `tx`. Side-effect: hands every [`AgentEvent::LogLine`] off to a drain
/// task that mirrors it into the `task_logs` SQLite table, so the per-task
/// SSE endpoint has a historical tail and the web UI can render progress
/// retroactively. This function's own receive loop never awaits a
/// `MemoryStore` method — see `event_bridge_bench.rs`'s
/// `live_broadcast_is_never_blocked_by_a_full_persist_channel` test.
pub fn spawn(bus: &EventBus<AgentEvent>, tx: Arc<broadcast::Sender<Arc<str>>>, store: MemoryStore) {
    spawn_with_tunables(
        bus,
        tx,
        store,
        PERSIST_CHANNEL_CAPACITY,
        PRUNE_SWEEP_INTERVAL,
    );
}

/// Same as [`spawn`], with the persistence channel's capacity and the
/// prune-sweep interval as explicit parameters so tests can force overflow,
/// or fast-forward a sweep, deterministically — without waiting out the
/// production-sized buffer or the real 30s interval.
fn spawn_with_tunables(
    bus: &EventBus<AgentEvent>,
    tx: Arc<broadcast::Sender<Arc<str>>>,
    store: MemoryStore,
    persist_capacity: usize,
    prune_sweep_interval: Duration,
) {
    let mut rx = bus.subscribe();
    let (persist_tx, persist_rx) = mpsc::channel::<TaskLogInsert>(persist_capacity);
    tokio::spawn(drain_persist_logs(store, persist_rx, prune_sweep_interval));
    tokio::spawn(async move {
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
                        try_persist(
                            &persist_tx,
                            TaskLogInsert {
                                task_id: task_id.0.to_string(),
                                ts: *ts,
                                level: level_str(level).to_string(),
                                line: line.clone(),
                            },
                        );
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

/// Hand `insert` to the drain task's channel without blocking. Drops (and
/// counts) it if the channel is full — the caller must never await here,
/// since this runs inline in the broadcast receive loop.
fn try_persist(persist_tx: &mpsc::Sender<TaskLogInsert>, insert: TaskLogInsert) -> bool {
    if persist_tx.try_send(insert).is_ok() {
        true
    } else {
        PERSIST_DROPPED.fetch_add(1, Ordering::Relaxed);
        false
    }
}

fn level_str(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
        LogLevel::Debug => "debug",
    }
}

/// Owns every `MemoryStore` write on the log-persistence side of the
/// bridge, entirely off the broadcast loop: batches inserts (`BATCH_ROWS`
/// rows or `BATCH_INTERVAL`, whichever comes first) in one transaction,
/// and periodically prunes every task it has written to
/// (`PRUNE_SWEEP_INTERVAL`).
async fn drain_persist_logs(
    store: MemoryStore,
    mut rx: mpsc::Receiver<TaskLogInsert>,
    prune_sweep_interval: Duration,
) {
    let mut batch: Vec<TaskLogInsert> = Vec::with_capacity(BATCH_ROWS);
    let mut dirty_tasks: HashSet<String> = HashSet::new();
    let mut ticker = tokio::time::interval(BATCH_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_prune_sweep = tokio::time::Instant::now();

    loop {
        tokio::select! {
            maybe_row = rx.recv() => {
                match maybe_row {
                    Some(row) => {
                        dirty_tasks.insert(row.task_id.clone());
                        batch.push(row);
                        if batch.len() >= BATCH_ROWS {
                            flush_batch(&store, &mut batch).await;
                        }
                    }
                    None => {
                        // Bridge shut down (sender dropped) — flush what's
                        // left and stop.
                        flush_batch(&store, &mut batch).await;
                        break;
                    }
                }
            }
            _ = ticker.tick() => {
                flush_batch(&store, &mut batch).await;
                if last_prune_sweep.elapsed() >= prune_sweep_interval {
                    prune_dirty_tasks(&store, &mut dirty_tasks).await;
                    last_prune_sweep = tokio::time::Instant::now();
                }
            }
        }
    }
}

async fn flush_batch(store: &MemoryStore, batch: &mut Vec<TaskLogInsert>) {
    if batch.is_empty() {
        return;
    }
    if let Err(e) = store.record_task_logs_batch(batch).await {
        tracing::warn!(
            "task_log batch persist failed ({} rows): {e}",
            batch.len()
        );
    }
    batch.clear();
}

async fn prune_dirty_tasks(store: &MemoryStore, dirty_tasks: &mut HashSet<String>) {
    for task_id in dirty_tasks.drain() {
        if let Err(e) = store.prune_task_logs(&task_id).await {
            tracing::warn!("task_log prune failed for {task_id}: {e}");
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::Utc;
    use lopi_core::TaskId;

    fn make_insert(task_id: &str, i: u64) -> TaskLogInsert {
        TaskLogInsert {
            task_id: task_id.to_string(),
            ts: Utc::now(),
            level: "info".to_string(),
            line: format!("line {i}"),
        }
    }

    /// Phase 4 — deterministic, no consumer draining the channel: exactly
    /// `capacity` sends succeed, the next one is dropped and counted.
    #[test]
    fn full_persist_channel_drops_and_counts_without_blocking() {
        let (tx, _rx) = mpsc::channel::<TaskLogInsert>(4);
        for i in 0..4 {
            assert!(
                try_persist(&tx, make_insert("t", i)),
                "first 4 sends must be accepted into an undrained capacity-4 channel"
            );
        }
        let before = persist_dropped_count();
        assert!(
            !try_persist(&tx, make_insert("t", 4)),
            "5th send must be dropped once the channel is full"
        );
        assert!(persist_dropped_count() > before);
    }

    /// Phase 1 — the live rebroadcast must never stall behind persistence,
    /// even when the persistence channel is a 1-slot bottleneck under a
    /// tight burst.
    #[tokio::test]
    async fn live_broadcast_is_never_blocked_by_a_full_persist_channel() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let bus: EventBus<AgentEvent> = EventBus::new(4_096);
        let (tx, mut sub_rx) = broadcast::channel::<Arc<str>>(4_096);
        let tx = Arc::new(tx);
        spawn_with_tunables(&bus, tx, store, 1, PRUNE_SWEEP_INTERVAL);

        let task_id = TaskId::new();
        let n: u64 = 2_000;
        for i in 0..n {
            bus.send(AgentEvent::LogLine {
                task_id,
                line: format!("l{i}"),
                level: LogLevel::Info,
                ts: Utc::now(),
            });
        }
        for _ in 0..n {
            tokio::time::timeout(Duration::from_millis(200), sub_rx.recv())
                .await
                .expect("live rebroadcast stalled — the bridge must not await persistence")
                .expect("broadcast channel closed unexpectedly");
        }
    }

    /// Phase 2 — a trickle (well below `BATCH_ROWS`) must still land,
    /// flushed by the timer trigger rather than sitting in the buffer
    /// forever waiting for a row count that will never arrive.
    #[tokio::test]
    async fn drain_task_flushes_a_trickle_on_the_timer() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let bus: EventBus<AgentEvent> = EventBus::new(64);
        let (tx, _sub_rx) = broadcast::channel::<Arc<str>>(64);
        let tx = Arc::new(tx);
        spawn(&bus, tx, store.clone());

        let task_id = TaskId::new();
        bus.send(AgentEvent::LogLine {
            task_id,
            line: "only one".to_string(),
            level: LogLevel::Info,
            ts: Utc::now(),
        });

        tokio::time::sleep(BATCH_INTERVAL * 4).await;
        let rows = store
            .load_task_logs(&task_id.0.to_string(), 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].line, "only one");
    }

    /// Phase 2 — a burst above `BATCH_ROWS` for one task lands with
    /// insertion order preserved.
    #[tokio::test]
    async fn batch_preserves_per_task_ordering() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let bus: EventBus<AgentEvent> = EventBus::new(4_096);
        let (tx, _sub_rx) = broadcast::channel::<Arc<str>>(4_096);
        let tx = Arc::new(tx);
        spawn(&bus, tx, store.clone());

        let task_id = TaskId::new();
        for i in 0..250u64 {
            bus.send(AgentEvent::LogLine {
                task_id,
                line: format!("line {i}"),
                level: LogLevel::Info,
                ts: Utc::now(),
            });
        }
        tokio::time::sleep(BATCH_INTERVAL * 6).await;
        let rows = store
            .load_task_logs(&task_id.0.to_string(), 500)
            .await
            .unwrap();
        assert_eq!(rows.len(), 250);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.line, format!("line {i}"));
        }
    }

    /// Phase 3 — pruning happens on the drain task's sweep timer, not the
    /// old every-64-inserts counter, and still enforces `MAX_PER_TASK`.
    /// Uses a short injected sweep interval (via `spawn_with_tunables`)
    /// rather than the real 30s production interval or paused virtual
    /// time — `tokio::time::pause`/`advance` doesn't mix safely with a
    /// real SQLite connection pool's own internal timers (a paused-time
    /// run of this test previously hit a spurious sqlx pool-acquire
    /// timeout), so a real, short sleep is the robust choice here.
    #[tokio::test]
    async fn prune_sweep_enforces_max_per_task_on_a_timer() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let bus: EventBus<AgentEvent> = EventBus::new(4_096);
        let (tx, _sub_rx) = broadcast::channel::<Arc<str>>(4_096);
        let tx = Arc::new(tx);
        let short_prune_interval = Duration::from_millis(100);
        spawn_with_tunables(&bus, tx, store.clone(), PERSIST_CHANNEL_CAPACITY, short_prune_interval);

        let task_id = TaskId::new();
        let total = lopi_memory::TASK_LOG_MAX_PER_TASK as u64 + 50;
        for i in 0..total {
            bus.send(AgentEvent::LogLine {
                task_id,
                line: format!("line {i}"),
                level: LogLevel::Info,
                ts: Utc::now(),
            });
        }
        // Let the batches flush, then let a couple of (short) prune sweeps
        // fire for real.
        tokio::time::sleep(short_prune_interval * 6).await;

        let rows = store
            .load_task_logs(&task_id.0.to_string(), total as i64 + 10)
            .await
            .unwrap();
        assert_eq!(rows.len() as i64, lopi_memory::TASK_LOG_MAX_PER_TASK);
    }
}

#[cfg(test)]
#[path = "event_bridge_bench.rs"]
mod event_bridge_bench;
