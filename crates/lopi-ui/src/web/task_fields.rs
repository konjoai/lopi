//! `CreateTaskRequest` → `Task` field mapping, plus goal validation at the
//! API boundary. Split out of `handlers.rs` to keep that file under the
//! 500-line CI file-size gate.

use super::types::{CreateTaskRequest, MAX_GOAL_LENGTH};
use lopi_core::{PermissionMode, PermissionModeError, ReportChannel, ReportChannelError, Task};

/// Why [`apply_loop_fields`] rejected a `CreateTaskRequest` — every variant
/// maps to a 422, never a silent drop or coercion of the offending field.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(super) enum ApplyLoopFieldsError {
    /// `req.report` named an unknown or currently-unreachable channel.
    #[error(transparent)]
    Report(#[from] ReportChannelError),
    /// `req.permission_mode` named anything other than the four headless-safe
    /// modes [`PermissionMode`] exposes.
    #[error(transparent)]
    PermissionMode(#[from] PermissionModeError),
}

/// Apply the loop/verifier/report/override fields exposed on
/// [`CreateTaskRequest`] onto a freshly constructed `Task`. Kept separate
/// from `create_task` so the field-mapping contract is unit-testable
/// without an HTTP round-trip.
///
/// # Errors
/// Returns [`ApplyLoopFieldsError::Report`] when `req.report` names an
/// unknown or currently-unreachable channel (e.g. `"whatsapp"`) — reuses
/// [`ReportChannel::parse`], the same validator `Task`/`ScheduleEntry`
/// already use, rather than a second report-channel parser. Returns
/// [`ApplyLoopFieldsError::PermissionMode`] when `req.permission_mode` names
/// anything other than the four headless-safe modes, reusing
/// [`PermissionMode::parse`] the same way.
pub(super) fn apply_loop_fields(
    task: &mut Task,
    req: &CreateTaskRequest,
) -> Result<(), ApplyLoopFieldsError> {
    if let Some(report) = &req.report {
        ReportChannel::parse(report)?;
        task.report = Some(report.clone());
    }
    if let Some(mode) = &req.permission_mode {
        task.permission_mode = PermissionMode::parse(mode)?;
    }
    if let Some(v) = req.verifier_required {
        task.verifier_required = v;
    }
    if let Some(m) = &req.verifier_model {
        task.verifier_model = Some(m.clone());
    }
    if let Some(e) = &req.verifier_effort {
        task.verifier_effort = Some(e.clone());
    }
    if let Some(n) = req.max_iterations {
        task.max_iterations = Some(n);
    }
    if let Some(a) = req.autonomy_level {
        task.autonomy_level = Some(a);
    }
    if let Some(n) = req.no_progress_limit {
        task.no_progress_limit = Some(n);
    }
    if let Some(i) = req.isolation {
        task.isolation = Some(i);
    }
    if let Some(m) = &req.model {
        task.model = Some(m.clone());
    }
    if let Some(e) = &req.effort {
        task.effort = Some(e.clone());
    }
    if let Some(d) = req.deliverable {
        task.deliverable = Some(d);
    }
    if let Some(g) = &req.gate {
        task.gate = Some(g.clone());
    }
    if let Some(u) = &req.until {
        task.until = Some(u.clone());
    }
    if let Some(f) = req.on_fail {
        task.on_fail = Some(f);
    }
    if let Some(a) = &req.acceptance {
        task.acceptance = Some(a.clone());
    }
    if let Some(fo) = req.verifier_fail_open {
        task.verifier_fail_open = fo;
    }
    if let Some(b) = req.budget_tokens {
        task.budget_tokens = b;
    }
    if let Some(bo) = &req.budget_override {
        task.budget_override = Some(bo.clone());
    }
    Ok(())
}

/// Validate a submitted goal at the API boundary, per `.claude/rules/security-invariants.md`
/// ("max goal length, character set constraints"). Rejects:
/// - empty or whitespace-only goals (Ops-2 bug #5 — `{"goal":""}` spawned a real
///   agent),
/// - goals longer than [`MAX_GOAL_LENGTH`] characters,
/// - goals carrying C0/C1 control characters other than the ordinary
///   `\n` / `\r` / `\t` whitespace — NUL and ANSI escape sequences have no place
///   in a natural-language goal and are a log-poisoning / injection vector.
///
/// Pure and separate from `create_task` so the boundary contract is
/// table-testable without an HTTP round-trip. Returns the human-readable
/// rejection reason on failure.
pub(super) fn validate_goal(goal: &str) -> Result<(), String> {
    if goal.trim().is_empty() {
        return Err("goal must not be empty".to_string());
    }
    if goal.chars().count() > MAX_GOAL_LENGTH {
        return Err(format!("goal too long (max {MAX_GOAL_LENGTH} chars)"));
    }
    super::types::reject_control_chars(goal)
}
