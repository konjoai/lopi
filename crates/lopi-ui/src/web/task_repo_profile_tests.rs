// Regression tests for the Collision-Oracle-Build sprint's Part C parity fix:
// `POST /api/tasks` now applies the repo's `.lopi.toml` profile the same way
// every CLI entry point already does (`task_build.rs::build_task_from_fields`).

/// A `test_app_with_store`-shaped app, but bound to a caller-supplied repo
/// path instead of `PathBuf::from(".")`, so a `.lopi.toml` placed there is
/// the one `RepoProfile::load_from_repo` actually reads. No dispatch loop
/// runs, so the submitted `Task` sits in `queue` for the test to pop and
/// inspect directly — the response body only carries id/goal/queued/
/// duplicate_of/client_ref, not `allowed_dirs`.
async fn test_app_with_store_and_repo(repo: PathBuf) -> (Router, TaskQueue) {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(1, repo.clone(), queue.clone(), bus.clone()).with_store(store.clone()),
    );
    let state = AppState::new_with_repo(store, bus, queue.clone(), pool, None, repo);
    (build_app(state), queue)
}

#[tokio::test]
async fn create_task_applies_the_bound_repo_profile_when_no_repo_is_given_in_the_request() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join(".lopi.toml"),
        "allowed_dirs = [\"src/only\"]\n",
    )
    .unwrap();
    let (app, queue) = test_app_with_store_and_repo(dir.path().to_path_buf()).await;

    let body =
        serde_json::to_string(&serde_json::json!({ "goal": "no repo field in the request" }))
            .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let task = queue.pop().await;
    assert_eq!(
        task.allowed_dirs,
        vec!["src/only".to_string()],
        "the server's bound repo profile must apply even when the request omits `repo`"
    );
}

#[tokio::test]
async fn create_task_applies_the_request_repo_profile_over_the_bound_default() {
    let bound_repo = tempfile::TempDir::new().unwrap();
    let request_repo = tempfile::TempDir::new().unwrap();
    std::fs::write(
        request_repo.path().join(".lopi.toml"),
        "allowed_dirs = [\"request-repo-only\"]\n",
    )
    .unwrap();
    let (app, queue) = test_app_with_store_and_repo(bound_repo.path().to_path_buf()).await;

    let body = serde_json::to_string(&serde_json::json!({
        "goal": "explicit repo in the request",
        "repo": request_repo.path().to_string_lossy(),
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let task = queue.pop().await;
    assert_eq!(
        task.allowed_dirs,
        vec!["request-repo-only".to_string()],
        "an explicit request-level repo's profile wins over the server's bound repo"
    );
}
