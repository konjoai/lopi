// GET /api/economics tests. Included from tests.rs so these share the
// `test_app_started`-style helpers and the `super::*` import.

async fn test_app_economics(
    econ_cfg: Option<lopi_core::EconomicsConfig>,
) -> Router {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let mut pool_builder =
        AgentPool::new(1, PathBuf::from("."), queue.clone(), bus.clone()).with_store(store.clone());
    if let Some(cfg) = econ_cfg {
        if let Some(econ) = lopi_orchestrator::budget::Economics::new(&cfg, store.clone()) {
            pool_builder = pool_builder.with_economics(econ);
        }
    }
    let pool = Arc::new(pool_builder);
    let state = AppState::new(store, bus, queue, pool, None);
    build_app(state)
}

#[tokio::test]
async fn economics_reports_inactive_when_no_pool_configured() {
    let app = test_app_economics(None).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/economics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["active"], false);
}

#[tokio::test]
async fn economics_reports_headroom_and_tier_when_pool_configured() {
    let cfg = lopi_core::EconomicsConfig {
        pool: Some(lopi_core::Pool::AgentSdkCredits {
            monthly_allotment: lopi_core::Money::from_usd(50.0),
            resets_on: chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        }),
        ..lopi_core::EconomicsConfig::default()
    };
    let app = test_app_economics(Some(cfg)).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/economics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["active"], true);
    assert_eq!(json["tier"], "full");
    assert!((json["headroom_usd"].as_f64().unwrap() - 50.0).abs() < 1e-9);
    assert_eq!(json["pool_kind"], "agent_sdk_credits");
}
