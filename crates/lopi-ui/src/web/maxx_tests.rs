// /api/maxx CRUD tests. Included from tests.rs so these share the
// `test_app_started`-style helpers and the `super::*` import.

fn sample_maxx() -> serde_json::Value {
    serde_json::json!({
        "name": "overnight backlog",
        "goal": "work through the low-priority backlog",
        "priority": "low",
        "quiet_hours": [23, 7],
        "headroom_gate": true,
        "windows": ["five_hour", "seven_day"],
    })
}

async fn create_one_maxx(app: &Router) -> String {
    let resp = app
        .clone()
        .oneshot(json_post("/api/maxx", sample_maxx()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn create_maxx_returns_201_with_fields() {
    let app = test_app_started().await;
    let resp = app.oneshot(json_post("/api/maxx", sample_maxx())).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "overnight backlog");
    assert_eq!(json["enabled"], true);
    assert_eq!(json["headroom_gate"], true);
    assert_eq!(json["quiet_hours"], serde_json::json!([23, 7]));
    assert_eq!(
        json["windows"],
        serde_json::json!(["five_hour", "seven_day"])
    );
}

#[tokio::test]
async fn create_maxx_rejects_empty_name() {
    let app = test_app_started().await;
    let mut body = sample_maxx();
    body["name"] = serde_json::json!("   ");
    let resp = app.oneshot(json_post("/api/maxx", body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_maxx_rejects_empty_goal() {
    let app = test_app_started().await;
    let mut body = sample_maxx();
    body["goal"] = serde_json::json!("");
    let resp = app.oneshot(json_post("/api/maxx", body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_maxx_rejects_unknown_window() {
    let app = test_app_started().await;
    let mut body = sample_maxx();
    body["windows"] = serde_json::json!(["one_hour"]);
    let resp = app.oneshot(json_post("/api/maxx", body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_maxx_rejects_out_of_range_quiet_hours() {
    let app = test_app_started().await;
    let mut body = sample_maxx();
    body["quiet_hours"] = serde_json::json!([23, 24]);
    let resp = app.oneshot(json_post("/api/maxx", body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn list_maxx_includes_created() {
    let app = test_app_started().await;
    create_one_maxx(&app).await;
    let resp = app
        .oneshot(Request::builder().uri("/api/maxx").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["maxx"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn get_maxx_includes_run_history() {
    let app = test_app_started().await;
    let id = create_one_maxx(&app).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/maxx/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["id"], id);
    assert!(json["runs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_maxx_404_for_unknown_id() {
    let app = test_app_started().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/maxx/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_maxx_edits_in_place() {
    let app = test_app_started().await;
    let id = create_one_maxx(&app).await;
    let mut body = sample_maxx();
    body["name"] = serde_json::json!("renamed");
    body["headroom_gate"] = serde_json::json!(false);
    let resp = app
        .oneshot(json_put(&format!("/api/maxx/{id}"), body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["id"], id);
    assert_eq!(json["name"], "renamed");
    assert_eq!(json["headroom_gate"], false);
}

#[tokio::test]
async fn delete_maxx_removes_entry() {
    let app = test_app_started().await;
    let id = create_one_maxx(&app).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/maxx/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/maxx/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn enable_disable_maxx_toggles_flag() {
    let app = test_app_started().await;
    let id = create_one_maxx(&app).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/maxx/{id}/disable"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["enabled"], false);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/maxx/{id}/enable"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["enabled"], true);
}

#[tokio::test]
async fn enable_maxx_404_for_unknown_id() {
    let app = test_app_started().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/maxx/does-not-exist/enable")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
