// Sprint demo-measurement — synthetic-store mutation refusals and
// `measurement_provenance`/`synthetic` fields. See
// `docs/adr/0001-demo-mode-and-measurement.md`.

/// Mark `store` as a synthetic (`lopi demo`) store — the same
/// `store_metadata` write the fixture generator performs.
async fn mark_synthetic(store: &lopi_memory::MemoryStore) {
    store.set_metadata("synthetic", "true").await.unwrap();
}

#[tokio::test]
async fn create_task_is_refused_on_a_synthetic_store() {
    let (app, store) = test_app_with_store().await;
    mark_synthetic(&store).await;
    let body = serde_json::to_string(&serde_json::json!({ "goal": "should be refused" })).unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let json = json_body(resp).await;
    assert_eq!(json["synthetic"], true);
    assert!(json["error"].as_str().unwrap().contains("read-only"));
}

/// Regression: a non-synthetic store's `create_task` is untouched.
#[tokio::test]
async fn create_task_still_works_on_a_non_synthetic_store() {
    let (app, _store) = test_app_with_store().await;
    let body = serde_json::to_string(&serde_json::json!({ "goal": "a real task" })).unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn cancel_task_is_refused_on_a_synthetic_store() {
    let (app, store) = test_app_with_store().await;
    mark_synthetic(&store).await;
    let resp = send_req(
        app,
        "DELETE",
        "/api/tasks/00000000-0000-0000-0000-000000000000",
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let json = json_body(resp).await;
    assert_eq!(json["synthetic"], true);
}

/// Regression: a non-synthetic store's `cancel_task` still reaches the real
/// lookup path (404 for an unknown id, not a refusal).
#[tokio::test]
async fn cancel_task_still_works_on_a_non_synthetic_store() {
    let (app, _store) = test_app_with_store().await;
    let resp = send_req(
        app,
        "DELETE",
        "/api/tasks/00000000-0000-0000-0000-000000000000",
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn approve_plan_is_refused_on_a_synthetic_store() {
    let (app, store) = test_app_with_store().await;
    mark_synthetic(&store).await;
    let resp = send_req(
        app,
        "POST",
        "/api/tasks/00000000-0000-0000-0000-000000000000/plan/approve",
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn reject_plan_is_refused_on_a_synthetic_store() {
    let (app, store) = test_app_with_store().await;
    mark_synthetic(&store).await;
    let resp = send_req(
        app,
        "POST",
        "/api/tasks/00000000-0000-0000-0000-000000000000/plan/reject",
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn stats_reports_synthetic_true_and_measured_provenance_on_a_demo_store() {
    let (app, store) = test_app_with_store().await;
    mark_synthetic(&store).await;
    let resp = get_req(app, "/api/stats").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert_eq!(json["synthetic"], true);
    assert_eq!(json["measurement_provenance"]["kind"], "measured");
    assert!(json["measurement_provenance"]["source"]
        .as_str()
        .unwrap()
        .contains("turn_metrics"));
}

/// Regression: a non-synthetic store reports `synthetic: false`, and every
/// pre-existing `/api/stats` field is untouched.
#[tokio::test]
async fn stats_reports_synthetic_false_on_a_real_store() {
    let app = test_app().await;
    let resp = get_req(app, "/api/stats").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert_eq!(json["synthetic"], false);
    assert_eq!(json["measurement_provenance"]["kind"], "measured");
    assert!(json.get("running").is_some());
    assert!(json.get("queued").is_some());
    assert!(json.get("total_cost_usd_today").is_some());
}

#[tokio::test]
async fn budget_breakdown_carries_measurement_provenance() {
    let app = test_app().await;
    let resp = get_req(app, "/api/budget/breakdown").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert_eq!(json["measurement_provenance"]["kind"], "measured");
    assert!(json["measurement_provenance"]["source"]
        .as_str()
        .unwrap()
        .contains("turn_metrics"));
    // Pre-existing fields are untouched.
    assert!(json.get("by_model").is_some());
    assert!(json.get("trend").is_some());
}
