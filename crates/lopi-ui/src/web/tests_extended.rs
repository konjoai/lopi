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
    async fn patterns_response_has_patterns_array() {
        let app = test_app().await;
        let resp = get_req(app, "/api/patterns").await;
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

    /// Dead-letter list is empty on a fresh sail server.
    #[tokio::test]
    async fn dlq_list_empty_returns_empty_array() {
        let app = test_app().await;
        let resp = get_req(app, "/api/tasks/dead-letter").await;
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
        let resp = send_req(app, "POST", "/api/tasks/dead-letter/nope-not-real/retry", None).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// P2 — `GET /api/audit` returns an empty events array on a fresh
    /// store, and a 0 next_cursor.
    #[tokio::test]
    async fn audit_empty_returns_empty_events_and_zero_cursor() {
        let app = test_app().await;
        let resp = get_req(app, "/api/audit").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["events"].as_array().map(Vec::len), Some(0));
        assert_eq!(json["next_cursor"], 0);
    }

    /// P2 — record three rows via the store, then page through them via
    /// the endpoint to verify the since_id cursor.
    #[tokio::test]
    async fn audit_paginates_via_since_id_cursor() {
        let (app, store) = test_app_with_store().await;

        for i in 0..3 {
            store
                .record_audit(&lopi_memory::AuditInput::new(format!("test.{i}")))
                .await
                .unwrap();
        }
        // First page — 2 rows.
        let resp = get_req(app.clone(), "/api/audit?n=2").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["events"].as_array().unwrap().len(), 2);
        let cursor = json["next_cursor"].as_i64().unwrap();
        // Second page — picks up after the cursor (1 row left).
        let resp2 = get_req(app, &format!("/api/audit?since_id={cursor}&n=10")).await;
        let bytes2 = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json2: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
        assert_eq!(json2["events"].as_array().unwrap().len(), 1);
        let last = &json2["events"][0];
        assert!(last["id"].as_i64().unwrap() > cursor);
    }

    /// P2 — `POST /api/tasks` with required_capabilities and an empty
    /// agent registry returns 422 with a structured error.
    #[tokio::test]
    async fn create_task_with_unsatisfiable_capabilities_returns_422() {
        let app = test_app().await;
        let body = serde_json::to_string(&serde_json::json!({
            "goal": "needs gpu-cuda capability",
            "required_capabilities": ["gpu-cuda"],
        }))
        .unwrap();
        let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["registered_agent_count"], 0);
        assert!(json["error"]
            .as_str()
            .is_some_and(|e| e.contains("capability")));
    }

    /// Push a DLQ row directly via the store, then retry through the
    /// endpoint — the row is consumed and a new TaskId is returned.
    #[tokio::test]
    async fn dlq_retry_round_trip_takes_row_and_returns_new_task_id() {
        let (app, store) = test_app_with_store().await;

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

        let resp = send_req(app, "POST", &format!("/api/tasks/dead-letter/{dlq_id}/retry"), None).await;
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

    // ─── F1 — agent health monitoring + heartbeat ────────────────────

    /// Hitting /api/agents/:id/health for an agent that never sent a
    /// heartbeat returns 404 with a structured error.
    #[tokio::test]
    async fn health_unknown_agent_returns_404() {
        let app = test_app().await;
        let resp = get_req(app, "/api/agents/ghost/health").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// POST /api/agents/:id/heartbeat creates the entry and returns the
    /// snapshot marked Healthy.
    #[tokio::test]
    async fn health_heartbeat_marks_healthy_and_returns_snapshot() {
        let app = test_app().await;
        let resp = send_req(app, "POST", "/api/agents/alpha/heartbeat", None).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "healthy");
        assert_eq!(json["agent_id"], "alpha");
        assert!(json["last_seen"].is_string());
        assert_eq!(json["consecutive_failures"], 0);
    }

    // ─── F2 — per-task SSE stream + log ring buffer ──────────────────

    /// `GET /api/tasks/:id/logs` on an unknown task returns 200 with an
    /// empty array — not 404 — since "no logs" is a valid state.
    #[tokio::test]
    async fn task_logs_unknown_returns_empty_array() {
        let app = test_app().await;
        let resp = get_req(app, "/api/tasks/never-logged/logs").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["task_id"], "never-logged");
        assert_eq!(json["logs"].as_array().unwrap().len(), 0);
    }

    /// Seed a couple of log rows via the store, then read them back via
    /// the endpoint to verify the wire shape.
    #[tokio::test]
    async fn task_logs_returns_seeded_rows_oldest_first() {
        let (app, store) = test_app_with_store().await;

        let tid = "task-with-logs";
        let now = chrono::Utc::now();
        for (i, level) in ["info", "warn", "info"].iter().enumerate() {
            store
                .record_task_log(tid, now, level, &format!("line {i}"))
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

    /// /api/agents/health/summary on a fresh server reports all zeros.
    #[tokio::test]
    async fn health_summary_empty_returns_zeros() {
        let app = test_app().await;
        let resp = get_req(app, "/api/agents/health/summary").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["total"], 0);
        assert_eq!(json["healthy"], 0);
        assert_eq!(json["degraded"], 0);
        assert_eq!(json["dead"], 0);
    }
