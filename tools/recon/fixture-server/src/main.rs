//! Sprint U-0 dashboard recon — dev-only fixture server.
//!
//! Serves the real, unmodified `lopi-ui` axum app (empty store — no
//! pre-seeded rows) plus one extra, recon-only control route,
//! `POST /recon/pump`, merged onto the same router. `panes`/`cards` in the
//! SvelteKit app are pure client-side session state (see
//! `web/src/lib/stores/stack.ts` — never rehydrated from the server), so the
//! *only* way to get a real StackCard on screen is to drive the real
//! composer UI to submit a real task through the real `POST /api/tasks`
//! handler. This binary cannot know that task's server-assigned id ahead of
//! time (it's a fresh `Uuid::new_v4()` every run), so Playwright creates the
//! card for real, reads the id back off the network response, and calls
//! `/recon/pump` with `{task_id, scenario}` to replay a deterministic
//! (fixed content, fixed millisecond delays, no RNG) `AgentEvent` sequence
//! for that id over the real event bus.
//!
//! Never spawns `AgentPool::run()` — no dispatch loop, no `claude`
//! subprocess anywhere in this binary. `/recon/pump` has no auth and is
//! reachable outside `lopi-ui`'s own auth layer — acceptable only because
//! this binary ever binds to `127.0.0.1` for a local recon capture session,
//! never shipped or exposed. Read-only with respect to every other file in
//! this repo; this crate exists only under `tools/recon/`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use axum::{extract::State, routing::post, Json, Router};
use chrono::Utc;
use lopi_core::{AgentEvent, EventBus, LogLevel, TaskId, TaskStatus};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use serde::Deserialize;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let port: u16 = arg_value(&args, "--port")
        .and_then(|s| s.parse().ok())
        .unwrap_or(4100);
    let db_path = arg_value(&args, "--db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/lopi-recon-{port}.db")));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(PathBuf::from(format!("{}{ext}", db_path.display())));
    }

    let store = MemoryStore::open(&db_path).await?;
    let bus: EventBus<AgentEvent> = EventBus::new(4096);
    let queue = TaskQueue::new();
    let repo_path = PathBuf::from("/tmp/lopi-recon-repo");
    let pool = Arc::new(
        AgentPool::new(4, repo_path, queue.clone(), bus.clone()).with_store(store.clone()),
    );

    let host = "127.0.0.1".to_string();
    lopi_ui::web::validate_auth_policy(None, true, &host)?;
    let state = lopi_ui::web::AppState::new(store, bus.clone(), queue, pool, None)
        .with_cors(Vec::new(), false);
    let app = lopi_ui::web::build_app(state).merge(recon_router(bus));

    let addr: std::net::SocketAddr = format!("{host}:{port}").parse()?;
    println!("fixture-server: http://{host}:{port}  (recon control: POST /recon/pump)");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn recon_router(bus: EventBus<AgentEvent>) -> Router {
    Router::new()
        .route("/recon/pump", post(pump_handler))
        .with_state(bus)
}

#[derive(Debug, Deserialize)]
struct PumpRequest {
    task_id: Uuid,
    scenario: String,
    /// Extra task ids for scenarios that drive several cards from one call
    /// (S3's four concurrent agents). Empty for single-task scenarios.
    #[serde(default)]
    extra_task_ids: Vec<Uuid>,
}

async fn pump_handler(State(bus): State<EventBus<AgentEvent>>, Json(req): Json<PumpRequest>) -> &'static str {
    let id = TaskId(req.task_id);
    match req.scenario.as_str() {
        "implementing" => {
            tokio::spawn(implementing_cycle(bus, id, "crates/lopi-webhook/src/github.rs", "cargo nextest run -p lopi-webhook"));
        }
        "implementing-set" => {
            let mut ids = vec![id];
            ids.extend(req.extra_task_ids.iter().map(|u| TaskId(*u)));
            let specs: [(&'static str, &'static str); 4] = [
                ("crates/lopi-webhook/src/github.rs", "cargo nextest run -p lopi-webhook"),
                ("src/telemetry.rs", "cargo test telemetry"),
                ("src/encode.rs", "cargo bench encode"),
                ("crates/lopi-orchestrator/src/schedule.rs", "cargo test schedule"),
            ];
            for (i, tid) in ids.into_iter().enumerate() {
                let (file, cmd) = specs[i % specs.len()];
                let bus2 = bus.clone();
                let stagger = 150 * (i as u64);
                tokio::spawn(async move {
                    wait(stagger).await;
                    implementing_cycle(bus2, tid, file, cmd).await;
                });
            }
        }
        "streaming" => {
            tokio::spawn(streaming_cycle(bus, id));
        }
        "gate-failure" => {
            tokio::spawn(gate_failure(bus, id));
        }
        "scrollback" => {
            tokio::spawn(scrollback(bus, id));
        }
        "pathological" => {
            tokio::spawn(pathological(bus, id));
        }
        "success" => {
            tokio::spawn(finish(bus, id, true));
        }
        "failure" => {
            tokio::spawn(finish(bus, id, false));
        }
        _ => {}
    }
    "ok"
}

async fn wait(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

fn log(bus: &EventBus<AgentEvent>, id: TaskId, line: &str) {
    bus.send(AgentEvent::LogLine { task_id: id, line: line.to_string(), level: LogLevel::Info, ts: Utc::now() });
}
fn tool_call(bus: &EventBus<AgentEvent>, id: TaskId, tool: &str, summary: &str) {
    bus.send(AgentEvent::ToolCall { task_id: id, tool: tool.to_string(), summary: summary.to_string() });
}
fn tool_result(bus: &EventBus<AgentEvent>, id: TaskId, preview: &str) {
    bus.send(AgentEvent::ToolResult { task_id: id, tool: String::new(), is_error: false, preview: preview.to_string() });
}
fn token_delta(bus: &EventBus<AgentEvent>, id: TaskId, out: u32, inp: u32, cache: u32) {
    bus.send(AgentEvent::TokenDelta { task_id: id, output_tokens: out, input_tokens: inp, cache_read_tokens: cache });
}
fn started(bus: &EventBus<AgentEvent>, id: TaskId, branch: &str, repo: &str) {
    bus.send(AgentEvent::TaskStarted { task_id: id, attempt: 1, branch: branch.to_string(), repo: repo.to_string() });
}
fn status(bus: &EventBus<AgentEvent>, id: TaskId, s: TaskStatus) {
    bus.send(AgentEvent::StatusChanged { task_id: id, status: s, attempt: 1 });
}
fn phase(bus: &EventBus<AgentEvent>, id: TaskId, p: &str) {
    bus.send(AgentEvent::Phase { task_id: id, phase: p.to_string() });
}

/// S2/S3 — one task's steady-state "implementing" cycle, repeated forever
/// on a fixed beat so a video/frame-strip capture always has fresh motion.
async fn implementing_cycle(bus: EventBus<AgentEvent>, id: TaskId, file: &'static str, cmd: &'static str) {
    started(&bus, id, "lopi/attempt-1", "~/lopi");
    wait(150).await;
    status(&bus, id, TaskStatus::Implementing);
    phase(&bus, id, "implementing");
    let mut n: u32 = 0;
    loop {
        log(&bus, id, &format!("Reading {file}"));
        tool_call(&bus, id, "Read", file);
        wait(220).await;
        tool_result(&bus, id, "312 lines");
        wait(180).await;
        log(&bus, id, "Applying edit");
        tool_call(&bus, id, "Edit", file);
        wait(220).await;
        tool_result(&bus, id, "applied 1 edit (+18 -4)");
        wait(180).await;
        tool_call(&bus, id, "Bash", cmd);
        wait(260).await;
        tool_result(&bus, id, "   Summary 41 tests run: 41 passed, 0 failed");
        n += 1;
        token_delta(&bus, id, 120 * n, 20 * n, 4200);
        wait(500).await;
    }
}

/// S4 — fine-grained streaming pump: short fixed chunks at a fast, fixed
/// cadence, so Census B can measure characters-per-DOM-mutation against a
/// known input distribution.
async fn streaming_cycle(bus: EventBus<AgentEvent>, id: TaskId) {
    started(&bus, id, "lopi/redis-cache-attempt-1", "~/kyro");
    wait(150).await;
    status(&bus, id, TaskStatus::Implementing);
    phase(&bus, id, "implementing");
    let chunks = [
        "I'll add a Redis-backed semantic cache",
        " to the retrieval path.",
        " First, a RedisCache struct",
        " wrapping the connection pool,",
        " then wire it into retrieve()",
        " behind a feature flag,",
        " then a TTL-based eviction policy.",
        " Starting with the cache struct now.",
    ];
    loop {
        for c in chunks {
            log(&bus, id, c);
            token_delta(&bus, id, 6, 0, 0);
            wait(160).await;
        }
        tool_call(&bus, id, "Edit", "crates/lopi-agent/src/retrieve.rs");
        wait(220).await;
        tool_result(&bus, id, "applied 1 edit (+9 -1)");
        wait(400).await;
    }
}

/// S5 — gate/verifier failure with the failure record left on screen.
async fn gate_failure(bus: EventBus<AgentEvent>, id: TaskId) {
    started(&bus, id, "lopi/scorer-thresholds-attempt-2", "~/lopi");
    wait(150).await;
    status(&bus, id, TaskStatus::Scoring);
    phase(&bus, id, "scoring");
    wait(300).await;
    bus.send(AgentEvent::ScoreUpdated { task_id: id, test_pass_rate: 0.63, lint_errors: 2, diff_lines: 54 });
    wait(400).await;
    bus.send(AgentEvent::VerifierVerdict {
        task_id: id,
        passed: false,
        gaps: vec![
            "error path for empty input is untested".to_string(),
            "public fn missing rustdoc".to_string(),
        ],
        fix_hints: vec![
            "add a unit test covering the empty case".to_string(),
            "document the public fn".to_string(),
        ],
        confidence: 0.81,
    });
    wait(300).await;
    status(&bus, id, TaskStatus::Retrying { attempt: 2 });
}

/// S9 — 2200 fixed-content log lines at a fixed 4ms cadence (~9s total).
async fn scrollback(bus: EventBus<AgentEvent>, id: TaskId) {
    started(&bus, id, "lopi/s9-scrollback-attempt-1", "~/lopi");
    status(&bus, id, TaskStatus::Implementing);
    let templates = [
        "cargo check: clean",
        "clippy: 0 hints",
        "Token pressure: 41% (within budget)",
        "Eviction fired: 6 turns reclaimed (2.1k tokens)",
        "Cache hit on system prompt — 1850 tokens saved",
        "Edit applied to crates/lopi-agent/src/runner.rs",
        "cargo nextest: 39 passed, 0 failed",
        "Rate limit window: 12% utilized",
    ];
    for i in 0u32..2200 {
        log(&bus, id, &format!("[{i:04}] {}", templates[(i as usize) % templates.len()]));
        wait(4).await;
    }
    log(&bus, id, "[done] 2200 lines emitted for S9 scrollback census");
}

/// S10 — pathological content: one 4000-char no-whitespace line, one line
/// with raw ANSI escapes.
async fn pathological(bus: EventBus<AgentEvent>, id: TaskId) {
    started(&bus, id, "lopi/s10-pathological-attempt-1", "~/lopi");
    status(&bus, id, TaskStatus::Implementing);
    wait(200).await;
    log(&bus, id, "Parsing vendor export — this one is not well-formed");
    wait(300).await;
    let long_line: String = "a1b2c3d4".repeat(500);
    log(&bus, id, &long_line);
    wait(300).await;
    log(&bus, id, "\u{1b}[31mERROR\u{1b}[0m: connection refused (\u{1b}[1mretry 3/5\u{1b}[0m) upstream=db-primary");
    wait(300).await;
    log(&bus, id, "Recovered after 2 malformed records");
}

/// S12 — drive a task straight to a terminal outcome (used to build the
/// "everything finished, all green" board from N real cards).
async fn finish(bus: EventBus<AgentEvent>, id: TaskId, success: bool) {
    started(&bus, id, "lopi/attempt-1", "~/lopi");
    wait(100).await;
    status(&bus, id, TaskStatus::Testing);
    wait(200).await;
    if success {
        bus.send(AgentEvent::ScoreUpdated { task_id: id, test_pass_rate: 0.97, lint_errors: 0, diff_lines: 42 });
        wait(200).await;
        let outcome = TaskStatus::Success { branch: "lopi/attempt-1".to_string(), pr_url: None };
        bus.send(AgentEvent::TaskCompleted { task_id: id, outcome, total_attempts: 1, successor: None });
    } else {
        bus.send(AgentEvent::ScoreUpdated { task_id: id, test_pass_rate: 0.41, lint_errors: 5, diff_lines: 30 });
        wait(200).await;
        let outcome = TaskStatus::Failed { reason: "exhausted max_retries — see task logs".to_string() };
        bus.send(AgentEvent::TaskCompleted { task_id: id, outcome, total_attempts: 3, successor: None });
    }
}
