//! Synthetic `AgentEvent` replay for `lopi watch --demo` (demo-measurement
//! sprint).
//!
//! `lopi_ui::tui::run` is purely event-bus driven — it never reads the
//! store, unlike the web dashboard, which reads it directly on every
//! request (see `docs/adr/0001-demo-mode-and-measurement.md` point 7). So a
//! demo TUI session needs a one-time seed of synthetic `AgentEvent`s
//! reflecting the *same* task list [`crate::generate`] persisted, built
//! from the same `generator_content::build_task_plans` this
//! module shares with the generator — that's what keeps the two surfaces
//! showing the same picture without re-reading the DB.

use chrono::Utc;
use lopi_core::{AgentEvent, LogLevel, Priority, TaskStatus};

use crate::content;
use crate::generator_content::{build_task_plans, TaskOutcome, TaskPlan};

/// Build a synthetic `AgentEvent` sequence reflecting the same deterministic
/// task plan `generate(dest, real, seed)` would persist for `seed`, so a
/// `lopi watch --demo` TUI session (purely event-driven, never reads the
/// store) shows the same picture as the web dashboard. Feed the returned
/// events into `lopi_ui::tui`'s `AppState` directly (a later change wires
/// this) — do NOT `EventBus::send` them before a subscriber exists, they'd
/// be silently dropped by the broadcast channel.
///
/// Deterministic in *content* for a given seed (same goal/status/log text);
/// event timestamps use `chrono::Utc::now()` at call time, matching the
/// "timestamps are real, content is seeded" rule from the ADR.
#[must_use]
pub fn replay_events(seed: u64) -> Vec<AgentEvent> {
    let plans = build_task_plans(seed);
    let mut events = Vec::with_capacity(plans.len() * 3);
    for plan in &plans {
        events.push(AgentEvent::TaskQueued {
            task_id: plan.id,
            goal: plan.goal.clone(),
            priority: Priority::Normal,
        });

        if plan.outcome == TaskOutcome::Queued {
            continue;
        }

        let repo = content::REPOS[plan.repo_index].path.to_string();
        let branch = format!("lopi/{}-attempt-1", plan.id);
        events.push(AgentEvent::TaskStarted {
            task_id: plan.id,
            attempt: 1,
            branch,
            repo,
        });

        let status = outcome_task_status(plan);
        events.push(AgentEvent::StatusChanged {
            task_id: plan.id,
            status: status.clone(),
            attempt: 1,
        });

        push_log_lines(&mut events, plan);

        if plan.outcome.is_terminal() {
            events.push(AgentEvent::TaskCompleted {
                task_id: plan.id,
                outcome: status,
                total_attempts: 1,
                successor: None,
            });
        }
    }
    events
}

/// Map a [`TaskOutcome`] to the granular (`Running`) or terminal
/// [`TaskStatus`] it corresponds to for event-replay purposes.
fn outcome_task_status(plan: &TaskPlan) -> TaskStatus {
    match plan.outcome {
        TaskOutcome::Queued => TaskStatus::Queued,
        TaskOutcome::Running => TaskStatus::Implementing,
        TaskOutcome::Success => TaskStatus::Success {
            branch: format!("lopi/{}-attempt-1", plan.id),
            pr_url: None,
        },
        TaskOutcome::Failed => TaskStatus::Failed {
            reason: "exhausted max_retries — see task logs".to_string(),
        },
        TaskOutcome::RolledBack => TaskStatus::RolledBack,
        TaskOutcome::Conflict => TaskStatus::Conflict {
            paths: vec!["src/lib.rs".to_string()],
        },
    }
}

/// Push a couple of `LogLine` events for a non-queued task, reusing
/// `content::LOG_LINE_TEMPLATES`.
fn push_log_lines(events: &mut Vec<AgentEvent>, plan: &TaskPlan) {
    let stage = if plan.outcome == TaskOutcome::Running {
        "implement"
    } else {
        "score"
    };
    for template in content::LOG_LINE_TEMPLATES.iter().take(2) {
        events.push(AgentEvent::LogLine {
            task_id: plan.id,
            line: template.replace("{stage}", stage),
            level: LogLevel::Info,
            ts: Utc::now(),
        });
    }
}
