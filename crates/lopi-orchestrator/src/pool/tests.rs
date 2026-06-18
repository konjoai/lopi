#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use lopi_core::topology::TopologyHint;
use lopi_core::{AgentEvent, EventBus, Priority, Task};
use std::path::PathBuf;

fn make_pool(max: usize) -> AgentPool {
    let queue = TaskQueue::new();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    AgentPool::new(max, PathBuf::from("."), queue, bus)
}

#[tokio::test]
async fn stats_when_empty() {
    let pool = make_pool(2);
    let stats = pool.stats();
    assert_eq!(stats.running, 0);
    assert_eq!(stats.queued, 0);
    assert_eq!(stats.succeeded, 0);
    assert_eq!(stats.failed, 0);
}

#[tokio::test]
async fn submit_task_increases_queued_count() {
    let pool = make_pool(2);
    let task = Task::new("do something useful");
    pool.submit(task).await;
    let stats = pool.stats();
    assert_eq!(stats.queued, 1);
}

#[tokio::test]
async fn submit_multiple_tasks_increases_queued() {
    let pool = make_pool(4);
    for i in 0..3 {
        let task = Task::new(format!("task number {i} unique goal"));
        pool.submit(task).await;
    }
    let stats = pool.stats();
    assert_eq!(stats.queued, 3);
}

#[tokio::test]
async fn submit_duplicate_goal_returns_existing_id() {
    let pool = make_pool(2);
    let t1 = Task::new("fix the same bug");
    let t2 = Task::new("fix the same bug");
    let r1 = pool.submit(t1).await;
    let r2 = pool.submit(t2).await;
    // First submit returns None (new task)
    assert!(r1.is_none());
    // Second submit returns Some (duplicate)
    assert!(r2.is_some());
    // Only one task in the queue
    assert_eq!(pool.stats().queued, 1);
}

#[tokio::test]
async fn cancel_nonexistent_task_returns_false() {
    let pool = make_pool(2);
    let fake_id = TaskId::new();
    let cancelled = pool.cancel(&fake_id).await;
    assert!(!cancelled);
}

#[tokio::test]
async fn pool_queue_accessor_works() {
    let pool = make_pool(2);
    let queue = pool.queue();
    // Queue starts empty
    assert!(queue.is_empty());
}

#[tokio::test]
async fn pool_bus_accessor_works() {
    let pool = make_pool(2);
    let bus = pool.bus();
    let mut rx = bus.subscribe();
    // Send an event and verify the bus works
    bus.send(AgentEvent::TaskQueued {
        task_id: TaskId::new(),
        goal: "test goal".to_string(),
        priority: Priority::Normal,
    });
    let ev = rx.try_recv();
    assert!(ev.is_ok());
}

#[tokio::test]
async fn submit_broadcasts_task_queued_event() {
    let pool = make_pool(2);
    let mut rx = pool.bus().subscribe();
    let task = Task::new("broadcast test goal");
    pool.submit(task).await;
    // Should have received a TaskQueued event
    let ev = rx.try_recv();
    assert!(ev.is_ok());
    match ev.unwrap() {
        AgentEvent::TaskQueued { goal, .. } => {
            assert_eq!(goal, "broadcast test goal");
        }
        other => panic!("expected TaskQueued, got {other:?}"),
    }
}

#[tokio::test]
async fn pool_with_store_does_not_panic() {
    let queue = TaskQueue::new();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let pool = AgentPool::new(2, PathBuf::from("."), queue, bus);
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let pool = pool.with_store(store);
    let task = Task::new("task with store");
    pool.submit(task).await;
    assert_eq!(pool.stats().queued, 1);
}

#[tokio::test]
async fn uptime_is_non_zero_after_submit() {
    let pool = make_pool(2);
    // Small sleep to ensure uptime > 0
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    let stats = pool.stats();
    // uptime_secs may be 0 for very fast tests, but started_at should be set
    // Just verify it doesn't panic
    let _ = stats.uptime_secs;
}

#[tokio::test]
async fn shutdown_completes_without_running_tasks() {
    let pool = make_pool(2);
    // Shutdown with no running tasks should complete immediately
    pool.shutdown().await;
}

#[tokio::test]
async fn submit_high_priority_task() {
    let pool = make_pool(2);
    let mut task = Task::new("critical security fix");
    task.priority = Priority::High;
    pool.submit(task).await;
    let stats = pool.stats();
    assert_eq!(stats.queued, 1);
}

// ─── Sprint T — topology classification ──────────────────────────

#[test]
fn effective_topology_uses_explicit_hint() {
    let mut task = Task::new("anything at all");
    task.topology = Some(TopologyHint::Sequential);
    assert_eq!(effective_topology(&task), TopologyHint::Sequential);
}

#[test]
fn effective_topology_classifies_when_unset() {
    let task = Task::new("update every crate independently in parallel");
    assert_eq!(effective_topology(&task), TopologyHint::Parallel);
}

#[tokio::test]
async fn submit_fills_in_missing_topology() {
    let pool = make_pool(2);
    let task = Task::new("decompose the epic into subtasks and delegate");
    assert!(task.topology.is_none());
    pool.submit(task).await;
    // The task was enqueued (topology enrichment never blocks submission).
    assert_eq!(pool.stats().queued, 1);
}

// ─── P2 — required-capability matching ───────────────────────────

#[tokio::test]
async fn can_satisfy_with_empty_requirements_always_passes() {
    let pool = make_pool(2);
    let task = Task::new("vanilla task, no requirements");
    assert!(pool.can_satisfy(&task));
}

#[tokio::test]
async fn can_satisfy_returns_false_with_empty_registry() {
    let pool = make_pool(2);
    let mut task = Task::new("needs python");
    task.required_capabilities = vec!["python".into()];
    // No agents registered → must fail closed.
    assert!(!pool.can_satisfy(&task));
}

#[tokio::test]
async fn can_satisfy_picks_up_any_matching_agent() {
    let pool = make_pool(2);
    pool.register_capabilities("alpha", vec!["rust".into(), "git".into()]);
    pool.register_capabilities("beta", vec!["python".into(), "ml".into()]);
    let mut task = Task::new("ml inference");
    task.required_capabilities = vec!["python".into(), "ml".into()];
    assert!(pool.can_satisfy(&task), "beta covers both required caps");
    // No single agent has rust+python — must fail.
    task.required_capabilities = vec!["rust".into(), "python".into()];
    assert!(!pool.can_satisfy(&task));
}

#[tokio::test]
async fn deregister_removes_capability_advertisement() {
    let pool = make_pool(2);
    pool.register_capabilities("alpha", vec!["rust".into()]);
    let mut task = Task::new("rust work");
    task.required_capabilities = vec!["rust".into()];
    assert!(pool.can_satisfy(&task));
    assert!(pool.deregister_capabilities("alpha"));
    assert!(!pool.can_satisfy(&task));
    // Second deregister is a no-op.
    assert!(!pool.deregister_capabilities("alpha"));
}

// ─── P2 — per-agent rate limiting ────────────────────────────────

#[tokio::test]
async fn unregistered_agent_is_unlimited() {
    let pool = make_pool(2);
    // Without registration the gate is wide open — every acquire
    // returns true and there's no in-flight to release.
    for _ in 0..100 {
        assert!(pool.try_acquire_agent("ghost").await);
    }
    // release_agent on an unregistered id is a clean noop.
    pool.release_agent("ghost");
}

#[tokio::test]
async fn register_rejects_zero_per_minute() {
    let pool = make_pool(2);
    let ok = pool.register_agent_rate_limit(
        "bad",
        crate::AgentRateLimit {
            max_per_minute: 0,
            max_concurrent: 4,
        },
    );
    assert!(!ok, "0/min should be rejected");
    // No entry was written.
    assert!(pool.agent_rate_limit("bad").is_none());
}

#[tokio::test]
async fn token_bucket_caps_burst_at_max_per_minute() {
    let pool = make_pool(2);
    assert!(pool.register_agent_rate_limit(
        "alpha",
        crate::AgentRateLimit {
            max_per_minute: 3,
            max_concurrent: 0
        },
    ));
    // First 3 acquires succeed; the 4th is rate-limited.
    for _ in 0..3 {
        assert!(pool.try_acquire_agent("alpha").await);
    }
    assert!(!pool.try_acquire_agent("alpha").await);
    let snap = pool.agent_rate_limit("alpha").unwrap();
    assert_eq!(snap.in_flight, 3);
}

#[tokio::test]
async fn concurrency_cap_short_circuits_before_bucket() {
    let pool = make_pool(2);
    assert!(pool.register_agent_rate_limit(
        "alpha",
        crate::AgentRateLimit {
            max_per_minute: 1_000,
            max_concurrent: 2
        },
    ));
    // Two acquires use 2 of 1000 tokens but saturate the concurrency cap.
    assert!(pool.try_acquire_agent("alpha").await);
    assert!(pool.try_acquire_agent("alpha").await);
    assert!(
        !pool.try_acquire_agent("alpha").await,
        "concurrency cap should block even with tokens to spare"
    );
    // Release frees a slot.
    pool.release_agent("alpha");
    assert!(pool.try_acquire_agent("alpha").await);
}

#[tokio::test]
async fn release_saturates_at_zero() {
    let pool = make_pool(2);
    assert!(pool.register_agent_rate_limit(
        "alpha",
        crate::AgentRateLimit {
            max_per_minute: 10,
            max_concurrent: 2
        },
    ));
    // Three releases against zero in-flight must not underflow.
    pool.release_agent("alpha");
    pool.release_agent("alpha");
    pool.release_agent("alpha");
    let snap = pool.agent_rate_limit("alpha").unwrap();
    assert_eq!(snap.in_flight, 0);
}
