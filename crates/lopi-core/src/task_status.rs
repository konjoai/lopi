//! [`TaskStatus`] — split out of `task.rs` purely to keep that file under
//! the 500-line CI file-size gate as AVO-Supervisor-2 (Feature 2) added the
//! `Stuck` variant; same rationale as `task_source.rs`'s split. Re-exported
//! from `task.rs` unchanged so every existing `task::TaskStatus` path stays
//! valid.

use serde::{Deserialize, Serialize};

/// Current lifecycle status of a [`crate::Task`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// Task is waiting in the queue for an available agent.
    Queued,
    /// Agent is generating an implementation plan.
    Planning,
    /// Plan generated; paused awaiting human approval before implementation
    /// (Phase 11 — plan approval gate). The proposed plan rides on the
    /// accompanying [`crate::AgentEvent::PlanProposed`] event.
    AwaitingPlanApproval {
        /// Attempt whose plan is pending approval.
        attempt: u8,
    },
    /// Agent is applying code changes.
    Implementing,
    /// Agent is running the test suite.
    Testing,
    /// Agent is evaluating test and lint results.
    Scoring,
    /// Task is being retried after a previous failed attempt.
    Retrying {
        /// Attempt number of the upcoming retry.
        attempt: u8,
    },
    /// AVO-Supervisor-2 (Feature 2) — still running, but this attempt is
    /// thrashing (near-identical diff to the last one) or plateaued (no
    /// score gain for several attempts). Distinct from `Retrying`; never
    /// terminal on its own — see `runner::stall`.
    Stuck {
        /// Attempt number the stall was detected on.
        attempt: u8,
        /// Short machine tag for the dashboard badge: `"plateau"`,
        /// `"thrash"`, or `"plateau+thrash"`.
        reason: String,
    },
    /// Task completed successfully and a branch (and optionally PR) was created.
    Success {
        /// Branch containing the successful changes.
        branch: String,
        /// URL of the opened pull request, if auto-PR is enabled.
        pr_url: Option<String>,
    },
    /// Task failed after exhausting all retry attempts.
    Failed {
        /// Human-readable description of why the task failed.
        reason: String,
    },
    /// Changes were rolled back after a failure.
    RolledBack,
    /// A pre-PR rebase onto the advanced default branch hit conflicts, so the
    /// task stopped rather than force an unsafe merge. Carries the conflicting
    /// paths so an operator — or a follow-up task — knows exactly what collided.
    Conflict {
        /// Repository-relative paths that conflicted during the rebase.
        paths: Vec<String>,
    },
}

impl TaskStatus {
    /// Canonical, machine-readable status string for durable persistence and
    /// the JSON API / WebSocket snapshot.
    ///
    /// This is the single lifecycle vocabulary that the store, the REST API,
    /// and the web client's status bucketing all agree on. Unlike a
    /// human-facing display label, it never embeds a reason, emoji, attempt
    /// count, or branch — persisting one of those (as the CLI/REPL paths once
    /// did via a display formatter) produced compound values like
    /// `"failed ❌ Cancelled"` that no consumer could bucket. Every write to
    /// the `tasks.status` column must go through this so a fresh page load,
    /// which only has the snapshot to read, buckets terminal tasks correctly.
    #[must_use]
    pub fn db_status(&self) -> &'static str {
        match self {
            TaskStatus::Queued => "queued",
            TaskStatus::Planning => "planning",
            TaskStatus::AwaitingPlanApproval { .. } => "awaiting_plan_approval",
            TaskStatus::Implementing => "implementing",
            TaskStatus::Testing => "testing",
            TaskStatus::Scoring => "scoring",
            TaskStatus::Retrying { .. } => "retrying",
            TaskStatus::Stuck { .. } => "stuck",
            TaskStatus::Success { .. } => "success",
            TaskStatus::Failed { .. } => "failed",
            TaskStatus::RolledBack => "rolled_back",
            TaskStatus::Conflict { .. } => "conflict",
        }
    }
}
