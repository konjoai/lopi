// Sprint S2 — CORS allowlist (Phase 2). `include!`-ed into `tests.rs` (see
// that file's doc comment on `get_req` for why) purely to keep `tests.rs`
// within the 500-line file-size gate.

#[tokio::test]
async fn cors_denies_non_allowlisted_origin() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("Origin", "https://evil.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.headers().get("access-control-allow-origin").is_none(),
        "a non-allowlisted origin must not get an Access-Control-Allow-Origin header back"
    );
}

/// The default config (no `[web].cors_allowed_origins` set) must still let
/// the web app talk to a local `lopi sail` — `http://localhost:5173` is
/// `web/vite.config.js`'s dev server port.
#[tokio::test]
async fn cors_allows_default_dev_origin_with_no_config() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("Origin", "http://localhost:5173")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.headers()
            .get("access-control-allow-origin")
            .expect("allowlisted dev origin should get the header back"),
        "http://localhost:5173"
    );
}

#[tokio::test]
async fn cors_permissive_opt_out_allows_any_origin() {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let pool = Arc::new(AgentPool::new(
        1,
        PathBuf::from("."),
        queue.clone(),
        bus.clone(),
    ));
    let state = AppState::new(store, bus, queue, pool, None).with_cors(Vec::new(), true);
    let app = build_app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("Origin", "https://anywhere.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.headers().get("access-control-allow-origin").is_some());
}
