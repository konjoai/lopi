//! Sprint F3 kill-test harness (KT-3.1 / KT-3.2) — synthetic load against the
//! *real* bridge and store code.
//!
//! This is not a live Claude Code agent fleet on real M3 hardware — this
//! session's environment cannot run four concurrent Claude sessions for a
//! 30-run paired comparison. Instead it drives the actual `event_bridge::spawn`
//! function against a real disk-backed `MemoryStore` (same dual-pool WAL
//! config as production) with synthetic `AgentEvent::LogLine` traffic at a
//! stated, documented rate. That substitution is recorded here and in
//! `.konjo/killtests/F3/KT-3.1.md` rather than left implicit — see CLAUDE.md's
//! instruction to measure rather than assume.
//!
//! Run one sample:
//! ```text
//! cargo test -p lopi-ui --release bridge_load_bench -- --ignored --nocapture --test-threads=1
//! ```
//! Prints a single `BENCH_RESULT {json}` line; the 30-run paired samples in
//! `benchmarks/results/` were collected by invoking the compiled release
//! test binary directly in a loop (see that directory's `summary.md`).

// print_stdout: this bench's whole job is to print a `BENCH_RESULT {json}`
// line for the collection scripts in `benchmarks/results/` to grep out of
// `--nocapture` test output — that's a deliberate reporting mechanism, not
// a stray debug print.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)]

use super::spawn;
use lopi_core::{AgentEvent, EventBus, LogLevel, TaskId};
use lopi_memory::MemoryStore;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

/// Concurrent synthetic agents — matches KT-3.1's "four concurrent agents".
const AGENTS: usize = 4;
/// Log lines emitted per agent.
const LINES_PER_AGENT: u64 = 3_000;
/// Synthetic per-line delay. ~250 lines/sec/agent, chosen to approximate a
/// `--include-partial-messages` stream — a documented assumption, not a
/// measurement of a real Claude Code session.
const LINE_INTERVAL: Duration = Duration::from_millis(4);

#[derive(Serialize)]
struct BenchResult {
    lines_sent: u64,
    lines_received: u64,
    lagged_events: u64,
    lagged_sum_n: u64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    /// p95 broadcast latency for lines whose predecessor was a multiple of
    /// 64 (the pre-fix prune trigger) — answers KT-3.2.
    prune_boundary_p95_ms: f64,
    /// p95 latency for every other line — the KT-3.2 comparison baseline.
    steady_state_p95_ms: f64,
    rows_in_db: u64,
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "manual benchmark — see benchmarks/results/*/run_bench.sh"]
async fn bridge_load_bench() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = MemoryStore::open(dir.path().join("bench.db"))
        .await
        .expect("open bench store");

    let bus: EventBus<AgentEvent> = EventBus::new(512);
    let (tx, mut sub_rx) = broadcast::channel::<Arc<str>>(512);
    let tx = Arc::new(tx);
    spawn(&bus, tx, store.clone());

    let latencies: Arc<Mutex<Vec<(f64, bool)>>> = Arc::new(Mutex::new(Vec::new()));
    let lagged_events = Arc::new(AtomicU64::new(0));
    let lagged_sum = Arc::new(AtomicU64::new(0));
    let received = Arc::new(AtomicU64::new(0));

    let collector = {
        let latencies = latencies.clone();
        let lagged_events = lagged_events.clone();
        let lagged_sum = lagged_sum.clone();
        let received = received.clone();
        tokio::spawn(async move {
            loop {
                match sub_rx.recv().await {
                    Ok(json) => {
                        if let Ok(AgentEvent::LogLine { ts, line, .. }) =
                            serde_json::from_str::<AgentEvent>(&json)
                        {
                            let latency_ms =
                                (chrono::Utc::now() - ts).num_microseconds().unwrap_or(0) as f64
                                    / 1000.0;
                            let prune_boundary = line.ends_with("#PB");
                            latencies
                                .lock()
                                .expect("lock")
                                .push((latency_ms, prune_boundary));
                            received.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        lagged_events.fetch_add(1, Ordering::Relaxed);
                        lagged_sum.fetch_add(n, Ordering::Relaxed);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    };

    let bus = Arc::new(bus);
    let mut handles = Vec::with_capacity(AGENTS);
    for a in 0..AGENTS {
        let bus = bus.clone();
        handles.push(tokio::spawn(async move {
            let task_id = TaskId::new();
            for i in 1..=LINES_PER_AGENT {
                let marker = if i.is_multiple_of(65) { " #PB" } else { "" };
                bus.send(AgentEvent::LogLine {
                    task_id,
                    line: format!("agent {a} line {i}{marker}"),
                    level: LogLevel::Info,
                    ts: chrono::Utc::now(),
                });
                tokio::time::sleep(LINE_INTERVAL).await;
            }
        }));
    }
    for h in handles {
        h.await.expect("agent task panicked");
    }
    // Let the collector and drain task catch up.
    tokio::time::sleep(Duration::from_millis(1_000)).await;
    collector.abort();

    let rows_in_db = store.count_task_logs().await.unwrap_or(u64::MAX);

    let mut all_lat = latencies.lock().expect("lock").clone();
    all_lat.sort_by(|a, b| a.0.total_cmp(&b.0));
    let sorted_ms: Vec<f64> = all_lat.iter().map(|(ms, _)| *ms).collect();
    let prune_ms: Vec<f64> = {
        let mut v: Vec<f64> = all_lat
            .iter()
            .filter(|(_, pb)| *pb)
            .map(|(ms, _)| *ms)
            .collect();
        v.sort_by(f64::total_cmp);
        v
    };
    let steady_ms: Vec<f64> = {
        let mut v: Vec<f64> = all_lat
            .iter()
            .filter(|(_, pb)| !*pb)
            .map(|(ms, _)| *ms)
            .collect();
        v.sort_by(f64::total_cmp);
        v
    };

    let result = BenchResult {
        lines_sent: AGENTS as u64 * LINES_PER_AGENT,
        lines_received: received.load(Ordering::Relaxed),
        lagged_events: lagged_events.load(Ordering::Relaxed),
        lagged_sum_n: lagged_sum.load(Ordering::Relaxed),
        p50_ms: percentile(&sorted_ms, 0.50),
        p95_ms: percentile(&sorted_ms, 0.95),
        p99_ms: percentile(&sorted_ms, 0.99),
        prune_boundary_p95_ms: percentile(&prune_ms, 0.95),
        steady_state_p95_ms: percentile(&steady_ms, 0.95),
        rows_in_db,
    };
    println!(
        "BENCH_RESULT {}",
        serde_json::to_string(&result).expect("serialize bench result")
    );
}
