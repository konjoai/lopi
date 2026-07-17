//! Store tests — schedule chains. Split out to keep the main module
//! under the 500-line file gate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;

fn input(name: &str) -> ScheduleChainInput {
    ScheduleChainInput {
        id: None,
        name: name.into(),
        cron: "0 2 * * *".into(),
        repo: Some("/tmp/repo".into()),
        priority: "high".into(),
        autonomy_level: "verified_pr".into(),
        on_fail: "continue".into(),
        enabled: true,
        steps: vec![
            ChainStepInput {
                goal: "research".into(),
                allowed_dirs: vec!["docs/".into()],
                forbidden_dirs: vec![],
            },
            ChainStepInput {
                goal: "implement".into(),
                allowed_dirs: vec!["src/".into()],
                forbidden_dirs: vec!["infra/".into()],
            },
        ],
    }
}

#[tokio::test]
async fn upsert_then_get_round_trips_steps_in_order() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let row = store
        .upsert_schedule_chain(&input("nightly"))
        .await
        .unwrap();
    let fetched = store.get_schedule_chain(&row.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "nightly");
    assert_eq!(fetched.on_fail, "continue");
    assert_eq!(fetched.steps.len(), 2);
    assert_eq!(fetched.steps[0].goal, "research");
    assert_eq!(fetched.steps[0].step_order, 0);
    assert_eq!(fetched.steps[1].goal, "implement");
    assert_eq!(fetched.steps[1].forbidden_dirs, vec!["infra/".to_string()]);
}

#[tokio::test]
async fn upsert_with_id_replaces_steps_entirely() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let row = store.upsert_schedule_chain(&input("orig")).await.unwrap();
    let mut edit = input("orig");
    edit.id = Some(row.id.clone());
    edit.steps = vec![ChainStepInput {
        goal: "just one step now".into(),
        allowed_dirs: vec![],
        forbidden_dirs: vec![],
    }];
    let updated = store.upsert_schedule_chain(&edit).await.unwrap();
    assert_eq!(updated.steps.len(), 1);
    assert_eq!(updated.steps[0].goal, "just one step now");
}

#[tokio::test]
async fn on_fail_normalizes_unknown_to_stop() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut bad = input("weird");
    bad.on_fail = "explode".into();
    let row = store.upsert_schedule_chain(&bad).await.unwrap();
    assert_eq!(row.on_fail, "stop");
}

#[tokio::test]
async fn set_enabled_toggles_flag() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let row = store.upsert_schedule_chain(&input("toggle")).await.unwrap();
    assert!(store
        .set_schedule_chain_enabled(&row.id, false)
        .await
        .unwrap());
    assert!(
        !store
            .get_schedule_chain(&row.id)
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
    assert!(!store
        .set_schedule_chain_enabled("nope", true)
        .await
        .unwrap());
}

#[tokio::test]
async fn delete_removes_chain_steps_and_runs() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let row = store.upsert_schedule_chain(&input("doomed")).await.unwrap();
    store.start_chain_run(&row.id).await.unwrap();
    assert!(store.delete_schedule_chain(&row.id).await.unwrap());
    assert!(store.get_schedule_chain(&row.id).await.unwrap().is_none());
    assert!(store.list_chain_runs(&row.id, 10).await.unwrap().is_empty());
    assert!(!store.delete_schedule_chain(&row.id).await.unwrap());
}

#[tokio::test]
async fn run_lifecycle_advances_and_finishes() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let chain = store
        .upsert_schedule_chain(&input("run-test"))
        .await
        .unwrap();
    let run = store.start_chain_run(&chain.id).await.unwrap();
    assert_eq!(run.current_step, 0);
    assert_eq!(run.status, "running");

    store.advance_chain_run(&run.id, 0, "task-1").await.unwrap();
    store.advance_chain_run(&run.id, 1, "task-2").await.unwrap();
    store.finish_chain_run(&run.id, "completed").await.unwrap();

    let runs = store.list_chain_runs(&chain.id, 10).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].current_step, 1);
    assert_eq!(runs[0].current_task_id.as_deref(), Some("task-2"));
    assert_eq!(runs[0].status, "completed");
}

#[tokio::test]
async fn running_runs_are_the_boot_resume_set() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let chain = store
        .upsert_schedule_chain(&input("resume-test"))
        .await
        .unwrap();
    let still_running = store.start_chain_run(&chain.id).await.unwrap();
    let finished = store.start_chain_run(&chain.id).await.unwrap();
    store
        .finish_chain_run(&finished.id, "completed")
        .await
        .unwrap();

    let running = store.list_running_chain_runs().await.unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].id, still_running.id);
}

#[tokio::test]
async fn list_is_empty_on_fresh_store() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    assert!(store.list_schedule_chains().await.unwrap().is_empty());
}
