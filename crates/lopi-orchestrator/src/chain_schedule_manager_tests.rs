//! Orchestrator tests — chain schedule manager. Split out to keep the
//! main module under the 500-line file gate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::queue::TaskQueue;
use lopi_core::{EventBus, Task};
use lopi_memory::{ChainStepInput, ScheduleChainInput};
use std::time::Duration;

fn two_step_chain(on_fail: &str) -> ScheduleChainInput {
    ScheduleChainInput {
        id: None,
        name: "chain-test".into(),
        cron: "0 2 * * *".into(),
        repo: None,
        priority: "normal".into(),
        autonomy_level: "draft_pr".into(),
        on_fail: on_fail.into(),
        enabled: true,
        steps: vec![
            ChainStepInput {
                goal: "step one".into(),
                allowed_dirs: vec![],
                forbidden_dirs: vec![],
            },
            ChainStepInput {
                goal: "step two".into(),
                allowed_dirs: vec![],
                forbidden_dirs: vec![],
            },
        ],
    }
}

async fn manager() -> (ChainScheduleManager, AgentPool, MemoryStore) {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let queue = TaskQueue::new();
    let bus: EventBus<AgentEvent> = EventBus::new(64);
    let pool = AgentPool::new(1, PathBuf::from("."), queue, bus).with_store(store.clone());
    (
        ChainScheduleManager::new(pool.clone(), store.clone()),
        pool,
        store,
    )
}

/// Poll a run row until `pred` accepts it, or panic after a short
/// timeout. Standing in for a real event wait: the listener spawned by
/// `submit_step` runs on the same executor as the test, so it needs the
/// test to yield (via the sleep) before it gets scheduled.
async fn wait_for_run(
    store: &MemoryStore,
    run_id: &str,
    pred: impl Fn(&ChainRunRow) -> bool,
) -> ChainRunRow {
    for _ in 0..200 {
        if let Some(run) = store.get_chain_run(run_id).await.unwrap() {
            if pred(&run) {
                return run;
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("condition never became true within timeout");
}

#[tokio::test]
async fn start_is_idempotent() {
    let (mgr, _pool, _store) = manager().await;
    mgr.start().await.unwrap();
    mgr.start().await.unwrap();
}

#[tokio::test]
async fn register_and_unregister_roundtrip() {
    let (mgr, _pool, store) = manager().await;
    mgr.start().await.unwrap();
    let row = store
        .upsert_schedule_chain(&two_step_chain("stop"))
        .await
        .unwrap();
    assert!(mgr.register(row.clone().into()).await.unwrap());
    // Invalid cron returns Ok(false), not an error.
    let mut bad_input = two_step_chain("stop");
    bad_input.cron = "not a cron".into();
    let bad_row = store.upsert_schedule_chain(&bad_input).await.unwrap();
    assert!(!mgr.register(bad_row.into()).await.unwrap());
    // Unregister is idempotent.
    mgr.unregister(&row.id).await.unwrap();
    mgr.unregister(&row.id).await.unwrap();
}

#[tokio::test]
async fn run_now_submits_only_step_zero() {
    let (mgr, _pool, store) = manager().await;
    let row = store
        .upsert_schedule_chain(&two_step_chain("stop"))
        .await
        .unwrap();
    let run_id = mgr.run_now(row.into()).await.unwrap().unwrap();

    let run = store.get_chain_run(&run_id).await.unwrap().unwrap();
    assert_eq!(run.current_step, 0);
    assert!(run.current_task_id.is_some());
    assert_eq!(run.status, "running");
}

#[tokio::test]
async fn run_now_on_chain_with_no_steps_returns_none() {
    let (mgr, _pool, store) = manager().await;
    let mut input = two_step_chain("stop");
    input.steps.clear();
    let row = store.upsert_schedule_chain(&input).await.unwrap();
    assert!(mgr.run_now(row.into()).await.unwrap().is_none());
}

#[tokio::test]
async fn task_completion_advances_to_next_step_and_finishes_chain() {
    let (mgr, pool, store) = manager().await;
    let row = store
        .upsert_schedule_chain(&two_step_chain("stop"))
        .await
        .unwrap();
    let run_id = mgr.run_now(row.into()).await.unwrap().unwrap();

    let step0_task = store
        .get_chain_run(&run_id)
        .await
        .unwrap()
        .unwrap()
        .current_task_id
        .unwrap();
    let step0_id = TaskId(uuid::Uuid::parse_str(&step0_task).unwrap());
    pool.bus().send(AgentEvent::TaskCompleted {
        task_id: step0_id,
        outcome: TaskStatus::Success {
            branch: "b".into(),
            pr_url: None,
        },
        total_attempts: 1,
        successor: None,
    });

    // Step advances to 1 with a *different* task id.
    let run = wait_for_run(&store, &run_id, |r| r.current_step == 1).await;
    let step1_task = run.current_task_id.clone().unwrap();
    assert_ne!(step1_task, step0_task);
    assert_eq!(run.status, "running");

    let step1_id = TaskId(uuid::Uuid::parse_str(&step1_task).unwrap());
    pool.bus().send(AgentEvent::TaskCompleted {
        task_id: step1_id,
        outcome: TaskStatus::Success {
            branch: "b".into(),
            pr_url: None,
        },
        total_attempts: 1,
        successor: None,
    });

    wait_for_run(&store, &run_id, |r| r.status == "completed").await;
}

#[tokio::test]
async fn on_fail_stop_ends_run_without_submitting_next_step() {
    let (mgr, pool, store) = manager().await;
    let row = store
        .upsert_schedule_chain(&two_step_chain("stop"))
        .await
        .unwrap();
    let run_id = mgr.run_now(row.into()).await.unwrap().unwrap();
    let step0_task = store
        .get_chain_run(&run_id)
        .await
        .unwrap()
        .unwrap()
        .current_task_id
        .unwrap();
    let step0_id = TaskId(uuid::Uuid::parse_str(&step0_task).unwrap());

    pool.bus().send(AgentEvent::TaskCompleted {
        task_id: step0_id,
        outcome: TaskStatus::Failed {
            reason: "boom".into(),
        },
        total_attempts: 3,
        successor: None,
    });

    // Still parked on step 0 — never advanced past the failed step.
    let run = wait_for_run(&store, &run_id, |r| r.status == "failed").await;
    assert_eq!(run.current_step, 0);
}

#[tokio::test]
async fn on_fail_continue_advances_past_a_failed_step() {
    let (mgr, pool, store) = manager().await;
    let row = store
        .upsert_schedule_chain(&two_step_chain("continue"))
        .await
        .unwrap();
    let run_id = mgr.run_now(row.into()).await.unwrap().unwrap();
    let step0_task = store
        .get_chain_run(&run_id)
        .await
        .unwrap()
        .unwrap()
        .current_task_id
        .unwrap();
    let step0_id = TaskId(uuid::Uuid::parse_str(&step0_task).unwrap());

    pool.bus().send(AgentEvent::TaskCompleted {
        task_id: step0_id,
        outcome: TaskStatus::Failed {
            reason: "boom".into(),
        },
        total_attempts: 3,
        successor: None,
    });

    wait_for_run(&store, &run_id, |r| r.current_step == 1).await;
}

/// The exact failure mode from the original incident: kill the backend
/// mid-chain, restart it, confirm the chain resumes correctly rather than
/// re-running a completed step or silently dropping the rest of the chain.
#[tokio::test]
async fn resume_orphaned_advances_when_step_already_finished_before_restart() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let chain_row = store
        .upsert_schedule_chain(&two_step_chain("stop"))
        .await
        .unwrap();
    let run = store.start_chain_run(&chain_row.id).await.unwrap();

    // Simulate step 0 having actually completed durably (mark_completed
    // wrote the row) just before the process died, so its
    // `TaskCompleted` event never reached the (now-dead) listener.
    let finished_task = Task::new("step one");
    store.save_task(&finished_task, "queued").await.unwrap();
    store
        .mark_completed(&finished_task.id, "success")
        .await
        .unwrap();
    store
        .advance_chain_run(&run.id, 0, &finished_task.id.0.to_string())
        .await
        .unwrap();

    // Fresh manager + fresh pool/bus/queue — simulates the new process.
    let queue = TaskQueue::new();
    let bus: EventBus<AgentEvent> = EventBus::new(64);
    let pool = AgentPool::new(1, PathBuf::from("."), queue, bus).with_store(store.clone());
    let mgr = ChainScheduleManager::new(pool, store.clone());
    mgr.start().await.unwrap();

    // Resumed by advancing to step 1 — never re-running the already
    // `success` step 0.
    wait_for_run(&store, &run.id, |r| r.current_step == 1).await;
}

#[tokio::test]
async fn resume_orphaned_resubmits_the_in_flight_step_when_task_was_lost() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let chain_row = store
        .upsert_schedule_chain(&two_step_chain("stop"))
        .await
        .unwrap();
    let run = store.start_chain_run(&chain_row.id).await.unwrap();

    // Step 0's task was queued but never reached a terminal durable
    // status — the old process died mid-run, so it's truly orphaned.
    let orphaned_task = Task::new("step one");
    store.save_task(&orphaned_task, "running").await.unwrap();
    store
        .advance_chain_run(&run.id, 0, &orphaned_task.id.0.to_string())
        .await
        .unwrap();

    let queue = TaskQueue::new();
    let bus: EventBus<AgentEvent> = EventBus::new(64);
    let pool = AgentPool::new(1, PathBuf::from("."), queue, bus).with_store(store.clone());
    let mgr = ChainScheduleManager::new(pool, store.clone());
    mgr.start().await.unwrap();

    // Resubmitted at the SAME step (0), with a fresh task id — never
    // silently dropped, never skipped to step 1 for an unfinished step.
    wait_for_run(&store, &run.id, |r| {
        r.current_step == 0
            && r.current_task_id.as_deref() != Some(orphaned_task.id.0.to_string().as_str())
    })
    .await;
}
