//! Stack-Chain-1 — integration test for the exact failure mode that
//! motivated this sprint: a `schedule_chains` run left mid-flight when the
//! backend process dies, and whether restarting the process resumes it
//! correctly.
//!
//! Unlike `chain_schedule_manager_tests.rs` (unit tests sharing one
//! in-memory `MemoryStore` across "before" and "after"), this test opens a
//! real on-disk SQLite file, drops every in-process object (`AgentPool`,
//! `ChainScheduleManager`, `MemoryStore`, `TaskQueue`, `EventBus` — the full
//! set that dies with a process), then reopens a completely fresh set
//! against the same file. That's the actual boundary a process restart
//! crosses: only the DB file survives, nothing in memory does. If resume
//! only worked because a `MemoryStore` handle happened to still be alive,
//! this test would catch it; the unit tests would not.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use lopi_core::{AgentEvent, AutonomyLevel, EventBus};
use lopi_memory::{ChainStepInput, MemoryStore, ScheduleChainInput};
use lopi_orchestrator::{AgentPool, ChainScheduleManager, TaskQueue};

fn chain_input() -> ScheduleChainInput {
    ScheduleChainInput {
        id: None,
        name: "incident-resume-test".into(),
        cron: "0 2 * * *".into(),
        repo: None,
        priority: "normal".into(),
        autonomy_level: AutonomyLevel::default().tag_snake().to_string(),
        on_fail: "stop".into(),
        enabled: true,
        steps: vec![
            ChainStepInput {
                goal: "research the incident root cause".into(),
                allowed_dirs: vec![],
                forbidden_dirs: vec![],
            },
            ChainStepInput {
                goal: "implement the fix".into(),
                allowed_dirs: vec![],
                forbidden_dirs: vec![],
            },
        ],
    }
}

/// Build a fresh, unstarted set of in-process objects against the DB file at
/// `db_path` — standing in for "the process just started."
async fn boot(db_path: &PathBuf) -> (ChainScheduleManager, MemoryStore) {
    let store = MemoryStore::open(db_path).await.expect("open db file");
    let queue = TaskQueue::new();
    let bus: EventBus<AgentEvent> = EventBus::new(64);
    let pool = AgentPool::new(1, PathBuf::from("."), queue, bus).with_store(store.clone());
    (ChainScheduleManager::new(pool, store.clone()), store)
}

async fn wait_for_step(store: &MemoryStore, run_id: &str, want_step: i64) {
    for _ in 0..200 {
        if let Some(run) = store.get_chain_run(run_id).await.expect("get run") {
            if run.current_step == want_step {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("run never reached step {want_step} within timeout");
}

#[tokio::test]
async fn chain_survives_a_full_process_restart_mid_flight() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("lopi-resume-test.sqlite");

    // "Process 1": create the chain, fire it, let step 0 get submitted.
    let run_id = {
        let (mgr, store) = boot(&db_path).await;
        let chain = store
            .upsert_schedule_chain(&chain_input())
            .await
            .expect("create chain");
        let run_id = mgr
            .run_now(chain.into())
            .await
            .expect("run_now")
            .expect("chain has steps");
        wait_for_step(&store, &run_id, 0).await;
        let run = store
            .get_chain_run(&run_id)
            .await
            .expect("get run")
            .expect("run exists");
        assert!(
            run.current_task_id.is_some(),
            "step 0 must have a task in flight before the simulated crash"
        );
        assert_eq!(run.status, "running");
        run_id
        // `mgr`, `store`, `pool`, `bus`, `queue` all drop here — nothing
        // in-process survives past this point, exactly like a real restart.
    };

    // "Process 2": fresh objects, same DB file. Step 0's task was queued but
    // never reached a terminal durable status (no execution loop ever ran in
    // this test), so it is genuinely orphaned by the "restart" — this must
    // resubmit step 0, not skip to step 1 and not silently drop the chain.
    {
        let (mgr, store) = boot(&db_path).await;
        mgr.start().await.expect("start resumes orphaned runs");
        wait_for_step(&store, &run_id, 0).await;
        let run = store
            .get_chain_run(&run_id)
            .await
            .expect("get run")
            .expect("run survives the restart in the db file");
        assert_eq!(
            run.current_step, 0,
            "resume must resubmit the orphaned step, not skip ahead"
        );
        assert_eq!(
            run.status, "running",
            "an orphaned run is not silently dropped"
        );
    }
}

#[tokio::test]
async fn completed_step_advances_on_restart_instead_of_rerunning() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("lopi-resume-completed-test.sqlite");

    let run_id = {
        let (_mgr, store) = boot(&db_path).await;
        let chain = store
            .upsert_schedule_chain(&chain_input())
            .await
            .expect("create chain");
        let run = store.start_chain_run(&chain.id).await.expect("start run");

        // Simulate step 0 having actually finished (mark_completed wrote the
        // durable row) in the instant before the process died — its
        // `TaskCompleted` event never reached a listener because there was
        // no process left to receive it.
        let task = lopi_core::Task::new("research the incident root cause");
        store.save_task(&task, "queued").await.expect("save task");
        store
            .mark_completed(&task.id, "success")
            .await
            .expect("mark completed");
        store
            .advance_chain_run(&run.id, 0, &task.id.0.to_string())
            .await
            .expect("advance run");
        run.id
    };

    let (mgr, store) = boot(&db_path).await;
    mgr.start().await.expect("start resumes orphaned runs");
    wait_for_step(&store, &run_id, 1).await;
    let run = store
        .get_chain_run(&run_id)
        .await
        .expect("get run")
        .expect("run exists");
    assert_eq!(
        run.current_step, 1,
        "a step that actually finished before the restart must advance, not rerun"
    );
}
