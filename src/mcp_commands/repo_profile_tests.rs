//! Regression test for the Collision-Oracle-Build sprint's Part C parity fix:
//! `lopi_submit_task` (MCP) now applies the repo's `.lopi.toml` profile the
//! same way every other entry point already does. Split out of
//! `mod_tests.rs` to keep that file within the 500-line budget.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use tempfile::TempDir;

/// A `test_state`-shaped `AppState`, but bound to a caller-supplied repo
/// path instead of `PathBuf::from(".")`, so a `.lopi.toml` placed there is
/// the one `RepoProfile::load_from_repo` actually reads. No dispatch loop
/// spawned, same as `test_state()` — `state.queue.pop()` hands back the
/// real submitted `Task`.
async fn test_state_with_repo(repo: PathBuf) -> AppState {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(8);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(1, repo.clone(), queue.clone(), bus.clone()).with_store(store.clone()),
    );
    AppState::new_with_repo(store, bus, queue, pool, None, repo)
}

#[tokio::test]
async fn submit_task_applies_the_pool_repo_profile_when_no_repo_is_given_in_the_request() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join(".lopi.toml"),
        "allowed_dirs = [\"src/only\"]\n",
    )
    .unwrap();
    let state = test_state_with_repo(dir.path().to_path_buf()).await;

    submit_task(&state, &json!({ "goal": "no repo field in the request" }))
        .await
        .unwrap();

    let task = state.queue.pop().await;
    assert_eq!(
        task.allowed_dirs,
        vec!["src/only".to_string()],
        "the pool's bound repo profile must apply even when the request omits `repo`"
    );
}

#[tokio::test]
async fn submit_task_applies_the_request_repo_profile_over_the_pool_default() {
    let pool_repo = TempDir::new().unwrap();
    let request_repo = TempDir::new().unwrap();
    std::fs::write(
        request_repo.path().join(".lopi.toml"),
        "allowed_dirs = [\"request-repo-only\"]\n",
    )
    .unwrap();
    let state = test_state_with_repo(pool_repo.path().to_path_buf()).await;

    submit_task(
        &state,
        &json!({
            "goal": "explicit repo in the request",
            "repo": request_repo.path().to_string_lossy(),
        }),
    )
    .await
    .unwrap();

    let task = state.queue.pop().await;
    assert_eq!(
        task.allowed_dirs,
        vec!["request-repo-only".to_string()],
        "an explicit request-level repo's profile wins over the pool's bound repo"
    );
}

#[tokio::test]
async fn submit_task_without_a_lopi_toml_leaves_the_task_default_untouched() {
    let dir = TempDir::new().unwrap();
    let state = test_state_with_repo(dir.path().to_path_buf()).await;

    submit_task(&state, &json!({ "goal": "no profile file here" }))
        .await
        .unwrap();

    let task = state.queue.pop().await;
    assert_eq!(
        task.allowed_dirs,
        vec!["src/".to_string(), "tests/".to_string()],
        "no .lopi.toml means RepoProfile::default(), a no-op apply — Task::new's own default stands"
    );
}
