// ─── Egress-Allowlist-1: `provenance` field exposure (web task-read surface) ──
// Split out of `task_field_tests.rs` purely to keep that file under the
// 500-line CI file-size gate; no behavioral difference from being inline.

/// Egress-Allowlist-1, Phase 2 — `get_task`'s response must surface the
/// operator-vs-untrusted provenance marker `TaskRow::provenance()` derives
/// from `task.source`, distinguishing an operator-initiated run (`Cli`,
/// the default) from an untrusted-origin one (a simulated webhook task).
#[tokio::test]
async fn get_task_surfaces_provenance_marker() {
    let (app, store) = test_app_with_store().await;

    let operator_task = Task::new("run from the CLI");
    store.save_task(&operator_task, "queued").await.unwrap();

    let mut untrusted_task = Task::new("run from a CI webhook");
    untrusted_task.source = lopi_core::TaskSource::Webhook {
        repo: "konjoai/lopi".into(),
        event: "check_run".into(),
    };
    store.save_task(&untrusted_task, "queued").await.unwrap();

    let operator_resp = get_req(app.clone(), &format!("/api/tasks/{}", operator_task.id.0)).await;
    let operator_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(operator_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(operator_json["provenance"], "operator");

    let untrusted_resp = get_req(app, &format!("/api/tasks/{}", untrusted_task.id.0)).await;
    let untrusted_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(untrusted_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(untrusted_json["provenance"], "untrusted");
}
