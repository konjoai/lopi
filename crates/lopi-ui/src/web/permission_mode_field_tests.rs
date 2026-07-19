// ─── Permission-Modes-1: `permission_mode` field exposure (web task-create surface) ──
// Split out of `task_field_tests.rs` purely to keep that file under the
// 500-line CI file-size gate; no behavioral difference from being inline.

#[test]
fn apply_loop_fields_threads_permission_mode_through() {
    let mut task = Task::new("locked-down task");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "locked-down task",
        "permission_mode": "dontAsk",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(task.permission_mode, lopi_core::PermissionMode::DontAsk);
}

#[test]
fn apply_loop_fields_omitting_permission_mode_keeps_the_bypass_default() {
    let mut task = Task::new("plain task");
    let req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "plain task"})).unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(
        task.permission_mode,
        lopi_core::PermissionMode::BypassPermissions,
        "an omitted permission_mode must reproduce the pre-existing unconditional behavior"
    );
}

#[test]
fn apply_loop_fields_rejects_unrecognized_permission_mode() {
    let mut task = Task::new("t");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "t",
        "permission_mode": "plan",
    }))
    .unwrap();
    let err = apply_loop_fields(&mut task, &req).unwrap_err();
    assert_eq!(
        err,
        crate::web::handlers::ApplyLoopFieldsError::PermissionMode(
            lopi_core::PermissionModeError::Unknown("plan".to_string())
        )
    );
    assert_eq!(
        task.permission_mode,
        lopi_core::PermissionMode::BypassPermissions,
        "task must not be mutated on a rejected permission mode"
    );
}

#[tokio::test]
async fn create_task_rejects_unrecognized_permission_mode() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "locked-down task",
        "permission_mode": "plan",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("plan"),
        "must surface the typed PermissionModeError, not a generic message"
    );
}

#[tokio::test]
async fn create_task_with_permission_mode_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "auto-reviewed task",
        "permission_mode": "auto",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}
