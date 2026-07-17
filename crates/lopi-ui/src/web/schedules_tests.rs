// Schedule (cron) + config/version endpoint tests. Included from tests.rs so
// these share the `test_app` helpers and the `super::*` import.

/// Build an app whose `ScheduleManager` has been started, so register/run-now
/// exercise the live scheduler path rather than the error-swallowing fallback.
async fn test_app_started() -> Router {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(1, PathBuf::from("."), queue.clone(), bus.clone()).with_store(store.clone()),
    );
    let state = AppState::new(store, bus, queue, pool, None);
    state.schedules.start().await.unwrap();
    build_app(state)
}

fn json_post(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn json_put(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn sample_schedule() -> serde_json::Value {
    serde_json::json!({
        "name": "nightly checks",
        "cron": "0 2 * * *",
        "goal": "run the full test suite",
        "priority": "high",
        "allowed_dirs": ["src/"],
    })
}

/// Create a schedule and return its id.
async fn create_one(app: &Router) -> String {
    let resp = app
        .clone()
        .oneshot(json_post("/api/schedules", sample_schedule()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn create_schedule_returns_201_with_next_runs() {
    let app = test_app_started().await;
    let resp = app
        .clone()
        .oneshot(json_post("/api/schedules", sample_schedule()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "nightly checks");
    assert_eq!(json["enabled"], true);
    assert!(json["next_runs"].as_array().unwrap().len() == 3);
}

#[tokio::test]
async fn create_schedule_rejects_invalid_cron() {
    let app = test_app_started().await;
    let mut body = sample_schedule();
    body["cron"] = serde_json::json!("not a cron");
    let resp = app.oneshot(json_post("/api/schedules", body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_schedule_rejects_empty_goal() {
    let app = test_app_started().await;
    let mut body = sample_schedule();
    body["goal"] = serde_json::json!("   ");
    let resp = app.oneshot(json_post("/api/schedules", body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn list_schedules_includes_created() {
    let app = test_app_started().await;
    let id = create_one(&app).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/schedules")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let arr = json["schedules"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], id);
}

#[tokio::test]
async fn get_schedule_unknown_returns_404() {
    let app = test_app_started().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/schedules/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_schedule_includes_run_history_field() {
    let app = test_app_started().await;
    let id = create_one(&app).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/schedules/{id}"))
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
async fn update_schedule_changes_cron() {
    let app = test_app_started().await;
    let id = create_one(&app).await;
    let mut body = sample_schedule();
    body["cron"] = serde_json::json!("0 5 * * *");
    let resp = app
        .oneshot(json_put(&format!("/api/schedules/{id}"), body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["cron"], "0 5 * * *");
    assert_eq!(json["id"], id);
}

#[tokio::test]
async fn update_unknown_schedule_returns_404() {
    let app = test_app_started().await;
    let resp = app
        .oneshot(json_put("/api/schedules/nope", sample_schedule()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn enable_and_disable_toggle_flag() {
    let app = test_app_started().await;
    let id = create_one(&app).await;
    let disable = app
        .clone()
        .oneshot(json_post(&format!("/api/schedules/{id}/disable"), serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(disable.status(), StatusCode::OK);
    assert_eq!(body_json(disable).await["enabled"], false);

    let enable = app
        .oneshot(json_post(&format!("/api/schedules/{id}/enable"), serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(enable.status(), StatusCode::OK);
    assert_eq!(body_json(enable).await["enabled"], true);
}

#[tokio::test]
async fn delete_schedule_then_get_returns_404() {
    let app = test_app_started().await;
    let id = create_one(&app).await;
    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/schedules/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::OK);
    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/schedules/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn run_now_returns_202_and_records_history() {
    let app = test_app_started().await;
    let id = create_one(&app).await;
    let resp = app
        .clone()
        .oneshot(json_post(&format!("/api/schedules/{id}/run-now"), serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let json = body_json(resp).await;
    assert_eq!(json["schedule_id"], id);
    assert_eq!(json["queued"], true);

    // The fire should now appear in the schedule's run history.
    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/schedules/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(get).await;
    assert_eq!(json["runs"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn run_now_unknown_returns_404() {
    let app = test_app_started().await;
    let resp = app
        .oneshot(json_post("/api/schedules/nope/run-now", serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// `POST /api/schedules/:id/autonomy` (`set_autonomy`) had zero test coverage
// at any level — confirmed by grep before writing these.
#[tokio::test]
async fn set_autonomy_returns_200_with_the_normalized_level() {
    let app = test_app_started().await;
    let id = create_one(&app).await;
    let resp = app
        .clone()
        .oneshot(json_post(
            &format!("/api/schedules/{id}/autonomy"),
            serde_json::json!({ "level": "verified_pr" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["id"], id);
    assert_eq!(json["autonomy_level"], "verified_pr");

    // Round-trip: the change is a real write, not just an echoed response.
    let get_resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/schedules/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["autonomy_level"], "verified_pr");
}

#[tokio::test]
async fn set_autonomy_normalizes_an_unrecognized_level_to_draft_pr() {
    let app = test_app_started().await;
    let id = create_one(&app).await;
    let resp = app
        .oneshot(json_post(
            &format!("/api/schedules/{id}/autonomy"),
            serde_json::json!({ "level": "not-a-real-level" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(
        json["autonomy_level"], "draft_pr",
        "an unrecognized level falls back to AutonomyLevel's #[default] (DraftPr), not an error"
    );
}

#[tokio::test]
async fn set_autonomy_unknown_schedule_returns_404() {
    let app = test_app_started().await;
    let resp = app
        .oneshot(json_post(
            "/api/schedules/nope/autonomy",
            serde_json::json!({ "level": "draft_pr" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn config_endpoint_returns_200() {
    // No lopi.toml in the test working dir → source "none", config null.
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert!(json.get("source").is_some());
}

#[tokio::test]
async fn version_endpoint_reports_service_and_version() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/version")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["service"], "lopi");
    assert!(json["version"].as_str().unwrap().len() >= 5);
    assert!(json.get("uptime_secs").is_some());
}
