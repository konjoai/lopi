//! Coverage scanner: uses `cargo llvm-cov --json --summary-only` to find
//! files in the diff with coverage below the 80% gate.
use crate::{QualityViolation, Severity, ViolationKind};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use tokio::process::Command;

/// Minimum line coverage required per file.
const COVERAGE_GATE: f64 = 80.0;

/// Run `cargo llvm-cov --json --summary-only` and return coverage violations
/// for files in `diff_files` that fall below `COVERAGE_GATE`.
///
/// # Errors
/// Returns an error if the subprocess cannot be spawned or the output cannot be parsed.
pub async fn scan_coverage(
    repo_path: &Path,
    diff_files: &[String],
) -> Result<Vec<QualityViolation>> {
    let output = Command::new("cargo")
        .args(["llvm-cov", "--json", "--summary-only"])
        .current_dir(repo_path)
        .output()
        .await
        .context("spawning cargo llvm-cov")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_coverage_json(&stdout, diff_files)
}

/// Parse `cargo llvm-cov --json --summary-only` output.
///
/// The JSON has the shape:
/// `{ "data": [{ "files": [{ "filename": "...", "summary": { "lines": { "percent": 75.0 } } }] }] }`
pub(crate) fn parse_coverage_json(
    json: &str,
    diff_files: &[String],
) -> Result<Vec<QualityViolation>> {
    if json.trim().is_empty() {
        return Ok(vec![]);
    }

    let report: CoverageReport = match serde_json::from_str(json) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(error = %e, "coverage JSON parse failed — llvm-cov may not be installed");
            return Ok(vec![]);
        }
    };

    let mut violations = Vec::new();
    for entry in report.data {
        for file in entry.files {
            // Only report on files that were changed in this diff.
            if !diff_files
                .iter()
                .any(|df| file.filename.ends_with(df.as_str()))
            {
                continue;
            }
            let pct = file.summary.lines.percent;
            if pct < COVERAGE_GATE {
                violations.push(QualityViolation {
                    file: file.filename.clone(),
                    line: None,
                    kind: ViolationKind::Coverage,
                    severity: Severity::Warning,
                    message: format!("{:.1}% line coverage (gate: {COVERAGE_GATE}%)", pct),
                    fix_hint: format!(
                        "Add tests to bring {filename} from {pct:.0}% to {COVERAGE_GATE}% coverage",
                        filename = file.filename,
                    ),
                    confidence: 0.8,
                });
            }
        }
    }

    Ok(violations)
}

// ── llvm-cov JSON types ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CoverageReport {
    data: Vec<CoverageData>,
}

#[derive(Deserialize)]
struct CoverageData {
    files: Vec<CoverageFile>,
}

#[derive(Deserialize)]
struct CoverageFile {
    filename: String,
    summary: CoverageSummary,
}

#[derive(Deserialize)]
struct CoverageSummary {
    lines: CoverageMetric,
}

#[derive(Deserialize)]
struct CoverageMetric {
    percent: f64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn empty_output_returns_empty() {
        let v = parse_coverage_json("", &["src/main.rs".to_string()]).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn malformed_json_returns_empty() {
        let v = parse_coverage_json("not json", &["src/main.rs".to_string()]).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn file_above_gate_not_reported() {
        let json = r#"{
            "data": [{"files": [{
                "filename": "src/main.rs",
                "summary": {"lines": {"percent": 90.0}}
            }]}]
        }"#;
        let v = parse_coverage_json(json, &["src/main.rs".to_string()]).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn file_below_gate_in_diff_is_reported() {
        let json = r#"{
            "data": [{"files": [{
                "filename": "src/lib.rs",
                "summary": {"lines": {"percent": 60.0}}
            }]}]
        }"#;
        let v = parse_coverage_json(json, &["src/lib.rs".to_string()]).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, ViolationKind::Coverage);
        assert_eq!(v[0].severity, Severity::Warning);
    }

    #[test]
    fn file_below_gate_not_in_diff_is_skipped() {
        let json = r#"{
            "data": [{"files": [{
                "filename": "src/other.rs",
                "summary": {"lines": {"percent": 50.0}}
            }]}]
        }"#;
        let v = parse_coverage_json(json, &["src/main.rs".to_string()]).unwrap();
        assert!(v.is_empty());
    }
}
