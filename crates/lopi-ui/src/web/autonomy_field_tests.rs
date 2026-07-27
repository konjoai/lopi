// ─── Web-composer loop.toml sprint: `autonomy_level` / `no_progress_limit` /
// `isolation` field exposure (web task-create surface) ──────────────────────
// Split out of `task_field_tests.rs` purely to keep that file under the
// 500-line CI file-size gate; no behavioral difference from being inline.

#[test]
fn apply_loop_fields_threads_autonomy_level_no_progress_limit_and_isolation_through() {
    let mut task = Task::new("web composer task");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "web composer task",
        "autonomy_level": "draft_pr",
        "no_progress_limit": 5,
        "isolation": "worktree",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(
        task.autonomy_level,
        Some(lopi_core::loop_config::AutonomyLevel::DraftPr)
    );
    assert_eq!(task.no_progress_limit, Some(5));
    assert_eq!(
        task.isolation,
        Some(lopi_core::loop_config::IsolationMode::Worktree)
    );
}

#[test]
fn apply_loop_fields_omitting_autonomy_level_no_progress_limit_and_isolation_leaves_them_none() {
    // `None` here is not "L2 draft PR" — it's "unset," so the repo's
    // `.lopi/loop.toml` values govern instead (file = base, this field is
    // only ever an override). A composer that left autonomy untouched must
    // never clobber a repo's tuned autonomy_level/no_progress_limit/
    // isolation with a hardcoded default.
    let mut task = Task::new("no overrides here");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "no overrides here",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(task.autonomy_level, None);
    assert_eq!(task.no_progress_limit, None);
    assert_eq!(task.isolation, None);
}

#[tokio::test]
async fn create_task_with_autonomy_level_no_progress_limit_and_isolation_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "web composer task",
        "autonomy_level": "verified_pr",
        "no_progress_limit": 4,
        "isolation": "worktree",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "the wire format for all three fields must actually deserialize"
    );
}

#[tokio::test]
async fn create_task_rejects_unknown_autonomy_level() {
    // Unlike `permission_mode`/`report` (custom parsers threaded through
    // `apply_loop_fields`, rejected with a 422 built by hand),
    // `autonomy_level` deserializes straight into the real `AutonomyLevel`
    // enum — an unrecognized variant never reaches the handler at all; the
    // `Json` extractor's own rejection fires first, and this app maps that
    // to 422 as well. Either way, an invalid value is rejected at the
    // boundary, never silently coerced to a default autonomy.
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "bad autonomy",
        "autonomy_level": "l5-nonsense",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
