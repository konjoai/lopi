    #[tokio::test]
    async fn index_returns_html() {
        let app = test_app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/")
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
        assert!(ct.contains("text/html"));
    }

    #[tokio::test]
    async fn sse_endpoint_returns_200() {
        let app = test_app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sse")
                    .header("Accept", "text/event-stream")
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
        assert!(ct.contains("text/event-stream"));
    }

    #[tokio::test]
    async fn stats_has_all_fields() {
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
        assert!(json.get("succeeded").is_some());
        assert!(json.get("failed").is_some());
        assert!(json.get("uptime_secs").is_some());
    }

    #[tokio::test]
    async fn create_task_accepts_low_priority() {
        let app = test_app().await;
        let body = serde_json::to_string(&serde_json::json!({
            "goal": "low priority maintenance",
            "priority": "low",
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
    async fn create_task_accepts_normal_priority() {
        let app = test_app().await;
        let body = serde_json::to_string(&serde_json::json!({
            "goal": "normal priority work",
            "priority": "normal",
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
    async fn tasks_list_response_has_tasks_array() {
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
        assert!(json["tasks"].is_array());
    }

    #[tokio::test]
    async fn metrics_has_all_metric_names() {
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
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("lopi_agents_running"));
        assert!(body.contains("lopi_agents_queued"));
        assert!(body.contains("lopi_tasks_succeeded_total"));
        assert!(body.contains("lopi_tasks_failed_total"));
        assert!(body.contains("lopi_uptime_seconds"));
    }

    #[tokio::test]
    async fn patterns_response_has_patterns_array() {
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
        assert!(json["patterns"].is_array());
    }

    #[tokio::test]
    async fn health_response_has_service_field() {
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
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["service"], "lopi");
    }

    #[tokio::test]
    async fn create_task_response_has_queued_field() {
        let app = test_app().await;
        let body = serde_json::to_string(&serde_json::json!({
            "goal": "check queued field",
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
        assert!(json.get("queued").is_some());
        assert!(json.get("id").is_some());
        assert!(json.get("goal").is_some());
    }

    #[tokio::test]
    async fn create_task_with_repo_returns_201() {
        let app = test_app().await;
        let body = serde_json::to_string(&serde_json::json!({
            "goal": "task with repo override",
            "repo": "/tmp/myrepo",
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
    async fn duplicate_task_returns_201_with_duplicate_of() {
        let app = test_app().await;
        let goal = "deduplicated task goal unique xyz";
        let body = serde_json::to_string(&serde_json::json!({
            "goal": goal,
        }))
        .unwrap();
        // Submit first time
        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/tasks")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp1.status(), StatusCode::CREATED);
        let bytes1 = axum::body::to_bytes(resp1.into_body(), usize::MAX)
            .await
            .unwrap();
        let json1: serde_json::Value = serde_json::from_slice(&bytes1).unwrap();
        assert_eq!(json1["queued"], true);

        // Submit same goal again — should be deduplicated
        let resp2 = app
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
        assert_eq!(resp2.status(), StatusCode::CREATED);
        let bytes2 = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json2: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
        // duplicate_of is set when the task already exists
        assert!(json2["duplicate_of"].is_string());
    }

    /// Dead-letter list is empty on a fresh sail server.
    #[tokio::test]
    async fn dlq_list_empty_returns_empty_array() {
        let app = test_app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/tasks/dead-letter")
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
        assert_eq!(json["dead_letters"].as_array().map(Vec::len), Some(0));
    }

    /// Retrying a nonexistent DLQ row returns 404 cleanly.
    #[tokio::test]
    async fn dlq_retry_unknown_returns_404() {
        let app = test_app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/tasks/dead-letter/nope-not-real/retry")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Push a DLQ row directly via the store, then retry through the
    /// endpoint — the row is consumed and a new TaskId is returned.
    #[tokio::test]
    async fn dlq_retry_round_trip_takes_row_and_returns_new_task_id() {
        let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let queue = TaskQueue::new();
        let pool = Arc::new(AgentPool::new(
            1,
            std::path::PathBuf::from("."),
            queue.clone(),
            bus.clone(),
        ));
        let mut state = AppState::new(store.clone(), bus, queue, pool, None);
        state.hydrate_tools().await.ok();
        let app = build_app(state);

        // Seed a DLQ row.
        let mut input = lopi_memory::DeadLetterInput::new(
            lopi_core::TaskId::new(),
            "retry-me-via-endpoint",
        );
        input.repo_path = Some("/tmp".into());
        input.total_attempts = 3;
        input.last_error = Some("3 attempts failed".into());
        input.source = "cli".into();
        let dlq_id = store.push_dead_letter(&input).await.unwrap();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/tasks/dead-letter/{dlq_id}/retry"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["retried_from"], dlq_id);
        assert!(json["new_task_id"].is_string());
        // The DLQ row is consumed by retry — count returns to zero.
        assert_eq!(store.count_dead_letters().await.unwrap(), 0);
    }
