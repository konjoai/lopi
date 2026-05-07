#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
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

    include!("tests_extended.rs");
