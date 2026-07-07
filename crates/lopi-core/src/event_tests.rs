#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::task::{Priority, TaskId, TaskStatus};

#[tokio::test]
async fn event_bus_subscribe_and_publish() {
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let mut rx = bus.subscribe();

    let task_id = TaskId::new();
    bus.send(AgentEvent::TaskQueued {
        task_id,
        goal: "test goal".to_string(),
        priority: Priority::Normal,
    });

    let ev = rx.try_recv().unwrap();
    match ev {
        AgentEvent::TaskQueued {
            task_id: received_id,
            goal,
            ..
        } => {
            assert_eq!(received_id, task_id);
            assert_eq!(goal, "test goal");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn event_bus_multiple_subscribers() {
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let mut rx1 = bus.subscribe();
    let mut rx2 = bus.subscribe();

    let task_id = TaskId::new();
    bus.send(AgentEvent::TaskCancelled { task_id });

    let e1 = rx1.try_recv().unwrap();
    let e2 = rx2.try_recv().unwrap();

    assert!(matches!(e1, AgentEvent::TaskCancelled { .. }));
    assert!(matches!(e2, AgentEvent::TaskCancelled { .. }));
}

#[tokio::test]
async fn event_bus_send_no_subscribers_does_not_panic() {
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    // No subscribers — send should not panic
    bus.send(AgentEvent::TaskCancelled {
        task_id: TaskId::new(),
    });
}

#[test]
fn agent_event_info_helper() {
    let task_id = TaskId::new();
    let ev = AgentEvent::info(task_id, "hello from info");
    match ev {
        AgentEvent::LogLine { line, level, .. } => {
            assert_eq!(line, "hello from info");
            assert!(matches!(level, LogLevel::Info));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn agent_event_warn_helper() {
    let task_id = TaskId::new();
    let ev = AgentEvent::warn(task_id, "warning message");
    match ev {
        AgentEvent::LogLine { line, level, .. } => {
            assert_eq!(line, "warning message");
            assert!(matches!(level, LogLevel::Warn));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn agent_event_error_helper() {
    let task_id = TaskId::new();
    let ev = AgentEvent::error(task_id, "error message");
    match ev {
        AgentEvent::LogLine { line, level, .. } => {
            assert_eq!(line, "error message");
            assert!(matches!(level, LogLevel::Error));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn agent_event_log_sets_timestamp() {
    let task_id = TaskId::new();
    let before = Utc::now();
    let ev = AgentEvent::log(task_id, "timed event", LogLevel::Debug);
    let after = Utc::now();
    match ev {
        AgentEvent::LogLine { ts, .. } => {
            assert!(ts >= before);
            assert!(ts <= after);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn agent_event_serializes_to_json() {
    let task_id = TaskId::new();
    let ev = AgentEvent::TaskCancelled { task_id };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("task_cancelled"));
    assert!(json.contains(&task_id.0.to_string()));
}

#[test]
fn pool_stats_event_serializes() {
    let ev = AgentEvent::PoolStats {
        running: 2,
        queued: 5,
        succeeded: 10,
        failed: 1,
        uptime_secs: 3600,
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("pool_stats"));
    assert!(json.contains("3600"));
}

#[test]
fn report_ready_event_serializes() {
    let task_id = TaskId::new();
    let ev = AgentEvent::ReportReady {
        task_id,
        channel: "telegram".to_string(),
        summary: "task done".to_string(),
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("report_ready"));
    assert!(json.contains("telegram"));
    assert!(json.contains("task done"));
}

#[test]
fn score_updated_event_serializes() {
    let task_id = TaskId::new();
    let ev = AgentEvent::ScoreUpdated {
        task_id,
        test_pass_rate: 0.95,
        lint_errors: 2,
        diff_lines: 50,
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("score_updated"));
}

#[tokio::test]
async fn event_bus_sender_clones_correctly() {
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let mut rx = bus.subscribe();
    let sender = bus.sender();

    let task_id = TaskId::new();
    let _ = sender.send(AgentEvent::TaskCancelled { task_id });

    let ev = rx.try_recv().unwrap();
    assert!(matches!(ev, AgentEvent::TaskCancelled { .. }));
}

#[test]
fn task_started_event_fields() {
    let task_id = TaskId::new();
    let ev = AgentEvent::TaskStarted {
        task_id,
        attempt: 1,
        branch: "feat/lopi-abc123".to_string(),
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("task_started"));
    assert!(json.contains("feat/lopi-abc123"));
}

#[test]
fn status_changed_event_fields() {
    let task_id = TaskId::new();
    let ev = AgentEvent::StatusChanged {
        task_id,
        status: TaskStatus::Planning,
        attempt: 1,
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("status_changed"));
}

#[test]
fn task_completed_event_fields() {
    let task_id = TaskId::new();
    let ev = AgentEvent::TaskCompleted {
        task_id,
        outcome: TaskStatus::Success {
            branch: "feat/lopi-fix".to_string(),
            pr_url: Some("https://github.com/org/repo/pull/42".to_string()),
        },
        total_attempts: 2,
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("task_completed"));
}
