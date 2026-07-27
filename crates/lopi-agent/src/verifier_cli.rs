//! CLI backend for the Konjo Verifier (Sprint F1 Phase 1) — drives `claude -p`
//! on subscription auth so the checker runs with no API key. Selected
//! automatically by [`crate::verifier::VerifierAgent::new_cli`] whenever no
//! direct-API client is configured, which — before this sprint — was every
//! production code path (`with_api` was never called outside a test).
//!
//! Kill-test findings this design encodes (`.konjo/killtests/F1/`):
//! - **KT-1.1** — `--json-schema` returned schema-conforming
//!   `structured_output` 30/30 times against a real subscription. Parsed
//!   first; [`crate::verifier::parse_verdict`]'s fence-strip parser is the
//!   fallback (0/30 malformed in that role in the same run).
//! - **KT-1.2** — with `Write,Edit,MultiEdit,NotebookEdit,Bash` denied (plus
//!   `Task`/`TodoWrite`/`ExitPlanMode`/`SlashCommand` for cost hygiene — see
//!   the kill-test writeup), a session explicitly instructed to modify a
//!   file refused and left the worktree byte-identical, twice.
//! - **KT-1.3** — `--bare` failed authentication 6/6 times in the sandboxed
//!   session this sprint was built in ("skip ... keychain reads" per
//!   `claude --help`), a harder failure than the brief anticipated ("needs
//!   project context"). The checker does **not** pass `--bare`. This finding
//!   needs re-verification on a real target machine — see
//!   `NEXT_SESSION_PROMPT.md`.
//! - **KT-1.4** — unaffected by this module; `resolve_verifier` (in
//!   `verifier.rs`) already guarantees the checker model differs from the
//!   worker's.
//!
//! `--system-prompt` (full override, not `--append-system-prompt`) carries
//! the checker's grading persona, so the checker session never inherits
//! Claude Code's own coding-agent framing — a cleaner isolation than
//! Phase 6's worker-side `--append-system-prompt` question, which is a
//! different, measurement-gated decision (see `LEDGER.md`).

use crate::claude_model::parse_claude_output;
use crate::claude_support::{
    apply_cli_caps, apply_env_allowlist, build_cli_error, scrub_inherited_anthropic_env,
    SessionMode,
};
use crate::verifier::parse_verdict;
use anyhow::{Context, Result};
use lopi_core::{PermissionMode, VerifierVerdict};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

/// Tools denied to every checker session. The first five are the brief's own
/// minimum (KT-1.2); the rest stop the costly sub-agent-delegation detour
/// KT-1.2 observed when a denied session still tries to route around the
/// deny list via `Task`/plan-mode tooling instead of just refusing.
const CHECKER_DISALLOWED_TOOLS: &[&str] = &[
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "Bash",
    "Task",
    "TodoWrite",
    "ExitPlanMode",
    "SlashCommand",
];

/// `--json-schema` value for a [`VerifierVerdict`] (KT-1.1).
const VERDICT_JSON_SCHEMA: &str = r#"{"type":"object","properties":{"passed":{"type":"boolean"},"gaps":{"type":"array","items":{"type":"string"}},"fix_hints":{"type":"array","items":{"type":"string"}},"confidence":{"type":"number"}},"required":["passed","gaps","fix_hints","confidence"],"additionalProperties":false}"#;

/// A single grading turn should conclude quickly; this bounds a wedged CLI
/// process rather than tuning typical latency (observed 6-25s per call).
const CHECKER_TIMEOUT: Duration = Duration::from_secs(180);
/// The checker never needs more than a couple of turns to read the supplied
/// diff/context and answer — bounding this caps a runaway exploration
/// attempt like the one KT-1.2 observed before `Task` was added to the deny
/// list.
const CHECKER_MAX_TURNS: u32 = 5;
/// Observed cost per call in this session was $0.14-$0.55 (KT-1.1/KT-1.2);
/// this is a hard backstop against a pathological session, not a typical-case
/// tuning.
const CHECKER_MAX_BUDGET_USD: f64 = 1.0;

/// Grade `user_prompt` (the verifier's assembled goal/plan/diff/rubric
/// prompt) against `system_prompt` (the verifier's grading persona) by
/// spawning `claude -p` with cwd `repo_path` — the same worktree
/// `get_repo_diff` read from, so the checker's working directory matches the
/// diff it's grading.
///
/// Fresh session, never resumed: no `--resume` flag is ever passed, which is
/// what makes this a checker rather than a continuation of the maker's own
/// context (Phase 1 design constraint — do not let session-continuity work
/// reach this path). Sprint F4 Phase 3 made this structural, not just
/// convention: `apply_cli_caps` always receives `SessionMode::None` here, and
/// `grade_via_cli_argv_never_includes_bare_or_resume` (below) asserts it.
///
/// # Errors
/// Returns `Err` on a CLI spawn failure, a non-zero exit, a timeout, or a
/// response that is neither valid `structured_output` nor fence-strip
/// parseable — all of which route the caller ([`crate::runner::verifier_runner`])
/// to the existing fail-closed error path.
pub(crate) async fn grade_via_cli(
    repo_path: &Path,
    system_prompt: &str,
    user_prompt: &str,
    model: &str,
    effort: Option<&str>,
) -> Result<VerifierVerdict> {
    let denied: Vec<String> = CHECKER_DISALLOWED_TOOLS
        .iter()
        .map(ToString::to_string)
        .collect();

    let mut cmd = Command::new("claude");
    apply_env_allowlist(&mut cmd);
    cmd.arg("-p")
        .arg(user_prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--json-schema")
        .arg(VERDICT_JSON_SCHEMA)
        .arg("--system-prompt")
        .arg(system_prompt);
    apply_cli_caps(
        &mut cmd,
        Some(model),
        effort,
        Some(PermissionMode::DontAsk.as_str()),
        Some(CHECKER_MAX_TURNS),
        Some(CHECKER_MAX_BUDGET_USD),
        &[],
        &denied,
        // KT-1.3 — `--bare` fails authentication in the sandboxed session
        // this was verified in; see this module's doc comment.
        false,
        // Sprint F4 Phase 3 — the checker is never resumed; see this
        // function's doc comment.
        SessionMode::None,
    );
    cmd.current_dir(repo_path);
    scrub_inherited_anthropic_env(&mut cmd);

    let raw = tokio::time::timeout(CHECKER_TIMEOUT, cmd.output())
        .await
        .context("checker cli timed out")?
        .context("spawning checker cli")?;

    if !raw.status.success() {
        let stderr = String::from_utf8_lossy(&raw.stderr);
        let stdout = String::from_utf8_lossy(&raw.stdout);
        return Err(build_cli_error(
            &stdout,
            &stderr,
            raw.status,
            repo_path,
            user_prompt.len(),
        ));
    }

    let stdout = String::from_utf8_lossy(&raw.stdout).into_owned();
    parse_cli_verdict(stdout)
}

/// Parse a checker CLI response: `structured_output` first (KT-1.1 measured
/// 30/30), the existing fence-strip parser against `result` as fallback.
fn parse_cli_verdict(stdout: String) -> Result<VerifierVerdict> {
    let out = parse_claude_output(stdout, true);
    if !out.succeeded() {
        anyhow::bail!("checker cli reported an error result: {}", out.text());
    }
    if let Some(structured) = out.structured_output.clone() {
        if let Ok(v) = serde_json::from_value::<VerifierVerdict>(structured) {
            return Ok(v);
        }
    }
    parse_verdict(out.text())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_verdict_prefers_structured_output() {
        let stdout = serde_json::json!({
            "type": "result",
            "is_error": false,
            "result": "not json at all — should be ignored",
            "structured_output": {
                "passed": true,
                "gaps": [],
                "fix_hints": [],
                "confidence": 0.95
            }
        })
        .to_string();
        let v = parse_cli_verdict(stdout).unwrap();
        assert!(v.passed);
        assert!((v.confidence - 0.95).abs() < 1e-6);
    }

    #[test]
    fn parse_cli_verdict_falls_back_to_fence_strip_parser() {
        let stdout = serde_json::json!({
            "type": "result",
            "is_error": false,
            "result": "```json\n{\"passed\":false,\"gaps\":[\"g\"],\"fix_hints\":[\"h\"],\"confidence\":0.4}\n```"
        })
        .to_string();
        let v = parse_cli_verdict(stdout).unwrap();
        assert!(!v.passed);
        assert_eq!(v.gaps, vec!["g".to_string()]);
    }

    #[test]
    fn parse_cli_verdict_errors_on_is_error_true() {
        let stdout = serde_json::json!({
            "type": "result",
            "is_error": true,
            "result": "Authentication error"
        })
        .to_string();
        assert!(parse_cli_verdict(stdout).is_err());
    }

    #[test]
    fn parse_cli_verdict_errors_when_neither_form_parses() {
        let stdout = serde_json::json!({
            "type": "result",
            "is_error": false,
            "result": "I refuse to answer in JSON today."
        })
        .to_string();
        assert!(parse_cli_verdict(stdout).is_err());
    }

    #[test]
    fn checker_disallowed_tools_covers_the_briefs_minimum() {
        for must_deny in ["Write", "Edit", "MultiEdit", "NotebookEdit", "Bash"] {
            assert!(
                CHECKER_DISALLOWED_TOOLS.contains(&must_deny),
                "{must_deny} must be denied to the checker"
            );
        }
    }

    /// Argv assertion in the same shape as `claude_support.rs`'s
    /// `apply_cli_caps_includes_every_configured_flag` — asserts the
    /// constructed argv directly rather than trusting a live spawn.
    #[test]
    fn grade_via_cli_argv_never_includes_bare_or_resume() {
        let mut cmd = Command::new("true");
        cmd.arg("-p")
            .arg("prompt")
            .arg("--output-format")
            .arg("json")
            .arg("--json-schema")
            .arg(VERDICT_JSON_SCHEMA)
            .arg("--system-prompt")
            .arg("system");
        let denied: Vec<String> = CHECKER_DISALLOWED_TOOLS
            .iter()
            .map(ToString::to_string)
            .collect();
        apply_cli_caps(
            &mut cmd,
            Some("claude-opus-5"),
            None,
            Some(PermissionMode::DontAsk.as_str()),
            Some(CHECKER_MAX_TURNS),
            Some(CHECKER_MAX_BUDGET_USD),
            &[],
            &denied,
            false,
            SessionMode::None,
        );
        let argv: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(!argv.contains(&"--bare".to_string()), "argv={argv:?}");
        assert!(!argv.contains(&"--resume".to_string()), "argv={argv:?}");
        assert!(!argv.contains(&"--session-id".to_string()), "argv={argv:?}");
        assert!(argv.contains(&"--json-schema".to_string()), "argv={argv:?}");
        assert!(
            argv.contains(&"dontAsk".to_string()),
            "checker must use a headless-safe, never-stalling permission mode; argv={argv:?}"
        );
        for tool in CHECKER_DISALLOWED_TOOLS {
            assert!(
                argv.iter().any(|a| a == tool),
                "{tool} missing from argv={argv:?}"
            );
        }
    }
}
