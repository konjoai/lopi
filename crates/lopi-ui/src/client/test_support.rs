//! Shared test-only fixtures for `client` submodule tests. Split out so
//! `local_tests.rs` and `remote_tests.rs` don't each hand-write an
//! identical all-`None` `CreateTaskRequest` literal (a DRY violation the
//! pre-commit hook's duplicate-block check caught).
#![cfg(test)]

use crate::web::types::CreateTaskRequest;

/// A minimal `CreateTaskRequest` with every optional field unset except
/// `goal` and a single-pass `max_iterations` — the common starting point
/// both `LocalClient` and `RemoteClient`'s round-trip tests build on.
pub(super) fn bare_create_task_request(goal: &str) -> CreateTaskRequest {
    CreateTaskRequest {
        goal: goal.to_string(),
        repo: None,
        priority: None,
        constraints: None,
        allowed_dirs: None,
        forbidden_dirs: None,
        max_retries: None,
        require_plan_approval: None,
        verifier_required: None,
        verifier_model: None,
        verifier_effort: None,
        report: None,
        max_iterations: Some(1),
        autonomy_level: None,
        no_progress_limit: None,
        isolation: None,
        model: None,
        effort: None,
        permission_mode: None,
        deliverable: None,
        gate: None,
        until: None,
        on_fail: None,
        client_ref: None,
        acceptance: None,
        verifier_fail_open: None,
        budget_tokens: None,
        budget_override: None,
    }
}
