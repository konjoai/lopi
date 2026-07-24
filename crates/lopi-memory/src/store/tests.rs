#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use super::*;
use chrono::Utc;
use lopi_core::{Attempt, Task, TaskId, TurnMetrics};
use uuid::Uuid;

#[tokio::test]
async fn save_and_load_task_round_trip() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("integrate the flux capacitor");
    store.save_task(&task, "queued").await.unwrap();

    let history = store.load_history(10).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].goal, "integrate the flux capacitor");
    assert_eq!(history[0].status, "queued");
}

#[tokio::test]
async fn mark_completed_updates_status() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("refactor the warp core");
    store.save_task(&task, "queued").await.unwrap();
    store.mark_completed(&task.id, "success").await.unwrap();

    let history = store.load_history(10).await.unwrap();
    assert_eq!(history[0].status, "success");
    assert!(history[0].completed_at.is_some());
}

#[tokio::test]
async fn mark_running_moves_out_of_queued_without_completing() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("compile the dilithium matrix");
    store.save_task(&task, "queued").await.unwrap();

    store.mark_running(&task.id).await.unwrap();
    let history = store.load_history(10).await.unwrap();
    assert_eq!(history[0].status, "running");
    // Still in flight — no completion timestamp yet.
    assert!(history[0].completed_at.is_none());

    // A subsequent terminal write flips it and stamps completion.
    store.mark_completed(&task.id, "success").await.unwrap();
    let history = store.load_history(10).await.unwrap();
    assert_eq!(history[0].status, "success");
    assert!(history[0].completed_at.is_some());
}

#[tokio::test]
async fn save_task_upserts_status() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("fix flaky test");
    store.save_task(&task, "queued").await.unwrap();
    store.save_task(&task, "implementing").await.unwrap();

    assert_eq!(store.task_count().await.unwrap(), 1);
    let history = store.load_history(10).await.unwrap();
    assert_eq!(history[0].status, "implementing");
}

#[tokio::test]
async fn save_attempt_persists() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("add feature X");
    store.save_task(&task, "queued").await.unwrap();

    let mut attempt = Attempt::new(task.id, 1, "lopi/abc-attempt-1");
    attempt.outcome = "success".into();
    store.save_attempt(&attempt).await.unwrap();
}

#[tokio::test]
async fn load_history_newest_first() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    for i in 0..5u8 {
        let t = Task::new(format!("task number {i} work"));
        store.save_task(&t, "queued").await.unwrap();
    }
    let history = store.load_history(3).await.unwrap();
    assert_eq!(history.len(), 3);
}

#[tokio::test]
async fn empty_store_returns_empty_history() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let history = store.load_history(10).await.unwrap();
    assert!(history.is_empty());
}

fn make_turn_metrics(task_id: TaskId) -> TurnMetrics {
    TurnMetrics {
        turn_id: Uuid::new_v4(),
        task_id,
        session_id: Uuid::new_v4(),
        model: "claude-sonnet-4-6".into(),
        attempt_number: 1,
        input_tokens: 500,
        output_tokens: 200,
        cache_read_input_tokens: 0,
        cache_write_input_tokens: 100,
        ttft_ms: 300,
        turn_latency_ms: 1200,
        tool_execution_ms: 50,
        context_tokens: 4000,
        context_pressure: 0.25,
        evictions_this_turn: 0,
        tool_calls: 2,
        tools_parallel: false,
        estimated_cost_usd: 0.003,
        timestamp: Utc::now(),
    }
}

#[tokio::test]
async fn save_turn_metrics_succeeds() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("measure tokens");
    store.save_task(&task, "queued").await.unwrap();
    let m = make_turn_metrics(task.id);
    store.save_turn_metrics(&m).await.unwrap();
}

#[tokio::test]
async fn task_costs_sums_billed_cost_per_task() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("accrue cost");
    store.save_task(&task, "queued").await.unwrap();
    // Two turns (e.g. plan + implement) on the same task, distinct turn_ids.
    let mut plan = make_turn_metrics(task.id);
    plan.estimated_cost_usd = 0.010;
    let mut implement = make_turn_metrics(task.id);
    implement.estimated_cost_usd = 0.037;
    store.save_turn_metrics(&plan).await.unwrap();
    store.save_turn_metrics(&implement).await.unwrap();

    let costs = store.task_costs().await.unwrap();
    let total = costs.get(&task.id.0.to_string()).copied().unwrap_or(0.0);
    assert!((total - 0.047).abs() < 1e-9, "expected 0.047, got {total}");
    // A task with no recorded turns is absent (callers treat as 0.0).
    assert!(!costs.contains_key("no-such-task"));
}

#[tokio::test]
async fn task_cost_sums_billed_cost_for_a_single_task() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("accrue cost, single-task query");
    store.save_task(&task, "queued").await.unwrap();
    let mut plan = make_turn_metrics(task.id);
    plan.estimated_cost_usd = 0.010;
    let mut implement = make_turn_metrics(task.id);
    implement.estimated_cost_usd = 0.037;
    store.save_turn_metrics(&plan).await.unwrap();
    store.save_turn_metrics(&implement).await.unwrap();

    let total = store.task_cost(&task.id.0.to_string()).await.unwrap();
    assert!((total - 0.047).abs() < 1e-9, "expected 0.047, got {total}");
}

#[tokio::test]
async fn task_cost_is_zero_for_a_task_with_no_recorded_turns() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let cost = store.task_cost("no-such-task").await.unwrap();
    assert_eq!(cost, 0.0);
}

#[tokio::test]
async fn daily_token_totals_nonzero_after_cli_turn_persisted() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("real billed run");
    store.save_task(&task, "queued").await.unwrap();
    store
        .save_turn_metrics(&make_turn_metrics(task.id))
        .await
        .unwrap();
    let (tokens, cost) = store.daily_token_totals().await.unwrap();
    // input 500 + output 200 + cache_read 0 + cache_write 100 = 800.
    assert_eq!(tokens, 800);
    assert!(cost > 0.0, "cost should reflect real spend, got {cost}");
}

#[tokio::test]
async fn save_turn_metrics_dedup_on_same_turn_id() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("measure tokens deduplicated");
    store.save_task(&task, "queued").await.unwrap();
    let mut m = make_turn_metrics(task.id);
    let fixed_id = Uuid::new_v4();
    m.turn_id = fixed_id;
    store.save_turn_metrics(&m).await.unwrap();
    // Second insert with same turn_id should silently succeed (ON CONFLICT DO NOTHING)
    store.save_turn_metrics(&m).await.unwrap();
}

#[tokio::test]
async fn task_count_increments_per_save() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    assert_eq!(store.task_count().await.unwrap(), 0);
    for i in 0..3u8 {
        let t = Task::new(format!("task count test {i}"));
        store.save_task(&t, "queued").await.unwrap();
    }
    assert_eq!(store.task_count().await.unwrap(), 3);
}

// ── Verify-1 F3/F4: DB-sourced lifecycle counts (the fix for /api/stats +
//    "N live" undercounting when the per-pool in-memory counters miss tasks
//    dispatched to other repos' pools). The store is the shared source of
//    truth, so its counts are correct regardless of how many pools wrote them.

#[tokio::test]
async fn status_counts_aggregates_every_lifecycle_bucket() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    for (goal, status) in [
        ("a", "running"),
        ("b", "running"),
        ("c", "queued"),
        ("d", "success"),
        ("e", "success"),
        ("f", "success"),
        ("g", "failed"),
    ] {
        store.save_task(&Task::new(goal), status).await.unwrap();
    }
    let counts = store.status_counts().await.unwrap();
    assert_eq!(counts.running, 2);
    assert_eq!(counts.queued, 1);
    assert_eq!(counts.succeeded, 3);
    assert_eq!(counts.failed, 1);
}

#[tokio::test]
async fn status_counts_tolerates_legacy_decorated_status() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    // A pre-Fix-1 write could leave a decorated status like "failed ❌ Cancelled";
    // prefix matching still buckets it correctly.
    store
        .save_task(&Task::new("legacy"), "failed ❌ Cancelled")
        .await
        .unwrap();
    assert_eq!(store.status_counts().await.unwrap().failed, 1);
}

// ── Verify-1 F8: existence check backing the 404-vs-empty-200 distinction on
//    the id-scoped read routes.

#[tokio::test]
async fn task_exists_distinguishes_known_from_bogus_id() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("known");
    let id = task.id.0.to_string();
    store.save_task(&task, "success").await.unwrap();
    assert!(store.task_exists(&id).await.unwrap(), "known id exists");
    assert!(
        !store
            .task_exists("00000000-0000-0000-0000-000000000000")
            .await
            .unwrap(),
        "bogus id does not exist"
    );
}
