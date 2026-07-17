#[tokio::test]
async fn create_task_with_all_options_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "comprehensive task",
        "priority": "critical",
        "constraints": ["no new deps", "keep async"],
        "allowed_dirs": ["src/"],
        "forbidden_dirs": ["vendor/"],
        "max_retries": 5,
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

// ─── Backend-1: client_ref — stable task identity for a stack card ───────────

#[tokio::test]
async fn create_task_client_ref_survives_round_trip_through_the_store() {
    let (app, _store) = test_app_with_store().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "stack card round-trip",
        "client_ref": "card-abc123",
    }))
    .unwrap();
    let resp = send_req(app.clone(), "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["client_ref"], "card-abc123",
        "the create response echoes client_ref back verbatim"
    );
    let id = json["id"].as_str().unwrap().to_string();
    assert!(
        !id.is_empty(),
        "a stable task id must be assigned synchronously at create time"
    );

    // Round-trip: fetch the task back and confirm both the id and the
    // client_ref survived the write to SQLite, not just the in-memory path.
    let get_resp = get_req(app, &format!("/api/tasks/{id}")).await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_bytes = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_json: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert_eq!(
        get_json["id"], id,
        "the same stable id round-trips through the store"
    );
    assert_eq!(
        get_json["client_ref"], "card-abc123",
        "client_ref round-trips through the store, not just the create response"
    );
}

#[tokio::test]
async fn create_task_without_client_ref_omits_it_on_round_trip() {
    let (app, _store) = test_app_with_store().await;
    let body = serde_json::to_string(&serde_json::json!({"goal": "no client ref here"})).unwrap();
    let resp = send_req(app.clone(), "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["client_ref"].is_null(),
        "omitted client_ref must not be fabricated"
    );
    let id = json["id"].as_str().unwrap().to_string();

    let get_resp = get_req(app, &format!("/api/tasks/{id}")).await;
    let get_bytes = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_json: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert!(
        get_json["client_ref"].is_null(),
        "a task created without client_ref stays NULL through the store, not e.g. an empty string"
    );
}

