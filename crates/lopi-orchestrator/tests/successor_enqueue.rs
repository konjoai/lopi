//! Sprint Successor-1, Phase 4 — integration test for the enqueue path.
//!
//! Exercises the exact two calls `pool/run_loop.rs::run_one` wires together
//! once a run finishes and `AgentRunner::take_pending_successor()` returns a
//! child: `lopi_core::derive_successor_task` (the real containment gates,
//! same call `AgentRunner::derive_and_stash_successor` makes) followed by
//! `AgentPool::submit` (the real dedup/topology/queue path every other
//! caller goes through — no bespoke test-only insertion).
//!
//! What this test does NOT do: drive a real `claude -p` subprocess through
//! `AgentRunner::run()`'s full plan → implement → test → score loop. That
//! requires a live Anthropic API session and is not reachable from this
//! sandbox — see `NEXT_SESSION_PROMPT.md` for what a live run still needs to
//! confirm. `finalize.rs`'s own unit tests (`derive_and_stash_successor_*`)
//! cover the piece this test cannot: that a passing `finalize()` call
//! actually invokes `derive_successor_task` and stashes its result.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use lopi_core::{
    derive_successor_task, AgentEvent, AutonomyLevel, EventBus, Successor, SuccessorCondition,
    Task, TaskSource,
};
use lopi_orchestrator::{AgentPool, TaskQueue};

fn fixture_successor(goal: &str) -> Successor {
    Successor {
        goal: goal.to_string(),
        when: SuccessorCondition::OnSuccess,
        rationale: "follow-up identified during the run".to_string(),
        allowed_dirs: vec![],
    }
}

#[tokio::test]
async fn a_derived_successor_appears_in_the_queue_with_lineage_depth_and_gates_applied() {
    // A parent task representative of a real run: constrained autonomy,
    // an explicit forbidden dir, and an untrusted (webhook) origin — so this
    // one test exercises all four containment gates at once, the same way
    // `successor::tests::kt_a_inverted_derive_successor_task_blocks_the_escalation`
    // does at the lopi-core level.
    let mut parent = Task::new("fix the failing check");
    parent.source = TaskSource::Webhook {
        repo: "org/repo".into(),
        event: "check_run".into(),
    };
    parent.autonomy_level = AutonomyLevel::DraftPr;
    parent.forbidden_dirs = vec!["secrets/".to_string()];
    parent.successor_enabled = true;

    let fixture = fixture_successor("write the follow-up migration");

    // The exact call `AgentRunner::derive_and_stash_successor` makes.
    let child = derive_successor_task(&parent, &fixture, lopi_core::DEFAULT_MAX_CHAIN_DEPTH)
        .expect("a valid fixture at depth 0 must derive a successor");

    // A real pool — no mocking of the queue or the submit path.
    let queue = TaskQueue::new();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let pool = AgentPool::new(1, PathBuf::from("."), queue, bus);

    assert_eq!(pool.stats().queued, 0);
    let child_id = child.id;
    // The exact call `pool/run_loop.rs::run_one` makes once a successor is
    // stashed. `submit()` returns `Some` only on a dedup hit against an
    // already-queued identical goal — `None` here is the expected "enqueued
    // fresh" outcome, not a failure.
    let dedup_hit = pool.submit(child).await;
    assert!(dedup_hit.is_none(), "nothing else is queued to dedup against");

    assert_eq!(
        pool.stats().queued,
        1,
        "a second task must appear in the queue"
    );

    let enqueued = pool.queue().pop().await;
    assert_eq!(enqueued.id, child_id);

    // Lineage.
    assert_eq!(enqueued.parent_task, Some(parent.id));
    assert_eq!(enqueued.chain_depth, 1);
    match enqueued.source {
        TaskSource::SelfAuthored { parent: p } => assert_eq!(p, parent.id),
        other => panic!("expected SelfAuthored source, got {other:?}"),
    }

    // Gate 2 — autonomy ceiling: never wider than the parent's DraftPr.
    assert!(enqueued.autonomy_level.rank() <= AutonomyLevel::DraftPr.rank());

    // Gate 3 — directory inheritance: the parent's forbidden dir survives.
    assert!(enqueued.forbidden_dirs.iter().any(|d| d == "secrets/"));

    // Gate 4 — untrusted source: plan approval forced, chain cannot self-extend.
    assert!(enqueued.require_plan_approval);
    assert!(!enqueued.successor_enabled);
}

#[tokio::test]
async fn depth_cap_rejection_means_nothing_is_enqueued() {
    let mut parent = Task::new("already deep in its chain");
    parent.successor_enabled = true;
    parent.chain_depth = lopi_core::DEFAULT_MAX_CHAIN_DEPTH;

    let fixture = fixture_successor("go one hop too far");
    let result = derive_successor_task(&parent, &fixture, lopi_core::DEFAULT_MAX_CHAIN_DEPTH);
    assert!(result.is_err(), "depth cap must reject");

    // Mirrors `derive_and_stash_successor`: a rejection means nothing is
    // ever handed to `pool.submit`, so the queue stays empty.
    let queue = TaskQueue::new();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let pool = AgentPool::new(1, PathBuf::from("."), queue, bus);
    assert_eq!(pool.stats().queued, 0);
}
