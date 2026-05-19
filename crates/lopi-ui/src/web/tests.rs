#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::web::types::MAX_GOAL_LENGTH;
use axum::body::Body;
use axum::http::{Request, StatusCode};
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

#[tokio::test]
async fn health_returns_200() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn stats_returns_200_with_required_fields() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
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
async fn tools_list_returns_empty_by_default() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/tools")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["tools"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn tools_register_then_get_round_trip() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "name": "test-search",
        "description": "search the corpus",
        "parameters": {"type": "object", "properties": {"q": {"type": "string"}}},
        "timeout_ms": 5_000,
        "retries": 1,
        "updated_at": chrono::Utc::now(),
    }))
    .unwrap();
    let post = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tools")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post.status(), StatusCode::CREATED);

    let get = app
        .oneshot(
            Request::builder()
                .uri("/api/tools/test-search")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(get.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["name"], "test-search");
    assert_eq!(json["timeout_ms"], 5_000);
}

#[tokio::test]
async fn constellation_list_empty_by_default() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/constellations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["constellations"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn constellation_dispatch_unknown_returns_404() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/constellation/never/dispatch")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn constellation_register_then_dispatch_round_trip() {
    let app = test_app().await;
    let create_body = serde_json::to_string(&serde_json::json!({
        "name": "fleet-alpha",
        "agents": [
            {"agent_id": "agent-1", "weight": 1.0, "tags": [], "max_concurrent": 0},
            {"agent_id": "agent-2", "weight": 1.0, "tags": [], "max_concurrent": 0}
        ],
        "routing_strategy": {"kind": "round_robin"},
        "created_at": chrono::Utc::now(),
    }))
    .unwrap();
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/constellations")
                .header("Content-Type", "application/json")
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);

    let disp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/constellation/fleet-alpha/dispatch")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(disp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(disp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let agent = json["agent_id"].as_str().unwrap();
    assert!(agent == "agent-1" || agent == "agent-2");
    assert_eq!(json["strategy"], "round_robin");
}

#[tokio::test]
async fn cache_stats_returns_zero_for_empty_store() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/cache/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/cache")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/cache/agent/never-existed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["agent_id"], "never-existed");
    assert_eq!(json["deleted"], 0);
}

#[tokio::test]
async fn tools_get_unknown_returns_404() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/tools/never-registered")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn patterns_returns_200() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/patterns")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("patterns").is_some());
}

#[tokio::test]
async fn auth_rejects_missing_token() {
    let app = test_app_with_auth(Some("secret-token")).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tasks")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_task_accepts_valid_goal() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "fix the flaky test",
    }))
    .unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tasks")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_task_with_priority_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "high priority task",
        "priority": "high",
    }))
    .unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tasks")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tasks")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/agents/{task_uuid}/checkpoint"))
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("checkpoint_id").is_some(), "response carries new id");
    assert_eq!(json["task_id"], task_uuid.to_string());
}

#[tokio::test]
async fn checkpoint_agent_rejects_non_uuid_returns_400() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({"state": "planning"})).unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/agents/not-a-uuid/checkpoint")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_task_not_found_returns_404() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/tasks/nonexistent-task-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/tasks/nonexistent-task-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn plans_returns_four_tiers() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/plans")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/plans")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
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
