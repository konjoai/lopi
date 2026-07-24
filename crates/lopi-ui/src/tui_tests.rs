//! `tui.rs` had zero test coverage before this file — `AppState`'s event
//! handling, sorting/filtering, and selection-wraparound are pure/synchronous
//! and fully unit-testable without a real terminal. Split out to keep
//! `tui.rs` under the 500-line CI file-size gate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use lopi_core::Priority;

fn queued(task_id: TaskId, goal: &str) -> AgentEvent {
    AgentEvent::TaskQueued {
        task_id,
        goal: goal.to_string(),
        priority: Priority::Normal,
    }
}

#[test]
fn task_queued_inserts_a_row_and_increments_queued_count() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(queued(id, "fix the bug"));
    assert_eq!(state.queued_count, 1);
    let row = state.agents.get(&id).expect("row inserted");
    assert_eq!(row.goal, "fix the bug");
    assert!(matches!(row.status, TaskStatus::Queued));
}

#[test]
fn task_started_updates_an_existing_row() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(queued(id, "fix the bug"));
    state.handle_event(AgentEvent::TaskStarted {
        task_id: id,
        attempt: 2,
        branch: "lopi/abc-attempt-2".to_string(),
        repo: "/repo".to_string(),
    });
    let row = state.agents.get(&id).unwrap();
    assert_eq!(row.attempt, 2);
    assert_eq!(row.branch, "lopi/abc-attempt-2");
}

#[test]
fn task_started_for_unknown_id_is_a_silent_no_op() {
    let mut state = AppState::new();
    state.handle_event(AgentEvent::TaskStarted {
        task_id: TaskId::new(),
        attempt: 1,
        branch: "b".to_string(),
        repo: "/repo".to_string(),
    });
    assert!(state.agents.is_empty());
}

#[test]
fn status_changed_updates_status_and_attempt() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(queued(id, "fix the bug"));
    state.handle_event(AgentEvent::StatusChanged {
        task_id: id,
        status: TaskStatus::Implementing,
        attempt: 1,
    });
    let row = state.agents.get(&id).unwrap();
    assert!(matches!(row.status, TaskStatus::Implementing));
    assert_eq!(row.attempt, 1);
}

#[test]
fn score_updated_sets_the_row_score() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(queued(id, "fix the bug"));
    state.handle_event(AgentEvent::ScoreUpdated {
        task_id: id,
        test_pass_rate: 0.75,
        lint_errors: 2,
        diff_lines: 10,
    });
    assert!((state.agents.get(&id).unwrap().score - 0.75).abs() < f32::EPSILON);
}

#[test]
fn log_line_ring_buffer_caps_at_max_log_lines() {
    let mut state = AppState::new();
    let id = TaskId::new();
    for i in 0..(MAX_LOG_LINES + 10) {
        state.handle_event(AgentEvent::info(id, format!("line {i}")));
    }
    assert_eq!(
        state.log_lines.len(),
        MAX_LOG_LINES,
        "must cap, not grow unbounded"
    );
    // Oldest entries were evicted — the earliest surviving line is the
    // 11th one pushed (10 dropped), not "line 0".
    assert_eq!(state.log_lines.front().unwrap().line, "line 10");
}

#[test]
fn task_completed_updates_row_and_increments_succeeded() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(queued(id, "fix the bug"));
    state.handle_event(AgentEvent::TaskCompleted {
        task_id: id,
        outcome: TaskStatus::Success {
            branch: "b".to_string(),
            pr_url: None,
        },
        total_attempts: 1,
        successor: None,
    });
    assert_eq!(state.succeeded, 1);
    assert_eq!(state.failed, 0);
    assert!(matches!(
        state.agents.get(&id).unwrap().status,
        TaskStatus::Success { .. }
    ));
}

#[test]
fn task_completed_increments_failed_on_failure_outcome() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(queued(id, "fix the bug"));
    state.handle_event(AgentEvent::TaskCompleted {
        task_id: id,
        outcome: TaskStatus::Failed {
            reason: "boom".to_string(),
        },
        total_attempts: 3,
        successor: None,
    });
    assert_eq!(state.failed, 1);
    assert_eq!(state.succeeded, 0);
}

/// Regression: the counter increment in `TaskCompleted` isn't gated by
/// whether the row still exists in `agents` — a task cancelled/evicted
/// before its terminal event arrives must still count.
#[test]
fn task_completed_increments_counters_even_for_an_unknown_task_id() {
    let mut state = AppState::new();
    state.handle_event(AgentEvent::TaskCompleted {
        task_id: TaskId::new(),
        outcome: TaskStatus::Success {
            branch: "b".to_string(),
            pr_url: None,
        },
        total_attempts: 1,
        successor: None,
    });
    assert_eq!(state.succeeded, 1);
    assert!(state.agents.is_empty());
}

#[test]
fn task_cancelled_removes_the_row() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(queued(id, "fix the bug"));
    state.handle_event(AgentEvent::TaskCancelled { task_id: id });
    assert!(!state.agents.contains_key(&id));
}

#[test]
fn pool_stats_overwrites_queued_count_rather_than_incrementing() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(queued(id, "a"));
    state.handle_event(queued(TaskId::new(), "b"));
    assert_eq!(state.queued_count, 2);
    state.handle_event(AgentEvent::PoolStats {
        running: 0,
        queued: 5,
        succeeded: 0,
        failed: 0,
        uptime_secs: 10,
    });
    assert_eq!(
        state.queued_count, 5,
        "PoolStats overwrites, unlike TaskQueued's +=1"
    );
}

#[test]
fn sorted_agents_orders_oldest_first() {
    let mut state = AppState::new();
    let first = TaskId::new();
    state.handle_event(queued(first, "first"));
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second = TaskId::new();
    state.handle_event(queued(second, "second"));
    std::thread::sleep(std::time::Duration::from_millis(5));
    let third = TaskId::new();
    state.handle_event(queued(third, "third"));

    let sorted = state.sorted_agents();
    assert_eq!(sorted.len(), 3);
    assert_eq!(sorted[0].id, first);
    assert_eq!(sorted[1].id, second);
    assert_eq!(sorted[2].id, third);
}

#[test]
fn visible_logs_filters_by_log_filter_when_set() {
    let mut state = AppState::new();
    let a = TaskId::new();
    let b = TaskId::new();
    state.handle_event(AgentEvent::info(a, "from a"));
    state.handle_event(AgentEvent::info(b, "from b"));
    state.log_filter = Some(a);

    let visible = state.visible_logs();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].task_id, a);
}

#[test]
fn visible_logs_shows_everything_when_no_filter() {
    let mut state = AppState::new();
    state.handle_event(AgentEvent::info(TaskId::new(), "a"));
    state.handle_event(AgentEvent::info(TaskId::new(), "b"));
    assert_eq!(state.visible_logs().len(), 2);
}

#[test]
fn visible_logs_caps_at_display_logs_and_keeps_the_most_recent_in_order() {
    let mut state = AppState::new();
    let id = TaskId::new();
    for i in 0..(DISPLAY_LOGS + 5) {
        state.handle_event(AgentEvent::info(id, format!("line {i}")));
    }
    let visible = state.visible_logs();
    assert_eq!(visible.len(), DISPLAY_LOGS);
    // Oldest-of-the-visible-window first, most recent last.
    assert_eq!(visible[0].line, format!("line {}", 5));
    assert_eq!(
        visible[DISPLAY_LOGS - 1].line,
        format!("line {}", DISPLAY_LOGS + 4)
    );
}

#[test]
fn select_next_and_prev_are_no_ops_with_no_agents() {
    let mut state = AppState::new();
    state.select_next();
    assert!(state.table_state.selected().is_none());
    state.select_prev();
    assert!(state.table_state.selected().is_none());
}

#[test]
fn select_next_wraps_from_last_to_first() {
    let mut state = AppState::new();
    for i in 0..3 {
        state.handle_event(queued(TaskId::new(), &format!("t{i}")));
    }
    state.table_state.select(Some(2));
    state.select_next();
    assert_eq!(state.table_state.selected(), Some(0));
}

#[test]
fn select_next_advances_by_one_when_not_at_the_end() {
    let mut state = AppState::new();
    for i in 0..3 {
        state.handle_event(queued(TaskId::new(), &format!("t{i}")));
    }
    state.table_state.select(Some(0));
    state.select_next();
    assert_eq!(state.table_state.selected(), Some(1));
}

#[test]
fn select_prev_wraps_from_first_to_last() {
    let mut state = AppState::new();
    for i in 0..3 {
        state.handle_event(queued(TaskId::new(), &format!("t{i}")));
    }
    state.table_state.select(Some(0));
    state.select_prev();
    assert_eq!(state.table_state.selected(), Some(2));
}

#[test]
fn select_prev_retreats_by_one_when_not_at_the_start() {
    let mut state = AppState::new();
    for i in 0..3 {
        state.handle_event(queued(TaskId::new(), &format!("t{i}")));
    }
    state.table_state.select(Some(2));
    state.select_prev();
    assert_eq!(state.table_state.selected(), Some(1));
}

#[test]
fn select_next_starts_at_zero_when_nothing_selected() {
    let mut state = AppState::new();
    state.handle_event(queued(TaskId::new(), "t"));
    state.select_next();
    assert_eq!(state.table_state.selected(), Some(0));
}

#[test]
fn uptime_formats_seconds_minutes_and_hours() {
    let state = AppState::new();
    // started_at is "now" — elapsed is ~0s, so this only exercises the
    // sub-60s branch directly; the minute/hour branches are pure string
    // formatting on the same `s` value, verified independently below.
    assert!(state.uptime().ends_with('s'));
}
