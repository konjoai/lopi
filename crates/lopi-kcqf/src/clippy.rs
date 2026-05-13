//! Clippy JSON output parser for quality violation extraction.
use crate::{QualityViolation, Severity, ViolationKind};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use tokio::process::Command;

/// Run `cargo clippy --message-format=json` in `repo_path` and return violations.
///
/// Filters to lints with `level = "error"` or `level = "warning"` that are
/// not from proc-macro expansions. Cognitive-complexity violations are mapped
/// to `ViolationKind::Complexity`; all others to `ViolationKind::Standards`.
///
/// # Errors
/// Returns an error if the subprocess cannot be spawned.
pub async fn scan_clippy(repo_path: &Path) -> Result<Vec<QualityViolation>> {
    let output = Command::new("cargo")
        .args([
            "clippy",
            "--workspace",
            "--message-format=json",
            "--",
            "-W",
            "clippy::cognitive_complexity",
        ])
        .current_dir(repo_path)
        .output()
        .await
        .context("spawning cargo clippy")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_clippy_json(&stdout)
}

/// Parse the line-delimited JSON stream produced by `cargo clippy --message-format=json`.
pub(crate) fn parse_clippy_json(json_stream: &str) -> Result<Vec<QualityViolation>> {
    let mut violations = Vec::new();

    for line in json_stream.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: ClippyMessage = match serde_json::from_str(line) {
            Ok(m) => m,
            Err(_) => continue, // non-JSON lines (e.g. "Compiling …") are silently skipped
        };

        if msg.reason != "compiler-message" {
            continue;
        }

        let diag = match msg.message {
            Some(d) => d,
            None => continue,
        };

        let level = diag.level.as_deref().unwrap_or("");
        if level != "error" && level != "warning" {
            continue;
        }

        // Skip proc-macro / compiler-internal spans.
        let span = match diag.spans.into_iter().find(|s| s.is_primary) {
            Some(s) => s,
            None => continue,
        };

        let file = span.file_name.clone();
        // Skip generated files and dependencies.
        if file.contains("/.cargo/") || file.starts_with("macro expansion") {
            continue;
        }

        let raw_message = diag.message.unwrap_or_default();
        let (kind, confidence) = classify_message(&raw_message);
        let severity = if level == "error" {
            Severity::Error
        } else {
            Severity::Warning
        };

        violations.push(QualityViolation {
            file,
            line: Some(span.line_start),
            kind,
            severity,
            message: raw_message.clone(),
            fix_hint: make_fix_hint(&raw_message),
            confidence,
        });
    }

    Ok(violations)
}

fn classify_message(msg: &str) -> (ViolationKind, f32) {
    if msg.contains("cognitive complexity") {
        (ViolationKind::Complexity, 1.0)
    } else if msg.contains("dead_code") || msg.contains("never used") {
        (ViolationKind::DeadCode, 0.9)
    } else {
        (ViolationKind::Standards, 0.95)
    }
}

fn make_fix_hint(msg: &str) -> String {
    if msg.contains("cognitive complexity") {
        "Reduce cognitive complexity by extracting helper functions".to_string()
    } else if msg.contains("dead_code") || msg.contains("never used") {
        "Remove unused code or add a doc comment if intentionally kept".to_string()
    } else if msg.len() > 120 {
        format!("Fix clippy lint: {}", &msg[..120])
    } else {
        format!("Fix clippy lint: {msg}")
    }
}

// ── Clippy JSON types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ClippyMessage {
    reason: String,
    message: Option<ClippyDiagnostic>,
}

#[derive(Deserialize)]
struct ClippyDiagnostic {
    message: Option<String>,
    level: Option<String>,
    #[serde(default)]
    spans: Vec<ClippySpan>,
}

#[derive(Deserialize)]
struct ClippySpan {
    file_name: String,
    line_start: u32,
    is_primary: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_output_returns_empty() {
        let violations = parse_clippy_json("").unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn parse_non_json_lines_are_skipped() {
        let input = "Compiling lopi-kcqf v0.1.0\nFinished dev\n";
        let violations = parse_clippy_json(input).unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn parse_compiler_message_with_warning() {
        let input = r#"{"reason":"compiler-message","message":{"message":"the message","level":"warning","spans":[{"file_name":"src/main.rs","line_start":42,"is_primary":true}]}}"#;
        let violations = parse_clippy_json(input).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].file, "src/main.rs");
        assert_eq!(violations[0].line, Some(42));
        assert_eq!(violations[0].severity, Severity::Warning);
    }

    #[test]
    fn parse_compiler_artifact_is_skipped() {
        let input = r#"{"reason":"compiler-artifact","package_id":"foo"}"#;
        let violations = parse_clippy_json(input).unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn cognitive_complexity_maps_to_complexity_kind() {
        let input = r#"{"reason":"compiler-message","message":{"message":"the function has a cognitive complexity of 20","level":"warning","spans":[{"file_name":"src/lib.rs","line_start":10,"is_primary":true}]}}"#;
        let violations = parse_clippy_json(input).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, ViolationKind::Complexity);
    }

    #[test]
    fn cargo_path_spans_are_skipped() {
        let input = r#"{"reason":"compiler-message","message":{"message":"unused","level":"warning","spans":[{"file_name":"/home/user/.cargo/registry/src/foo.rs","line_start":1,"is_primary":true}]}}"#;
        let violations = parse_clippy_json(input).unwrap();
        assert!(violations.is_empty());
    }
}
