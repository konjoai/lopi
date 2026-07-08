//! Shared `#[cfg(test)]` `Task` builder for prompt-building test modules.
//!
//! `runner::api_plan`'s and `stability`'s test modules each hand-rolled an
//! identical `Task { .. }` literal (single-source-of-truth violation flagged
//! by `dry_check.py`). Both need a `Task` with fixed, minimal `allowed_dirs`/
//! `forbidden_dirs` — deliberately shorter than `Task::new`'s defaults so
//! prompt-section assertions (`"# Allowed dirs"` etc.) stay exact — so this
//! is the one place that literal is written.

#![cfg(test)]

use lopi_core::Task;

/// Build a minimal `Task` for prompt-building tests.
///
/// Delegates to `Task::new` for every default (id, priority, max_retries,
/// timestamps, verifier/report/model fields, ...) so this fixture can never
/// drift from the real constructor, then narrows `allowed_dirs` to `["src/"]`
/// and `forbidden_dirs` to `[".github/"]` — shorter than `Task::new`'s
/// defaults — so callers asserting on prompt section contents get a stable,
/// minimal fixture.
pub(crate) fn make_test_task(goal: &str, constraints: Vec<String>) -> Task {
    let mut task = Task::new(goal);
    task.constraints = constraints;
    task.allowed_dirs = vec!["src/".into()];
    task.forbidden_dirs = vec![".github/".into()];
    task
}
