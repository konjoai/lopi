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

#[tokio::test]
async fn cache_stats_returns_zero_for_empty_store() {
    let app = test_app().await;
    let resp = get_req(app, "/api/cache/stats").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["total_entries"], 0);
    assert_eq!(json["total_size_bytes"], 0);
    // hit_rate_last_hour is 0.0 when the window is empty
    assert!(json.get("hit_rate_last_hour").is_some());
}

#[tokio::test]
async fn clear_cache_returns_deleted_zero_when_empty() {
    let app = test_app().await;
    let resp = send_req(app, "DELETE", "/api/cache", None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["deleted"], 0);
}

#[tokio::test]
async fn invalidate_agent_cache_returns_zero_for_unknown() {
    let app = test_app().await;
    let resp = send_req(app, "DELETE", "/api/cache/agent/never-existed", None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["agent_id"], "never-existed");
    assert_eq!(json["deleted"], 0);
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

#[tokio::test]
async fn create_task_with_all_options_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "comprehensive task",
        "priority": "critical",
        "constraints": ["no new deps", "keep async"],
        "allowed_dirs": ["src/"],
        "forbidden_dirs": ["vendor/"],
        "max_retries": 5,
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

// ─── Backend-1: client_ref — stable task identity for a stack card ───────────

#[tokio::test]
async fn create_task_client_ref_survives_round_trip_through_the_store() {
    let (app, _store) = test_app_with_store().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "stack card round-trip",
        "client_ref": "card-abc123",
    }))
    .unwrap();
    let resp = send_req(app.clone(), "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["client_ref"], "card-abc123",
        "the create response echoes client_ref back verbatim"
    );
    let id = json["id"].as_str().unwrap().to_string();
    assert!(
        !id.is_empty(),
        "a stable task id must be assigned synchronously at create time"
    );

    // Round-trip: fetch the task back and confirm both the id and the
    // client_ref survived the write to SQLite, not just the in-memory path.
    let get_resp = get_req(app, &format!("/api/tasks/{id}")).await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_bytes = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_json: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert_eq!(
        get_json["id"], id,
        "the same stable id round-trips through the store"
    );
    assert_eq!(
        get_json["client_ref"], "card-abc123",
        "client_ref round-trips through the store, not just the create response"
    );
}

#[tokio::test]
async fn create_task_without_client_ref_omits_it_on_round_trip() {
    let (app, _store) = test_app_with_store().await;
    let body = serde_json::to_string(&serde_json::json!({"goal": "no client ref here"})).unwrap();
    let resp = send_req(app.clone(), "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["client_ref"].is_null(),
        "omitted client_ref must not be fabricated"
    );
    let id = json["id"].as_str().unwrap().to_string();

    let get_resp = get_req(app, &format!("/api/tasks/{id}")).await;
    let get_bytes = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_json: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert!(
        get_json["client_ref"].is_null(),
        "a task created without client_ref stays NULL through the store, not e.g. an empty string"
    );
}

// ─── Loop/verifier/report/override field exposure (web task-create surface) ──

#[tokio::test]
async fn create_task_with_loop_fields_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "verified capped loop",
        "verifier_required": true,
        "verifier_model": "claude-opus-4-7",
        "verifier_effort": "high",
        "report": "telegram",
        "max_iterations": 5,
        "model": "claude-haiku-4-5-20251001",
        "effort": "low",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_task_rejects_unreachable_report_channel() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "report to whatsapp",
        "report": "whatsapp",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("inbound-only"),
        "must surface the existing typed ReportChannelError, not a generic message"
    );
}

#[test]
fn apply_loop_fields_leaves_task_unchanged_when_all_fields_are_absent() {
    let mut task = Task::new("plain task");
    let baseline = format!("{task:?}");
    let req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "plain task"})).unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(
        format!("{task:?}"),
        baseline,
        "no new field may change defaults when omitted"
    );
}

#[test]
fn apply_loop_fields_threads_verifier_overrides_through_exactly() {
    let mut task = Task::new("verified task");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "verified task",
        "verifier_required": true,
        "verifier_model": "opus",
        "verifier_effort": "high",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert!(task.verifier_required);
    assert_eq!(task.verifier_model.as_deref(), Some("opus"));
    assert_eq!(task.verifier_effort.as_deref(), Some("high"));
}

#[test]
fn apply_loop_fields_accepts_telegram_and_rejects_whatsapp() {
    let mut telegram_task = Task::new("t");
    let telegram_req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "t", "report": "telegram"})).unwrap();
    assert!(apply_loop_fields(&mut telegram_task, &telegram_req).is_ok());
    assert_eq!(telegram_task.report.as_deref(), Some("telegram"));

    let mut whatsapp_task = Task::new("w");
    let whatsapp_req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "w", "report": "whatsapp"})).unwrap();
    let err = apply_loop_fields(&mut whatsapp_task, &whatsapp_req).unwrap_err();
    assert_eq!(err, lopi_core::ReportChannelError::WhatsappUnsupported);
    assert_eq!(
        whatsapp_task.report, None,
        "task must not be mutated on a rejected report channel"
    );
}

#[test]
fn apply_loop_fields_accepts_zero_max_iterations_as_the_infinite_sentinel() {
    let mut task = Task::new("infinite task");
    let req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "infinite task", "max_iterations": 0}))
            .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(
        task.max_iterations,
        Some(0),
        "0 must flow through as the infinite sentinel, not be rejected or coerced"
    );
}

#[test]
fn apply_loop_fields_threads_model_and_effort_overrides() {
    let mut task = Task::new("overridden task");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "overridden task",
        "model": "claude-opus-4-7",
        "effort": "max",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(task.model.as_deref(), Some("claude-opus-4-7"));
    assert_eq!(task.effort.as_deref(), Some("max"));
}

#[test]
fn apply_loop_fields_threads_gate_until_and_on_fail() {
    let mut task = Task::new("guarded task");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "guarded task",
        "gate": "./preflight.sh",
        "until": "cargo test",
        "on_fail": "continue",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(task.gate.as_deref(), Some("./preflight.sh"));
    assert_eq!(task.until.as_deref(), Some("cargo test"));
    assert_eq!(task.on_fail, Some(lopi_core::loop_config::OnFail::Continue));
}

#[tokio::test]
async fn create_task_with_guardrail_fields_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "gated loop",
        "gate": "true",
        "until": "false",
        "on_fail": "backoff",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn checkpoint_agent_persists_row_returns_201() {
    let app = test_app().await;
    let task_uuid = uuid::Uuid::new_v4();
    let body = serde_json::to_string(&serde_json::json!({
        "state": "planning",
        "attempt": 1,
        "last_plan": "step 1\nstep 2",
        "repo_path": "/tmp/repo",
    }))
    .unwrap();
    let resp = send_req(
        app,
        "POST",
        &format!("/api/agents/{task_uuid}/checkpoint"),
        Some(body),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json.get("checkpoint_id").is_some(),
        "response carries new id"
    );
    assert_eq!(json["task_id"], task_uuid.to_string());
}

#[tokio::test]
async fn checkpoint_agent_rejects_non_uuid_returns_400() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({"state": "planning"})).unwrap();
    let resp = send_req(app, "POST", "/api/agents/not-a-uuid/checkpoint", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_task_not_found_returns_404() {
    let app = test_app().await;
    let resp = get_req(app, "/api/tasks/nonexistent-task-id").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn cancel_task_not_found_returns_404() {
    let app = test_app().await;
    let resp = send_req(app, "DELETE", "/api/tasks/nonexistent-task-id", None).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn plans_returns_four_tiers() {
    let app = test_app().await;
    let resp = get_req(app, "/api/plans").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let plans = json["plans"].as_array().expect("plans is an array");
    assert_eq!(plans.len(), 4);
    let ids: Vec<&str> = plans.iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["free", "starter", "growth", "enterprise"]);
}

#[tokio::test]
async fn plans_response_has_required_fields() {
    let app = test_app().await;
    let resp = get_req(app, "/api/plans").await;
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for plan in json["plans"].as_array().unwrap() {
        assert!(plan.get("id").is_some(), "plan has id");
        assert!(plan.get("name").is_some(), "plan has name");
        assert!(plan.get("price_usd_per_month").is_some(), "plan has price");
        assert!(plan.get("max_agents").is_some(), "plan has max_agents");
        assert!(plan.get("features").is_some(), "plan has features");
        let max = plan["max_agents"].as_u64().unwrap();
        assert!(max >= 1, "max_agents at least 1");
    }
}

include!("tests_extended.rs");
include!("schedules_tests.rs");
include!("task_stream_tests.rs");
include!("loop_tests.rs");
include!("quota_tests.rs");
include!("maxx_tests.rs");
