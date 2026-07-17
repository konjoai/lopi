// Schedule-chain (whole-stack cron) endpoint tests. Included from tests.rs so
// these share the `test_app`/`json_post`/`json_put`/`body_json` helpers
// already defined by `schedules_tests.rs`.

/// Build an app whose `ChainScheduleManager` has been started, so
/// register/run-now exercise the live scheduler path rather than the
/// error-swallowing fallback.
async fn test_app_chains_started() -> Router {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(1, PathBuf::from("."), queue.clone(), bus.clone()).with_store(store.clone()),
    );
    let state = AppState::new(store, bus, queue, pool, None);
    state.schedule_chains.start().await.unwrap();
    build_app(state)
}

fn sample_chain() -> serde_json::Value {
    serde_json::json!({
        "name": "incident stack",
        "cron": "0 2 * * *",
        "on_fail": "stop",
        "steps": [
            { "goal": "research the incident root cause" },
            { "goal": "write a kill-test that reproduces it" },
            { "goal": "implement the fix" },
        ],
    })
}

/// Create a chain and return its id.
async fn create_one_chain(app: &Router) -> String {
    let resp = app
        .clone()
        .oneshot(json_post("/api/schedule-chains", sample_chain()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn create_chain_returns_201_with_next_runs_and_steps_in_order() {
    let app = test_app_chains_started().await;
    let resp = app
        .clone()
        .oneshot(json_post("/api/schedule-chains", sample_chain()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "incident stack");
    assert_eq!(json["enabled"], true);
    assert!(json["next_runs"].as_array().unwrap().len() == 3);
    let steps = json["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0]["goal"], "research the incident root cause");
    assert_eq!(steps[0]["step_order"], 0);
    assert_eq!(steps[2]["goal"], "implement the fix");
}

#[tokio::test]
async fn create_chain_rejects_invalid_cron() {
    let app = test_app_chains_started().await;
    let mut body = sample_chain();
    body["cron"] = serde_json::json!("not a cron");
    let resp = app
        .oneshot(json_post("/api/schedule-chains", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_chain_rejects_empty_steps() {
    let app = test_app_chains_started().await;
    let mut body = sample_chain();
    body["steps"] = serde_json::json!([]);
    let resp = app
        .oneshot(json_post("/api/schedule-chains", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_chain_rejects_empty_step_goal() {
    let app = test_app_chains_started().await;
    let mut body = sample_chain();
    body["steps"][1]["goal"] = serde_json::json!("   ");
    let resp = app
        .oneshot(json_post("/api/schedule-chains", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn list_chains_includes_created() {
    let app = test_app_chains_started().await;
    let id = create_one_chain(&app).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/schedule-chains")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let arr = json["chains"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], id);
}

#[tokio::test]
async fn get_chain_unknown_returns_404() {
    let app = test_app_chains_started().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/schedule-chains/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_chain_includes_run_history_field() {
    let app = test_app_chains_started().await;
    let id = create_one_chain(&app).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/schedule-chains/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert!(json["runs"].is_array());
}

#[tokio::test]
async fn update_chain_replaces_steps() {
    let app = test_app_chains_started().await;
    let id = create_one_chain(&app).await;
    let mut body = sample_chain();
    body["steps"] = serde_json::json!([{ "goal": "just one step now" }]);
    let resp = app
        .oneshot(json_put(&format!("/api/schedule-chains/{id}"), body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let steps = json["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["goal"], "just one step now");
}

#[tokio::test]
async fn update_unknown_chain_returns_404() {
    let app = test_app_chains_started().await;
    let resp = app
        .oneshot(json_put("/api/schedule-chains/nope", sample_chain()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn chain_enable_and_disable_toggle_flag() {
    let app = test_app_chains_started().await;
    let id = create_one_chain(&app).await;
    let disable = app
        .clone()
        .oneshot(json_post(
            &format!("/api/schedule-chains/{id}/disable"),
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(disable.status(), StatusCode::OK);
    assert_eq!(body_json(disable).await["enabled"], false);

    let enable = app
        .oneshot(json_post(
            &format!("/api/schedule-chains/{id}/enable"),
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(enable.status(), StatusCode::OK);
    assert_eq!(body_json(enable).await["enabled"], true);
}

#[tokio::test]
async fn delete_chain_then_get_returns_404() {
    let app = test_app_chains_started().await;
    let id = create_one_chain(&app).await;
    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/schedule-chains/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::OK);
    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/schedule-chains/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn run_now_returns_202_and_submits_only_step_zero() {
    let app = test_app_chains_started().await;
    let id = create_one_chain(&app).await;
    let resp = app
        .clone()
        .oneshot(json_post(
            &format!("/api/schedule-chains/{id}/run-now"),
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let json = body_json(resp).await;
    assert_eq!(json["chain_id"], id);
    assert_eq!(json["queued"], true);

    // The fire should now appear in the chain's run history, parked at step 0
    // — the other two steps haven't been touched (this test app's pool never
    // dispatches, so step 0 never reaches a terminal state).
    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/schedule-chains/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(get).await;
    let runs = json["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["current_step"], 0);
    assert!(runs[0]["current_task_id"].is_string());
    assert_eq!(runs[0]["status"], "running");
}

#[tokio::test]
async fn chain_run_now_unknown_returns_404() {
    let app = test_app_chains_started().await;
    let resp = app
        .oneshot(json_post(
            "/api/schedule-chains/nope/run-now",
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
