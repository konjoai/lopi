    #[tokio::test]
    async fn index_returns_html() {
        let app = test_app().await;
        let resp = get_req(app, "/").await;
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
        let resp = get_req(app, "/api/stats").await;
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
        let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
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
        let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn tasks_list_response_has_tasks_array() {
        let app = test_app().await;
        let resp = get_req(app, "/api/tasks").await;
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
        let resp = get_req(app, "/metrics").await;
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
    async fn health_response_has_service_field() {
        let app = test_app().await;
        let resp = get_req(app, "/api/health").await;
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
        let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
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
        let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
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
        let resp2 = send_req(app, "POST", "/api/tasks", Some(body)).await;
        assert_eq!(resp2.status(), StatusCode::CREATED);
        let bytes2 = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json2: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
        // duplicate_of is set when the task already exists
        assert!(json2["duplicate_of"].is_string());
    }

    // ─── F2 — per-task SSE stream + log ring buffer ──────────────────

    /// `GET /api/tasks/:id/logs` on a *known* task with no logs yet returns
    /// 200 with an empty array — "no logs" is a valid state. (An *unknown*
    /// id is 404, gated on task existence — see `f8_id_scoped_reads_status_codes`.)
    #[tokio::test]
    async fn task_logs_known_task_no_logs_returns_empty_array() {
        let (app, store) = test_app_with_store().await;
        let task = Task::new("known task, no logs yet");
        store.save_task(&task, "running").await.unwrap();
        let tid = task.id.0.to_string();

        let resp = get_req(app, &format!("/api/tasks/{tid}/logs")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["task_id"], tid);
        assert_eq!(json["logs"].as_array().unwrap().len(), 0);
    }

    /// Seed a couple of log rows via the store for a saved task, then read
    /// them back via the endpoint to verify the wire shape. The task is saved
    /// first because `get_logs` gates on task existence (Verify-1 F8).
    #[tokio::test]
    async fn task_logs_returns_seeded_rows_oldest_first() {
        let (app, store) = test_app_with_store().await;

        let task = Task::new("task with logs");
        store.save_task(&task, "running").await.unwrap();
        let tid = task.id.0.to_string();
        let now = chrono::Utc::now();
        for (i, level) in ["info", "warn", "info"].iter().enumerate() {
            store
                .record_task_log(&tid, now, level, &format!("line {i}"))
                .await
                .unwrap();
        }

        let resp = get_req(app, &format!("/api/tasks/{tid}/logs?n=10")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let logs = json["logs"].as_array().unwrap();
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0]["line"], "line 0");
        assert_eq!(logs[1]["level"], "warn");
        assert_eq!(logs[2]["line"], "line 2");
    }

    /// `GET /api/logs` on a fresh store returns 200 with an empty array.
    #[tokio::test]
    async fn global_logs_empty_store_returns_empty_array() {
        let app = test_app().await;
        let resp = get_req(app, "/api/logs").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["logs"].as_array().unwrap().len(), 0);
    }

    /// Seed rows across two tasks, then read the global tail and verify
    /// it interleaves tasks oldest-first with the wire shape intact.
    #[tokio::test]
    async fn global_logs_returns_rows_across_tasks_oldest_first() {
        let (app, store) = test_app_with_store().await;

        let now = chrono::Utc::now();
        store.record_task_log("t-a", now, "info", "a1").await.unwrap();
        store.record_task_log("t-b", now, "error", "b1").await.unwrap();
        store.record_task_log("t-a", now, "info", "a2").await.unwrap();

        let resp = get_req(app, "/api/logs?n=2").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let logs = json["logs"].as_array().unwrap();
        // n=2 → newest window (b1, a2) in chronological order.
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0]["task_id"], "t-b");
        assert_eq!(logs[0]["level"], "error");
        assert_eq!(logs[1]["line"], "a2");
    }

    // ─── F3 — per-agent rate limiting ────────────────────────────────

    /// Posting a rate limit with `max_per_minute: 0` returns 422.
    #[tokio::test]
    async fn agent_rate_limit_zero_per_minute_returns_422() {
        let app = test_app().await;
        let body = serde_json::to_string(&serde_json::json!({
            "max_per_minute": 0,
            "max_concurrent": 4,
        }))
        .unwrap();
        let resp = send_req(app, "POST", "/api/agents/alpha/rate-limit", Some(body)).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// Register → GET → DELETE → GET round trip for a per-agent limit.
    #[tokio::test]
    async fn agent_rate_limit_round_trip_register_get_delete() {
        let app = test_app().await;
        // 1. Register.
        let body = serde_json::to_string(&serde_json::json!({
            "max_per_minute": 30,
            "max_concurrent": 2,
        }))
        .unwrap();
        let resp = send_req(app.clone(), "POST", "/api/agents/alpha/rate-limit", Some(body)).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        // 2. GET.
        let resp = get_req(app.clone(), "/api/agents/alpha/rate-limit").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["max_per_minute"], 30);
        assert_eq!(json["max_concurrent"], 2);
        assert_eq!(json["in_flight"], 0);
        // 3. DELETE.
        let resp = send_req(app.clone(), "DELETE", "/api/agents/alpha/rate-limit", None).await;
        assert_eq!(resp.status(), StatusCode::OK);
        // 4. GET after delete → 404.
        let resp = get_req(app, "/api/agents/alpha/rate-limit").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
