//! Test runner — execute the repo's test suite and map results back to
//! `SpecItem` names so the gap-fill command knows which spec items need work.
//!
//! Supported runners (auto-detected from directory contents):
//! - `cargo test --no-fail-fast` for Rust repos (Cargo.toml present)
//! - `python -m pytest --tb=no -q` for Python repos (setup.py / pyproject.toml)
//!
//! The output is parsed line-by-line: no JSON schema required. Rust's `cargo
//! test` emits `test name ... FAILED` / `ok`; pytest emits `PASSED` / `FAILED`
//! per test line in verbose mode.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

/// Outcome for a single test function.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestRunResult {
    /// Test function name — matches `SpecItem::name` for cross-reference.
    pub name: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Captured failure output (empty when `passed == true`).
    pub error: Option<String>,
}

/// Run the repo's tests and return per-test pass/fail results.
///
/// # Errors
///
/// Returns an error if the test runner cannot be spawned.
pub async fn run_tests(repo_path: impl AsRef<Path>) -> Result<Vec<TestRunResult>> {
    let root = repo_path.as_ref();
    if root.join("Cargo.toml").exists() {
        run_cargo(root).await
    } else if root.join("setup.py").exists()
        || root.join("pyproject.toml").exists()
        || root.join("setup.cfg").exists()
    {
        run_pytest(root).await
    } else {
        tracing::warn!("no supported test runner found in {}", root.display());
        Ok(vec![])
    }
}

/// Return the names of spec items that are NOT passing.
///
/// Matched by name: a `SpecItem` is considered failing when a `TestRunResult`
/// with the same name exists and `passed == false`.  Items with no run record
/// are also considered gaps (never ran → could be missing or broken).
pub fn coverage_gaps<'a>(
    spec_items: &'a [crate::SpecItem],
    results: &[TestRunResult],
) -> Vec<&'a crate::SpecItem> {
    let passing: std::collections::HashSet<_> = results
        .iter()
        .filter(|r| r.passed)
        .map(|r| r.name.as_str())
        .collect();
    spec_items
        .iter()
        .filter(|i| !passing.contains(i.name.as_str()))
        .collect()
}

async fn run_cargo(root: &Path) -> Result<Vec<TestRunResult>> {
    let out = Command::new("cargo")
        .args(["test", "--no-fail-fast", "--", "--test-output", "immediate"])
        .env("RUSTC_WRAPPER", "sccache")
        .current_dir(root)
        .output()
        .await?;

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    Ok(parse_cargo_output(&combined))
}

async fn run_pytest(root: &Path) -> Result<Vec<TestRunResult>> {
    let out = Command::new("python")
        .args(["-m", "pytest", "--tb=no", "-v", "--no-header"])
        .current_dir(root)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(parse_pytest_output(&stdout))
}

/// Parse `cargo test` output.
///
/// Lines of interest:
/// - `test module::name ... ok` — passed
/// - `test module::name ... FAILED` — failed
pub(crate) fn parse_cargo_output(output: &str) -> Vec<TestRunResult> {
    let mut results = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("test ") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("test ") {
            if let Some(name_end) = rest.rfind(" ... ") {
                let name = rest[..name_end].trim();
                let verdict = &rest[name_end + 5..];
                // Extract just the function name (last component after ::)
                let short = name.rsplit("::").next().unwrap_or(name);
                results.push(TestRunResult {
                    name: short.to_string(),
                    passed: verdict.starts_with("ok"),
                    error: if verdict.starts_with("FAILED") {
                        Some(format!("test {name} FAILED"))
                    } else {
                        None
                    },
                });
            }
        }
    }
    results
}

/// Parse `pytest -v` output.
///
/// Lines of interest:
/// - `tests/test_foo.py::test_bar PASSED` — passed
/// - `tests/test_foo.py::test_bar FAILED` — failed
pub(crate) fn parse_pytest_output(output: &str) -> Vec<TestRunResult> {
    let mut results = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("::") && (trimmed.ends_with("PASSED") || trimmed.ends_with("FAILED")) {
            let passed = trimmed.ends_with("PASSED");
            // Extract function name: last component after ::
            if let Some(pos) = trimmed.rfind("::") {
                let after = &trimmed[pos + 2..];
                let name = after.split_whitespace().next().unwrap_or(after);
                results.push(TestRunResult {
                    name: name.to_string(),
                    passed,
                    error: if !passed {
                        Some(trimmed.to_string())
                    } else {
                        None
                    },
                });
            }
        }
    }
    results
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn cargo_ok_test() {
        let out = "test core::score_weighted ... ok\n";
        let results = parse_cargo_output(out);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "score_weighted");
        assert!(results[0].passed);
    }

    #[test]
    fn cargo_failed_test() {
        let out = "test runner::run_loop::tests::basic_run ... FAILED\n";
        let results = parse_cargo_output(out);
        assert_eq!(results[0].name, "basic_run");
        assert!(!results[0].passed);
        assert!(results[0].error.is_some());
    }

    #[test]
    fn cargo_multiple_results() {
        let out = "test a::b::pass_me ... ok\ntest a::b::fail_me ... FAILED\n";
        let results = parse_cargo_output(out);
        assert_eq!(results.len(), 2);
        assert!(results[0].passed);
        assert!(!results[1].passed);
    }

    #[test]
    fn pytest_passed() {
        let out = "tests/test_api.py::test_health PASSED\n";
        let results = parse_pytest_output(out);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test_health");
        assert!(results[0].passed);
    }

    #[test]
    fn pytest_failed() {
        let out = "tests/test_api.py::test_broken FAILED\n";
        let results = parse_pytest_output(out);
        assert!(!results[0].passed);
        assert!(results[0].error.is_some());
    }

    #[test]
    fn coverage_gaps_returns_failing_items() {
        use crate::{SpecItem, SpecKind};
        let items = vec![
            SpecItem {
                name: "test_a".into(),
                description: "a".into(),
                kind: SpecKind::RustTest,
                file: "x.rs".into(),
                line: 1,
            },
            SpecItem {
                name: "test_b".into(),
                description: "b".into(),
                kind: SpecKind::RustTest,
                file: "x.rs".into(),
                line: 2,
            },
        ];
        let results = vec![
            TestRunResult {
                name: "test_a".into(),
                passed: true,
                error: None,
            },
            TestRunResult {
                name: "test_b".into(),
                passed: false,
                error: Some("FAILED".into()),
            },
        ];
        let gaps = coverage_gaps(&items, &results);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].name, "test_b");
    }

    #[test]
    fn coverage_gaps_missing_run_is_gap() {
        use crate::{SpecItem, SpecKind};
        let items = vec![SpecItem {
            name: "test_c".into(),
            description: "c".into(),
            kind: SpecKind::RustTest,
            file: "x.rs".into(),
            line: 3,
        }];
        // No test results at all — test_c was never run → gap
        let gaps = coverage_gaps(&items, &[]);
        assert_eq!(gaps.len(), 1);
    }

    #[test]
    fn cargo_skips_non_test_lines() {
        let out = "running 2 tests\ntest a ... ok\ntest result: ok. 1 passed; 0 failed";
        let results = parse_cargo_output(out);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "a");
    }
}
