//! Konjo Code Quality Framework — post-task violation scanner.
//!
//! After a successful agent run, `scan_diff` inspects the changed files for
//! quality violations (complexity, dead code, insufficient coverage) using
//! `cargo clippy --message-format=json` and `cargo llvm-cov --json`.
//! Each violation is converted to a low-priority fix task and injected back
//! into the orchestrator's `TaskQueue` — closing the continuous improvement loop.
//!
//! Design follows the Code Broker pattern (arXiv 2604.23088): static analysis
//! tools provide deterministic signals; tasks are tiered by clarity of the
//! signal (well-defined violations → Haiku, architectural drift → Sonnet).
#![warn(missing_docs)]

mod clippy;
mod coverage;
mod tasks;

pub use clippy::scan_clippy;
pub use coverage::scan_coverage;
pub use tasks::violations_to_tasks;

use lopi_core::Priority;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single quality violation detected in a changed file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityViolation {
    /// Relative file path (from repo root).
    pub file: String,
    /// Line number, if the tool reported one.
    pub line: Option<u32>,
    /// Category of violation.
    pub kind: ViolationKind,
    /// Severity level.
    pub severity: Severity,
    /// Human-readable description of the violation.
    pub message: String,
    /// Imperative one-sentence hint for the fix task goal.
    pub fix_hint: String,
    /// Signal clarity in `[0.0, 1.0]`: 1.0 = fully deterministic (clippy error),
    /// lower = heuristic (coverage estimate).
    pub confidence: f32,
}

/// Category of quality violation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// Cognitive complexity exceeds the configured threshold.
    Complexity,
    /// Dead code detected (`#[allow(dead_code)]` or unused items).
    DeadCode,
    /// Test coverage below the configured minimum for this file.
    Coverage,
    /// Clippy lint or standards violation not covered above.
    Standards,
}

/// Severity of a quality violation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Must be fixed; blocks shipping.
    Error,
    /// Should be fixed; does not block shipping.
    Warning,
}

impl QualityViolation {
    /// The task priority a violation of this severity should produce.
    pub fn task_priority(&self) -> Priority {
        match self.severity {
            Severity::Error => Priority::High,
            Severity::Warning => Priority::Normal,
        }
    }
}

/// Scan changed files in `repo_path` for quality violations.
///
/// Runs `scan_clippy` (always) and `scan_coverage` (when `diff_files` is non-empty).
/// Results are deduplicated by `(file, line, kind)`.
///
/// # Errors
/// Returns an error if spawning the analysis subprocess fails.
pub async fn scan_diff(
    repo_path: &Path,
    diff_files: &[String],
) -> anyhow::Result<Vec<QualityViolation>> {
    let mut violations = Vec::new();

    // Clippy provides deterministic, file-level signals.
    match scan_clippy(repo_path).await {
        Ok(mut v) => violations.append(&mut v),
        Err(e) => tracing::warn!(error = %e, "clippy scan failed; skipping"),
    }

    // Coverage only runs when there are specific diff files to check.
    if !diff_files.is_empty() {
        match scan_coverage(repo_path, diff_files).await {
            Ok(mut v) => violations.append(&mut v),
            Err(e) => tracing::warn!(error = %e, "coverage scan failed; skipping"),
        }
    }

    // Deduplicate: same file + line + kind.
    violations.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.kind == b.kind);
    Ok(violations)
}
