//! Tests for `repos_handlers.rs`, split out to keep that file under the
//! 500-line CI file-size gate (same precedent as `tui.rs`/`tui_tests.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use lopi_core::{AgentEvent, EventBus};
use lopi_memory::{DemoRepoRow, MemoryStore};
use lopi_orchestrator::{AgentPool, TaskQueue};
use std::sync::Arc;

/// `set_current_dir` is process-global, and `cargo test` runs tests as
/// threads within one process — the cwd-dependent cases must not overlap.
static CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// An `AppState` over a fresh in-memory store, optionally rooted at
/// `repo` (falls back to `"."`).
async fn state_with_repo(repo: PathBuf) -> AppState {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(8);
    let queue = TaskQueue::new();
    let pool = Arc::new(AgentPool::new(1, repo.clone(), queue.clone(), bus.clone()));
    AppState::new_with_repo(store, bus, queue, pool, None, repo)
}

/// An `AppState` over a store marked `synthetic=true` (`lopi demo`'s
/// marker) — every demo-guarded handler must short-circuit on this
/// without touching the real filesystem.
async fn synthetic_state() -> AppState {
    let state = state_with_repo(PathBuf::from(".")).await;
    state.store.set_metadata("synthetic", "true").await.unwrap();
    state
}

#[tokio::test]
async fn repos_json_returns_demo_repos_when_synthetic() {
    let state = synthetic_state().await;
    state
        .store
        .insert_demo_repo(&DemoRepoRow {
            name: "aurora-api".into(),
            stack: "Rust service".into(),
            path: "/demo/repos/aurora-api".into(),
            description: "a synthetic repo".into(),
            sort_order: 0,
        })
        .await
        .unwrap();

    let json = repos_json(&state).await;
    let repos = json["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 1, "demo repos are returned, not an empty scan");
    assert_eq!(repos[0]["owner"], "demo");
    assert_eq!(repos[0]["name"], "aurora-api");
    assert_eq!(repos[0]["path"], "/demo/repos/aurora-api");
}

/// Regression: a non-synthetic store must still scan the real
/// filesystem, exactly as before this sprint.
#[tokio::test]
async fn repos_json_scans_the_filesystem_when_not_synthetic() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = git_repo(tmp.path(), "aurora-api");
    let state = state_with_repo(PathBuf::from(&repo)).await;

    let json = repos_json(&state).await;
    let repos = json["repos"].as_array().unwrap();
    assert!(
        repos.iter().any(|r| r["path"] == repo),
        "the real repo is discovered via scan, not demo data"
    );
}

#[tokio::test]
async fn branches_json_returns_fixed_result_when_synthetic() {
    let state = synthetic_state().await;
    let json = branches_json(&state, "").await;
    assert_eq!(
        json,
        serde_json::json!({ "branches": ["main"], "default": "main" })
    );
}

/// Regression: a non-synthetic store must still enumerate real branches
/// via `git`, exactly as before this sprint.
#[tokio::test]
async fn branches_json_uses_real_git_when_not_synthetic() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with_branches(&tmp.path().join("r"), "main", &["main", "feat/x"]);
    let state = state_with_repo(PathBuf::from(&repo)).await;

    let json = branches_json(&state, "").await;
    let branches = json["branches"].as_array().unwrap();
    assert!(
        branches.iter().any(|b| b == "feat/x"),
        "real branches are enumerated, not the fixed synthetic list"
    );
}

#[tokio::test]
async fn list_claude_commands_returns_empty_when_synthetic() {
    let state = synthetic_state().await;
    let resp = list_claude_commands(
        State(state),
        Query(ClaudeCommandsQuery {
            repo: String::new(),
        }),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json, serde_json::json!({ "commands": [] }));
}

/// Regression: a non-synthetic store must still return `200` and reach
/// the real discovery path (no early short-circuit), exactly as before
/// this sprint.
#[tokio::test]
async fn list_claude_commands_returns_200_when_not_synthetic() {
    let tmp = tempfile::tempdir().unwrap();
    let state = state_with_repo(tmp.path().to_path_buf()).await;
    let resp = list_claude_commands(
        State(state),
        Query(ClaudeCommandsQuery {
            repo: String::new(),
        }),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Create `root/name` with a `.git` inside, and return its resolved path.
fn git_repo(root: &Path, name: &str) -> String {
    let p = root.join(name);
    std::fs::create_dir_all(p.join(".git")).unwrap();
    p.canonicalize().unwrap().display().to_string()
}

/// The regression: `sail --repo` defaults to a relative `"."`, whose
/// `parent()` is the empty path. Sibling discovery used to `read_dir("")`,
/// fail silently, and offer only the primary repo.
#[test]
fn relative_primary_discovers_siblings() {
    let guard = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let a = git_repo(root, "repo-a");
    let b = git_repo(root, "repo-b");
    std::fs::create_dir_all(root.join("not-a-repo")).unwrap();

    let restore = std::env::current_dir().unwrap();
    std::env::set_current_dir(root.join("repo-a")).unwrap();
    let got = scan_repos(Path::new("."), &[]);
    std::env::set_current_dir(restore).unwrap();
    drop(guard);

    assert_eq!(got, vec![a, b], "siblings discovered, non-repo excluded");
}

#[test]
fn extras_are_included_and_deduped_against_siblings() {
    let tmp = tempfile::tempdir().unwrap();
    let a = git_repo(tmp.path(), "repo-a");
    let b = git_repo(tmp.path(), "repo-b");
    // A dispatch target living nowhere near the primary.
    let far = tempfile::tempdir().unwrap();
    let f = git_repo(far.path(), "far-repo");

    let extras = vec![
        PathBuf::from(&f),
        PathBuf::from(&b),         // already found as a sibling
        far.path().join("no-git"), // not a repo — dropped
    ];
    let got = scan_repos(&PathBuf::from(&a), &extras);

    assert!(
        got.contains(&f),
        "extra outside the primary's tree is listed"
    );
    assert_eq!(
        got.iter().filter(|r| **r == b).count(),
        1,
        "an extra that is also a sibling appears once"
    );
    assert!(
        !got.iter().any(|r| r.ends_with("no-git")),
        "a non-repo extra is dropped"
    );
}

/// A primary that cannot be resolved must degrade to an empty list, not
/// panic — `absolutize` falls back to the path as-is.
#[test]
fn unresolvable_primary_yields_no_repos() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");
    assert!(scan_repos(&missing, &[]).is_empty());
}

fn git(repo: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?} failed");
}

/// A repo on `head`, carrying every branch in `branches`.
fn repo_with_branches(root: &Path, head: &str, branches: &[&str]) -> String {
    std::fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q", "-b", "base"]);
    git(root, &["config", "user.email", "t@t.t"]);
    git(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("f"), "x").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "init"]);
    for b in branches {
        git(root, &["branch", b]);
    }
    git(root, &["checkout", "-q", head]);
    root.display().to_string()
}

#[test]
fn generated_branches_are_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with_branches(
        &tmp.path().join("r"),
        "main",
        &[
            "main",
            "feat/x",
            "lopi/fe125cc0-63b6-43e4-a273-52f1dc84d1e4-attempt-1",
            "claude/forge-polish-m3",
        ],
    );
    let (branches, default) = git_branches(&repo);

    assert_eq!(
        branches,
        vec!["base", "feat/x", "main"],
        "lopi/* and claude/* are dropped"
    );
    assert_eq!(
        default, "main",
        "HEAD is reported when it survives the filter"
    );
}

/// Regression: `.take(100)` used to cap the branch list with no signal
/// that anything was hidden. Assert the cap still applies (unchanged
/// behavior) now that it's a logged `truncate` — a real repo can easily
/// carry more than 100 local branches, unlike the 500-repo cap.
#[test]
fn truncates_past_max_branches() {
    let tmp = tempfile::tempdir().unwrap();
    let extra: Vec<String> = (0..(MAX_BRANCHES + 5)).map(|i| format!("b{i}")).collect();
    let extra_refs: Vec<&str> = extra.iter().map(String::as_str).collect();
    let repo = repo_with_branches(&tmp.path().join("r"), "base", &extra_refs);
    let (branches, _default) = git_branches(&repo);
    assert_eq!(
        branches.len(),
        MAX_BRANCHES,
        "must cap at MAX_BRANCHES, not return every branch"
    );
}

/// A run can leave the repo checked out on a generated branch. The reported
/// default must still be a branch the dropdown actually offers.
#[test]
fn default_falls_back_when_head_is_generated() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with_branches(
        &tmp.path().join("r"),
        "lopi/abc-attempt-2",
        &["main", "lopi/abc-attempt-2"],
    );
    let (branches, default) = git_branches(&repo);

    assert!(!branches.iter().any(|b| b.starts_with("lopi/")));
    assert_eq!(
        default, "main",
        "a filtered HEAD falls back to main, not itself"
    );
    assert!(
        branches.contains(&default),
        "the default is always selectable"
    );
}

#[test]
fn branch_names_merely_containing_the_prefix_are_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with_branches(
        &tmp.path().join("r"),
        "main",
        &["main", "lopi-ui-refactor", "feat/claude-integration"],
    );
    let (branches, _) = git_branches(&repo);

    assert!(
        branches.contains(&"lopi-ui-refactor".to_string()),
        "only the `lopi/` path prefix is generated, not `lopi-*`"
    );
    assert!(
        branches.contains(&"feat/claude-integration".to_string()),
        "`claude` mid-name is not a `claude/` prefix"
    );
}
