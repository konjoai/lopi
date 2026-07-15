// GET /api/quota tests. Included from tests.rs so these share the
// `test_app_started`-style helpers and the `super::*` import.

async fn test_app_quota_started() -> (Router, EventBus<AgentEvent>) {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(1, PathBuf::from("."), queue.clone(), bus.clone()).with_store(store.clone()),
    );
    let state = AppState::new(store, bus.clone(), queue, pool, None);
    state.quota.start(&state.bus).await.unwrap();
    (build_app(state), bus)
}

#[tokio::test]
async fn quota_reports_null_for_unobserved_windows() {
    let (app, _bus) = test_app_quota_started().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/quota")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert!(json["five_hour"].is_null());
    assert!(json["seven_day"].is_null());
}

#[tokio::test]
async fn quota_reflects_observed_windows_independently() {
    let (app, bus) = test_app_quota_started().await;
    bus.send(AgentEvent::ApiRetry {
        task_id: lopi_core::TaskId::new(),
        status: "allowed_warning".into(),
        limit_type: "seven_day".into(),
        utilization: 0.92,
        resets_at: Some(1_782_691_200),
    });
    bus.send(AgentEvent::ApiRetry {
        task_id: lopi_core::TaskId::new(),
        status: "allowed".into(),
        limit_type: "five_hour".into(),
        utilization: 0.10,
        resets_at: Some(1_700_000_000),
    });
    // Give the QuotaTracker's subscriber task a moment to process both sends.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/quota")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert!((json["seven_day"]["utilization"].as_f64().unwrap() - 0.92).abs() < 1e-6);
    assert_eq!(json["seven_day"]["resets_at"], 1_782_691_200);
    assert!((json["five_hour"]["utilization"].as_f64().unwrap() - 0.10).abs() < 1e-6);
    assert_eq!(json["five_hour"]["resets_at"], 1_700_000_000);
}
