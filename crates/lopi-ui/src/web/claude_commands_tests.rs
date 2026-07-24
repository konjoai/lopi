// `GET /api/claude-commands` — Composer-Grammar-2's discovery endpoint.
// The discovery logic itself (`lopi_skill::discover_claude_commands`) is
// unit-tested exhaustively in `lopi-skill`; these confirm only the HTTP
// wiring — repo-query resolution, the `?repo=` fallback to the server's
// primary repo, and the response shape.
//
// The endpoint merges in Claude Code's built-ins and the *real* `$HOME`'s
// commands/skills/plugins (see `repos_handlers::list_claude_commands`), so
// the response is not hermetic — it can carry entries this test process
// never wrote, depending on whatever the machine running the test happens
// to have under its home directory. Assertions below only ever check that
// the repo-specific fixture is (or isn't) present, never the total count.

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
    let foo = commands
        .iter()
        .find(|c| c["name"] == "foo")
        .expect("the repo-scoped command is present in the response");
    assert_eq!(foo["hint"], "does foo");
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
    assert!(
        commands.iter().any(|c| c["name"] == "primary"),
        "the server's primary repo is scanned when `?repo=` is empty"
    );
}

#[tokio::test]
async fn claude_commands_repo_with_neither_dir_still_returns_builtins() {
    let tmp = tempfile::tempdir().unwrap();
    let app = test_app().await;
    let resp = get_req(
        app,
        &format!("/api/claude-commands?repo={}", tmp.path().display()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    let commands = json["commands"].as_array().unwrap();
    assert!(
        commands.iter().any(|c| c["name"] == "help"),
        "a repo with no .claude dir still surfaces Claude Code's own built-ins"
    );
    assert!(
        !commands.iter().any(|c| c["name"] == "nonexistent-repo-command"),
        "nothing from an unrelated repo leaks in"
    );
}
