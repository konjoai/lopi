// `GET /api/claude-commands` — Composer-Grammar-2's discovery endpoint.
// The discovery logic itself (`lopi_skill::discover_claude_commands`) is
// unit-tested exhaustively in `lopi-skill`; these confirm only the HTTP
// wiring — repo-query resolution, the `?repo=` fallback to the server's
// primary repo, and the response shape.

#[tokio::test]
async fn claude_commands_finds_a_legacy_command_in_the_query_repo() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".claude/commands")).unwrap();
    std::fs::write(
        tmp.path().join(".claude/commands/foo.md"),
        "---\ndescription: does foo\n---\n\nbody",
    )
    .unwrap();

    let app = test_app().await;
    let resp = get_req(
        app,
        &format!("/api/claude-commands?repo={}", tmp.path().display()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    let commands = json["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["name"], "foo");
    assert_eq!(commands[0]["hint"], "does foo");
}

#[tokio::test]
async fn claude_commands_empty_repo_query_falls_back_to_the_primary_repo() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".claude/commands")).unwrap();
    std::fs::write(
        tmp.path().join(".claude/commands/primary.md"),
        "primary repo command",
    )
    .unwrap();

    let app = test_app_with_repo(tmp.path().to_path_buf()).await;
    let resp = get_req(app, "/api/claude-commands").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    let commands = json["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["name"], "primary");
}

#[tokio::test]
async fn claude_commands_repo_with_neither_dir_returns_an_empty_list() {
    let tmp = tempfile::tempdir().unwrap();
    let app = test_app().await;
    let resp = get_req(
        app,
        &format!("/api/claude-commands?repo={}", tmp.path().display()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    assert_eq!(json["commands"].as_array().unwrap().len(), 0);
}
