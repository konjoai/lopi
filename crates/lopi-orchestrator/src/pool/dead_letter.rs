//! Verification gate (Finding #1) — dead-letter a task that exhausted its
//! retry budget without meeting its goal, instead of letting it become an
//! unremarkable `TaskStatus::Failed` with no durable, queryable trace.
//!
//! Split out of `run_loop.rs` purely to keep that file under the CLAUDE.md
//! file-size budget — pure code motion, called once from `run_one`'s single
//! terminal-outcome choke point, right after `AgentEvent::TaskCompleted` is
//! sent.

use lopi_core::{AgentEvent, EventBus, StopReason, TaskId, TaskStatus};
use lopi_memory::MemoryStore;
use tracing::warn;

/// If `outcome` is a `Failed` whose reason string encodes a genuine
/// retry-exhaustion [`StopReason`] (`MaxIterations`/`NoProgress`/`Budget` —
/// never `GoalMet`, which only ever terminates as `Success`; see
/// [`StopReason::parse_from_failure_reason`]), persist a `dead_letters` row
/// and broadcast [`AgentEvent::TaskDeadLettered`]. Any other `Failed` reason
/// (a cancellation, a non-retryable API error, a dry run) is left alone —
/// those are not retry exhaustion and must not be dead-lettered as if they
/// were.
pub(super) async fn record_if_exhausted(
    bus: &EventBus<AgentEvent>,
    store: Option<&MemoryStore>,
    task_id: TaskId,
    goal: &str,
    total_attempts: u8,
    outcome: &TaskStatus,
) {
    let TaskStatus::Failed { reason } = outcome else {
        return;
    };
    let Some(stop_reason) = StopReason::parse_from_failure_reason(reason) else {
        return;
    };
    if let Some(store) = store {
        if let Err(e) = store
            .save_dead_letter(
                &task_id.0.to_string(),
                goal,
                stop_reason.as_str(),
                total_attempts,
                reason,
            )
            .await
        {
            warn!(task_id = %task_id, "dead-letter persist failed: {e}");
        }
    }
    bus.send(AgentEvent::TaskDeadLettered {
        task_id,
        stop_reason: stop_reason.as_str().to_string(),
        attempts: total_attempts,
        goal: goal.to_string(),
        detail: reason.clone(),
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lopi_core::{Task, TaskId};

    fn task_id() -> TaskId {
        Task::new("fix the flaky test").id
    }

    #[tokio::test]
    async fn goal_met_success_is_never_dead_lettered() {
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let mut rx = bus.subscribe();
        record_if_exhausted(
            &bus,
            None,
            task_id(),
            "goal",
            1,
            &TaskStatus::Success {
                branch: "b".into(),
                pr_url: None,
            },
        )
        .await;
        assert!(rx.try_recv().is_err(), "a Success must never dead-letter");
    }

    #[tokio::test]
    async fn cancellation_is_not_dead_lettered() {
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let mut rx = bus.subscribe();
        record_if_exhausted(
            &bus,
            None,
            task_id(),
            "goal",
            1,
            &TaskStatus::Failed {
                reason: "Cancelled".into(),
            },
        )
        .await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn retry_exhaustion_emits_task_dead_lettered_and_persists() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let mut rx = bus.subscribe();
        let id = task_id();
        record_if_exhausted(
            &bus,
            Some(&store),
            id,
            "fix the flaky test",
            3,
            &TaskStatus::Failed {
                reason: "StopReason::max_iterations { Max retries exceeded }".into(),
            },
        )
        .await;

        let mut saw_event = false;
        while let Ok(ev) = rx.try_recv() {
            if let AgentEvent::TaskDeadLettered {
                task_id,
                stop_reason,
                attempts,
                goal,
                ..
            } = ev
            {
                saw_event = true;
                assert_eq!(task_id, id);
                assert_eq!(stop_reason, "max_iterations");
                assert_eq!(attempts, 3);
                assert_eq!(goal, "fix the flaky test");
            }
        }
        assert!(saw_event, "must emit TaskDeadLettered on retry exhaustion");

        let rows = store
            .load_dead_letters_for_task(&id.0.to_string())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].stop_reason, "max_iterations");
    }

    #[tokio::test]
    async fn dead_letter_still_emits_the_event_with_no_store_configured() {
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let mut rx = bus.subscribe();
        record_if_exhausted(
            &bus,
            None,
            task_id(),
            "goal",
            2,
            &TaskStatus::Failed {
                reason: "StopReason::no_progress { streak: 3, limit: 3 }".into(),
            },
        )
        .await;
        let mut saw_event = false;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, AgentEvent::TaskDeadLettered { .. }) {
                saw_event = true;
            }
        }
        assert!(saw_event, "no store must not block the event");
    }
}
