// End-to-end HTTP tests for the Loop Engineering surface — the read snapshot
// and the self-prompting strategy write path. Each test roots the app at a
// fresh temp repo so the loop-as-code write (`.lopi/loop.toml`) is isolated.

/// A unique temp repo dir per test name.
fn loop_temp_repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lopi_loop_e2e_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn get_json(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn post_json(app: &Router, uri: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn loop_engineering_snapshot_carries_self_prompt_catalog() {
    let repo = loop_temp_repo("snapshot");
    let app = test_app_with_repo(repo).await;

    let (status, json) = get_json(&app, "/api/loop-engineering").await;
    assert_eq!(status, StatusCode::OK);

    let strategies = json["self_prompt_strategies"].as_array().unwrap();
    assert_eq!(strategies.len(), 4, "S1–S4 strategy catalog");
    assert_eq!(strategies[0]["value"], "direct");
    assert!(
        strategies[1]["preview"]
            .as_str()
            .unwrap()
            .contains("test_pass_rate"),
        "each strategy carries a self-prompt preview"
    );
    // No file yet ⇒ the conservative default is reported.
    assert_eq!(json["config"]["self_prompt"], "direct");
    assert_eq!(json["config"]["self_prompt_tag"], "S1");
}

#[tokio::test]
async fn set_strategy_persists_loop_as_code_and_round_trips() {
    let repo = loop_temp_repo("set_strategy");
    let app = test_app_with_repo(repo.clone()).await;

    // Write the strategy via the API.
    let (status, json) = post_json(
        &app,
        "/api/loop-engineering/strategy",
        serde_json::json!({ "strategy": "reflexion" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["self_prompt"], "reflexion");
    assert_eq!(json["self_prompt_tag"], "S2");

    // Loop-as-code: the file is written to disk and contains the new strategy.
    let toml_path = repo.join(".lopi/loop.toml");
    assert!(toml_path.exists(), "strategy is persisted to .lopi/loop.toml");
    let written = std::fs::read_to_string(&toml_path).unwrap();
    assert!(written.contains("reflexion"), "toml carries the strategy");

    // The next snapshot reflects the persisted strategy.
    let (status, snap) = get_json(&app, "/api/loop-engineering").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snap["config"]["self_prompt"], "reflexion");
    assert_eq!(snap["config"]["self_prompt_tag"], "S2");
}

#[tokio::test]
async fn snapshot_carries_escalation_ladder_and_flag() {
    let repo = loop_temp_repo("escalation_snapshot");
    let app = test_app_with_repo(repo).await;

    let (status, json) = get_json(&app, "/api/loop-engineering").await;
    assert_eq!(status, StatusCode::OK);
    // Default: escalation off, ladder present and climbing from S1.
    assert_eq!(json["config"]["escalate_strategy"], false);
    let ladder = json["config"]["escalation_ladder"].as_array().unwrap();
    assert_eq!(ladder.len(), 4);
    assert_eq!(ladder[0]["tag"], "S1");
    assert_eq!(ladder[3]["tag"], "S4");
}

#[tokio::test]
async fn set_escalation_persists_and_round_trips() {
    let repo = loop_temp_repo("set_escalation");
    let app = test_app_with_repo(repo.clone()).await;

    let (status, json) = post_json(
        &app,
        "/api/loop-engineering/escalation",
        serde_json::json!({ "enabled": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["escalate_strategy"], true);

    let written = std::fs::read_to_string(repo.join(".lopi/loop.toml")).unwrap();
    assert!(written.contains("escalate_strategy = true"));

    let (_, snap) = get_json(&app, "/api/loop-engineering").await;
    assert_eq!(snap["config"]["escalate_strategy"], true);
}

#[tokio::test]
async fn set_strategy_rejects_unknown_tag() {
    let repo = loop_temp_repo("bad_strategy");
    let app = test_app_with_repo(repo.clone()).await;

    let (status, json) = post_json(
        &app,
        "/api/loop-engineering/strategy",
        serde_json::json!({ "strategy": "nonsense" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(json["error"].as_str().unwrap().contains("nonsense"));
    // A rejected write must not create the artifact.
    assert!(!repo.join(".lopi/loop.toml").exists());
}
