//! CLI backend for the failure post-mortem (Sprint F1 Phase 3) — same
//! transport shift as `verifier_cli.rs`: drives `claude -p` on subscription
//! auth instead of requiring a direct-API client, so post-mortem reflection
//! (`postmortem.rs::run_postmortem`) can actually run in every production
//! deployment (`with_api` was never wired outside a test — see `LEDGER.md`).
//!
//! Reuses the same read-only deny list and no-`--bare` decision as the
//! verifier CLI backend (`.konjo/killtests/F1/KT-1.2.md`, `KT-1.3.md`) —
//! the post-mortem is a reflection over a failure log, not a task that
//! needs to touch the worktree.

use super::postmortem::{
    build_postmortem_prompt, extract_constraint, PostmortemOutcome, POSTMORTEM_SYSTEM_PROMPT,
};
use crate::claude_model::parse_claude_output;
use crate::claude_support::{apply_cli_caps, build_cli_error, scrub_inherited_anthropic_env};
use anyhow::{Context, Result};
use lopi_core::PermissionMode;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

/// Deny list — identical rationale to `verifier_cli::CHECKER_DISALLOWED_TOOLS`.
const POSTMORTEM_DISALLOWED_TOOLS: &[&str] = &[
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

const POSTMORTEM_TIMEOUT: Duration = Duration::from_secs(120);
const POSTMORTEM_MAX_TURNS: u32 = 3;
const POSTMORTEM_MAX_BUDGET_USD: f64 = 0.5;

/// Run a post-mortem via the `claude` CLI. Mirrors
/// `postmortem::run_postmortem`'s contract exactly (same prompt builder,
/// same `extract_constraint` validation) — only the transport differs.
///
/// # Errors
/// Returns `Err` on a CLI spawn failure, non-zero exit, timeout, or a
/// response that fails [`extract_constraint`]'s validation. Callers should
/// log and continue — post-mortem failure must never block task
/// termination (see `postmortem.rs`'s module doc).
pub(crate) async fn run_postmortem_cli(
    repo_path: &Path,
    model: &str,
    goal: &str,
    error_log: &str,
) -> Result<PostmortemOutcome> {
    let prompt = build_postmortem_prompt(goal, error_log);
    let denied: Vec<String> = POSTMORTEM_DISALLOWED_TOOLS
        .iter()
        .map(ToString::to_string)
        .collect();

    let mut cmd = Command::new("claude");
    cmd.arg("-p")
        .arg(&prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--system-prompt")
        .arg(POSTMORTEM_SYSTEM_PROMPT);
    apply_cli_caps(
        &mut cmd,
        Some(model),
        None,
        Some(PermissionMode::DontAsk.as_str()),
        Some(POSTMORTEM_MAX_TURNS),
        Some(POSTMORTEM_MAX_BUDGET_USD),
        &[],
        &denied,
        false, // KT-1.3 — see verifier_cli.rs's module doc.
    );
    cmd.current_dir(repo_path);
    scrub_inherited_anthropic_env(&mut cmd);

    let raw = tokio::time::timeout(POSTMORTEM_TIMEOUT, cmd.output())
        .await
        .context("post-mortem cli timed out")?
        .context("spawning post-mortem cli")?;

    if !raw.status.success() {
        let stderr = String::from_utf8_lossy(&raw.stderr);
        let stdout = String::from_utf8_lossy(&raw.stdout);
        return Err(build_cli_error(
            &stdout,
            &stderr,
            raw.status,
            repo_path,
            prompt.len(),
        ));
    }

    let stdout = String::from_utf8_lossy(&raw.stdout).into_owned();
    let out = parse_claude_output(stdout, true);
    if !out.succeeded() {
        anyhow::bail!("post-mortem cli reported an error result: {}", out.text());
    }
    let constraint = extract_constraint(out.text())
        .context("post-mortem cli returned empty or invalid constraint")?;
    let usage = out.usage.unwrap_or_default();
    Ok(PostmortemOutcome {
        constraint,
        input_tokens: usage.input_tokens.try_into().unwrap_or(u32::MAX),
        output_tokens: usage.output_tokens.try_into().unwrap_or(u32::MAX),
        cache_read_tokens: usage.cache_read_tokens.try_into().unwrap_or(u32::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Argv assertion in the same shape as `verifier_cli.rs`'s own —
    /// no `--bare`, no `--resume`, the deny list present.
    #[test]
    fn postmortem_cli_argv_never_includes_bare_or_resume() {
        let mut cmd = Command::new("true");
        cmd.arg("-p")
            .arg("prompt")
            .arg("--output-format")
            .arg("json")
            .arg("--system-prompt")
            .arg(POSTMORTEM_SYSTEM_PROMPT);
        let denied: Vec<String> = POSTMORTEM_DISALLOWED_TOOLS
            .iter()
            .map(ToString::to_string)
            .collect();
        apply_cli_caps(
            &mut cmd,
            Some("claude-haiku-4-5"),
            None,
            Some(PermissionMode::DontAsk.as_str()),
            Some(POSTMORTEM_MAX_TURNS),
            Some(POSTMORTEM_MAX_BUDGET_USD),
            &[],
            &denied,
            false,
        );
        let argv: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(!argv.contains(&"--bare".to_string()), "argv={argv:?}");
        assert!(!argv.contains(&"--resume".to_string()), "argv={argv:?}");
        for tool in POSTMORTEM_DISALLOWED_TOOLS {
            assert!(
                argv.iter().any(|a| a == tool),
                "{tool} missing from argv={argv:?}"
            );
        }
    }
}
