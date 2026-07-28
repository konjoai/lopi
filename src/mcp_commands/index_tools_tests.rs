#![allow(clippy::unwrap_used)]
use super::*;
use std::process::Command;

fn init_git_repo(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "t"]);
    std::fs::write(
        dir.join("lib.rs"),
        "pub fn callee() {}\n\npub fn caller() {\n    callee();\n}\n",
    )
    .unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "init"]);
}

async fn seeded_store(dir: &std::path::Path) -> (IndexStore, String) {
    init_git_repo(dir);
    let store = IndexStore::open_in_memory().await.unwrap();
    lopi_index::reindex::reindex(&store, "repo", dir)
        .await
        .unwrap();
    (store, "repo".to_string())
}

#[test]
fn tool_defs_exposes_all_four_index_tools() {
    let names: Vec<String> = tool_defs().into_iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        vec!["lopi_find", "lopi_read", "lopi_refs", "lopi_query"]
    );
}

#[test]
fn kind_names_lists_every_symbol_kind() {
    let names = kind_names();
    assert_eq!(names.len(), 10);
    assert!(names.contains(&"fn"));
    assert!(names.contains(&"struct"));
}

#[test]
fn lang_names_lists_every_language() {
    let names = lang_names();
    assert_eq!(names.len(), 5);
    assert!(names.contains(&"rust"));
}

#[tokio::test]
async fn tool_handler_tools_matches_tool_defs() {
    let store = IndexStore::open_in_memory().await.unwrap();
    let handler = IndexToolHandler::new(
        store,
        std::path::PathBuf::from("."),
        "repo".into(),
        IndexConfig::default(),
    );
    assert_eq!(lopi_mcp::ToolHandler::tools(&handler).len(), 4);
}

#[tokio::test]
async fn call_find_returns_the_seeded_symbol() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let result = call_find(&store, &repo_id, &cfg, &json!({"query": "callee"}))
        .await
        .unwrap();
    assert!(result["total_matches"].as_u64().unwrap() >= 1);
    let items = result["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["qualified_name"] == "callee"));
}

#[tokio::test]
async fn call_read_returns_source_for_qualified_name() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let result = call_read(
        &store,
        dir.path(),
        &repo_id,
        &cfg,
        &json!({"qualified_name": "callee"}),
    )
    .await
    .unwrap();
    assert_eq!(result["path"], "lib.rs");
    assert!(result["text"].as_str().unwrap().contains("fn callee"));
}

#[tokio::test]
async fn call_read_returns_source_for_explicit_span() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let result = call_read(
        &store,
        dir.path(),
        &repo_id,
        &cfg,
        &json!({"path": "lib.rs", "line_start": 1, "line_end": 1}),
    )
    .await
    .unwrap();
    assert_eq!(result["text"], "pub fn callee() {}");
}

#[tokio::test]
async fn call_refs_callers_direction_finds_the_caller() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let result = call_refs(
        &store,
        &repo_id,
        &cfg,
        &json!({"qualified_name": "callee", "direction": "callers"}),
    )
    .await
    .unwrap();
    let items = result["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["qualified_name"] == "caller"));
}

#[tokio::test]
async fn call_refs_callees_direction_finds_the_callee() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let result = call_refs(
        &store,
        &repo_id,
        &cfg,
        &json!({"qualified_name": "caller", "direction": "callees"}),
    )
    .await
    .unwrap();
    let items = result["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["qualified_name"] == "callee"));
}

#[tokio::test]
async fn call_refs_rejects_an_invalid_direction() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let err = call_refs(
        &store,
        &repo_id,
        &cfg,
        &json!({"qualified_name": "callee", "direction": "sideways"}),
    )
    .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn call_query_then_refs_callers_expands_hits() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let result = call_query(
        &store,
        &repo_id,
        &cfg,
        &json!({"find": {"query": "callee"}, "then_refs": {"direction": "callers"}}),
    )
    .await
    .unwrap();
    let items = result["items"].as_array().unwrap();
    assert!(!items.is_empty());
    let refs = items[0]["refs"].as_object().unwrap();
    assert!(refs["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["qualified_name"] == "caller"));
}

#[tokio::test]
async fn call_query_then_refs_callees_expands_hits() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let result = call_query(
        &store,
        &repo_id,
        &cfg,
        &json!({"find": {"query": "caller"}, "then_refs": {"direction": "callees"}}),
    )
    .await
    .unwrap();
    let items = result["items"].as_array().unwrap();
    let refs = items[0]["refs"].as_object().unwrap();
    assert!(refs["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["qualified_name"] == "callee"));
}

#[tokio::test]
async fn call_query_without_then_refs_omits_the_refs_field() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let result = call_query(
        &store,
        &repo_id,
        &cfg,
        &json!({"find": {"query": "callee"}}),
    )
    .await
    .unwrap();
    let items = result["items"].as_array().unwrap();
    assert!(items[0]["refs"].is_null());
}

#[tokio::test]
async fn dispatch_routes_by_tool_name() {
    let dir = tempfile::TempDir::new().unwrap();
    let (store, repo_id) = seeded_store(dir.path()).await;
    let cfg = IndexConfig::default();
    let raw = dispatch(
        &store,
        dir.path(),
        &repo_id,
        &cfg,
        "lopi_find",
        json!({"query": "callee"}),
    )
    .await
    .unwrap();
    let parsed: Value = serde_json::from_str(&raw).unwrap();
    assert!(parsed["total_matches"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn dispatch_rejects_an_unknown_tool_name() {
    let store = IndexStore::open_in_memory().await.unwrap();
    let cfg = IndexConfig::default();
    let err = dispatch(
        &store,
        std::path::Path::new("."),
        "repo",
        &cfg,
        "not_a_real_tool",
        json!({}),
    )
    .await;
    assert!(err.is_err());
}
