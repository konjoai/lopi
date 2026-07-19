#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::web::handlers::{apply_loop_fields, validate_goal};
use crate::web::types::{CreateTaskRequest, MAX_GOAL_LENGTH};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use lopi_core::{LopiConfig, Task};
use lopi_orchestrator::AgentPool;
use std::path::PathBuf;
use tower::ServiceExt;

async fn test_app() -> Router {
    test_app_with_auth(None).await
}

async fn test_app_with_auth(auth_token: Option<&str>) -> Router {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let pool = Arc::new(AgentPool::new(
        1,
        PathBuf::from("."),
        queue.clone(),
        bus.clone(),
    ));
    let state = AppState::new(store, bus, queue, pool, auth_token.map(ToString::to_string));
    build_app(state)
}

async fn test_app_with_config(config: LopiConfig) -> Router {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let pool = Arc::new(AgentPool::new(
        1,
        PathBuf::from("."),
        queue.clone(),
        bus.clone(),
    ));
    let state = AppState::new(store, bus, queue, pool, None).with_config(Some(config));
    build_app(state)
}

/// A `LopiConfig` with a custom `db_path`. Field-level serde defaults fill in
/// everything else, so the test only states what it cares about.
fn config_with_db_path(db_path: &str) -> LopiConfig {
    serde_json::from_value(serde_json::json!({
        "lopi": { "db_path": db_path },
        "claude": {},
        "git": {},
    }))
    .expect("minimal config deserializes")
}

async fn json_body(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Build an app rooted at a caller-supplied repo path so write-side handlers
/// (e.g. the loop strategy persister) touch a temp dir, not the real repo.
async fn test_app_with_repo(repo: PathBuf) -> Router {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let pool = Arc::new(AgentPool::new(1, repo.clone(), queue.clone(), bus.clone()));
    let state = AppState::new_with_repo(store, bus, queue, pool, None, repo);
    build_app(state)
}

/// Build an app *and* return its `MemoryStore` handle, for tests that seed
/// or assert on rows directly rather than only through the HTTP surface.
/// `hydrate_tools` is a harmless no-op on a fresh in-memory store (a missing
/// registry file is treated as empty), so it's always called here rather
/// than only by the one caller that originally needed it.
///
/// Unlike `test_app`/`test_app_with_repo`, the pool here is wired with
/// `.with_store(store.clone())` so a task created over HTTP actually
/// persists (`AgentPool::submit` skips its `save_task` call entirely when
/// the pool has no store) — otherwise a caller reading `t.id`/`store` back
/// out would see nothing, no matter what the HTTP response claimed.
async fn test_app_with_store() -> (Router, lopi_memory::MemoryStore) {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(1, PathBuf::from("."), queue.clone(), bus.clone()).with_store(store.clone()),
    );
    let state = AppState::new(store.clone(), bus, queue, pool, None);
    (build_app(state), store)
}

/// GET `uri` against `app` and return the response. Shared by every
/// read-only endpoint test in this module (and `tests_extended.rs`,
/// `include!`-ed into this same module). Named `get_req`, not `get`, since
/// several tests already bind a local `let get = ...`.
async fn get_req(app: Router, uri: &str) -> axum::response::Response {
    app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// Send `method` to `uri` against `app` with an optional pre-serialized JSON
/// `body` (`Content-Type: application/json` is set automatically when a
/// body is given), and return the response. Shared by every mutating
/// endpoint test. Takes an already-serialized `String` rather than a
/// `Value` so call sites that build their payload via
/// `serde_json::to_string(&json!({..}))` can pass it straight through.
async fn send_req(
    app: Router,
    method: &str,
    uri: &str,
    body: Option<String>,
) -> axum::response::Response {
    let builder = Request::builder().method(method).uri(uri);
    let (builder, payload) = match body {
        Some(s) => (
            builder.header("Content-Type", "application/json"),
            Body::from(s),
        ),
        None => (builder, Body::empty()),
    };
    app.oneshot(builder.body(payload).unwrap()).await.unwrap()
}

#[tokio::test]
async fn health_returns_200() {
    let app = test_app().await;
    let resp = get_req(app, "/api/health").await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn stats_returns_200_with_required_fields() {
    let app = test_app().await;
    let resp = get_req(app, "/api/stats").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("running").is_some());
    assert!(json.get("queued").is_some());
}

#[tokio::test]
async fn tasks_list_returns_200() {
    let app = test_app().await;
    let resp = get_req(app, "/api/tasks").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("tasks").is_some());
}

#[tokio::test]
async fn metrics_returns_prometheus_text() {
    let app = test_app().await;
    let resp = get_req(app, "/metrics").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/plain"));
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("lopi_agents_running"));
}

// `get_quality_trend`, `get_q_values`, and `get_agent_dag` (metrics_handlers.rs)
// had zero HTTP-level coverage — only the pure `dag_graph_json` helper was
// tested in-module.
#[tokio::test]
async fn quality_trend_returns_200_with_empty_runs_for_fresh_store() {
    let app = test_app().await;
    let resp = get_req(app, "/api/quality/trend").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert!(json["runs"].as_array().unwrap().is_empty());
    assert!(json.get("repo").is_some());
}

#[tokio::test]
async fn q_values_returns_200_with_empty_values_for_fresh_store() {
    let app = test_app().await;
    let resp = get_req(app, "/api/routing/q-values").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert!(json["values"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn agent_dag_unknown_task_returns_404() {
    let app = test_app().await;
    let resp = get_req(app, "/api/agents/00000000-0000-0000-0000-000000000000/dag").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn agent_dag_known_task_with_no_nodes_yet_returns_empty_graph() {
    let (app, store) = test_app_with_store().await;
    let task = Task::new("a task with no dag nodes recorded yet");
    let id = task.id.0.to_string();
    store.save_task(&task, "running").await.unwrap();

    let resp = get_req(app, &format!("/api/agents/{id}/dag")).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a known task with no recorded DAG yet is a 200 with an empty graph, not a 404"
    );
    let json = json_body(resp).await;
    assert_eq!(json["task_id"], id);
    assert!(json["nodes"].as_array().unwrap().is_empty());
    assert!(json["edges"].as_array().unwrap().is_empty());
}

// `list_runs`/`get_run_trace` (loop_runs_handlers.rs) had zero HTTP-level
// coverage — only the pure `attempt_json`/`parse_str_array` helpers were
// tested in-module.
#[tokio::test]
async fn list_runs_returns_empty_for_fresh_store() {
    let app = test_app().await;
    let resp = get_req(app, "/api/loop-engineering/runs").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert!(json["runs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_runs_summarizes_a_seeded_attempt() {
    let (app, store) = test_app_with_store().await;
    let task = Task::new("tighten retry backoff");
    store.save_task(&task, "success").await.unwrap();
    let mut attempt = lopi_core::Attempt::new(task.id, 1, "lopi/abc-attempt-1");
    attempt.outcome = "success".into();
    store.save_attempt(&attempt).await.unwrap();

    let resp = get_req(app, "/api/loop-engineering/runs").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    let runs = json["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["task_id"], task.id.0.to_string());
    assert_eq!(runs[0]["final_outcome"], "success");
    assert_eq!(runs[0]["attempts"], 1);
}

#[tokio::test]
async fn run_trace_unknown_task_returns_404() {
    let app = test_app().await;
    let resp = get_req(app, "/api/loop-engineering/runs/not-a-real-run").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn run_trace_known_task_returns_attempt_timeline() {
    let (app, store) = test_app_with_store().await;
    let task = Task::new("fix flaky scorer");
    store.save_task(&task, "success").await.unwrap();
    let mut attempt = lopi_core::Attempt::new(task.id, 1, "lopi/abc-attempt-1");
    attempt.outcome = "success".into();
    store.save_attempt(&attempt).await.unwrap();

    let id = task.id.0.to_string();
    let resp = get_req(app, &format!("/api/loop-engineering/runs/{id}")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert_eq!(json["task_id"], id);
    assert_eq!(json["goal"], "fix flaky scorer");
    let attempts = json["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["attempt"], 1);
    assert_eq!(attempts[0]["outcome"], "success");
}

#[tokio::test]
async fn auth_rejects_missing_token() {
    let app = test_app_with_auth(Some("secret-token")).await;
    let resp = get_req(app, "/api/health").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_rejects_wrong_token() {
    let app = test_app_with_auth(Some("correct-token")).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("Authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_accepts_correct_token() {
    let app = test_app_with_auth(Some("correct-token")).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("Authorization", "Bearer correct-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_task_rejects_oversized_goal() {
    let app = test_app().await;
    let long_goal = "x".repeat(MAX_GOAL_LENGTH + 1);
    let body = serde_json::to_string(&serde_json::json!({
        "goal": long_goal,
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_task_accepts_valid_goal() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "fix the flaky test",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[test]
fn validate_goal_table() {
    // (goal, expect_ok, label)
    let over = "x".repeat(MAX_GOAL_LENGTH + 1);
    let at_limit = "y".repeat(MAX_GOAL_LENGTH);
    let cases: &[(&str, bool, &str)] = &[
        ("", false, "empty"),
        ("   ", false, "spaces only"),
        ("\t\n  \r", false, "whitespace only"),
        (over.as_str(), false, "over length"),
        ("fix the flaky test", true, "normal goal"),
        (at_limit.as_str(), true, "exactly at the limit"),
        ("multi\nline\tgoal", true, "ordinary whitespace allowed"),
        ("ship 🚀 the feature", true, "emoji/unicode allowed"),
        ("bad\u{0000}goal", false, "NUL control char"),
        ("bad\u{001B}[31mgoal", false, "ANSI escape control char"),
    ];
    for (goal, expect_ok, label) in cases {
        assert_eq!(
            validate_goal(goal).is_ok(),
            *expect_ok,
            "validate_goal mismatch for case: {label}"
        );
    }
}

#[tokio::test]
async fn task_stream_rejects_malformed_id_with_400() {
    // Ops-2 finding #8: a malformed id returned its error body with an implicit
    // 200. An error body must carry a 4xx.
    let app = test_app().await;
    let resp = get_req(app, "/api/tasks/not-a-uuid/stream").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn config_endpoint_reflects_loaded_config() {
    // Ops-2 bug #6: the endpoint used to re-discover a file independently and
    // return null when `--config` pointed outside the standard search. It must
    // now mirror the config the server was actually started with.
    let app = test_app_with_config(config_with_db_path("/tmp/lopi-cfg-surfacing-test.db")).await;
    let resp = get_req(app, "/api/config").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert_eq!(json["source"], "file");
    assert_eq!(
        json["config"]["lopi"]["db_path"],
        "/tmp/lopi-cfg-surfacing-test.db"
    );
}

#[tokio::test]
async fn config_endpoint_reports_none_without_config() {
    let app = test_app().await;
    let resp = get_req(app, "/api/config").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert_eq!(json["source"], "none");
    assert!(json["config"].is_null());
}

#[tokio::test]
async fn create_task_rejects_empty_goal() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({ "goal": "" })).unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    // Ops-2 bug #5: an empty goal used to return 201 and spawn a real agent.
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_task_rejects_whitespace_only_goal() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({ "goal": "   \t\n " })).unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_task_with_priority_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "high priority task",
        "priority": "high",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("id").is_some());
    assert_eq!(json["goal"], "high priority task");
}

include!("tests_extended.rs");
include!("schedules_tests.rs");
include!("schedule_chains_tests.rs");
include!("task_stream_tests.rs");
include!("loop_tests.rs");
include!("quota_tests.rs");
include!("maxx_tests.rs");
include!("task_create_tests.rs");
include!("task_field_tests.rs");
include!("permission_mode_field_tests.rs");
