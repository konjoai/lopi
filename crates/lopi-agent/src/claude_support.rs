//! Subprocess-env scrubbing and fix-prompt error compression — split out of
//! `claude.rs` purely to keep that file under the 500-line CI file-size
//! gate; `scrub_inherited_anthropic_env` is re-exported from `claude`
//! unchanged, so `crate::claude::scrub_inherited_anthropic_env` stays valid
//! for `claude_stream.rs`'s call site.

use lopi_core::Task;
use lopi_toon::encode_task_context;
use std::path::Path;
use std::process::ExitStatus;
use tokio::process::Command;

/// Build the planning prompt: a TOON-encoded task context (goal, dirs,
/// constraints, pattern memory, lessons) plus the optional previous-failure
/// addendum. Shared by `ClaudeCode`'s one-shot `plan` and streaming
/// `plan_streamed` paths so the prompt stays identical. Takes
/// `ClaudeCode`'s pattern-memory fields explicitly (rather than `&self`) so
/// it can live outside the `claude` module.
pub(crate) fn build_plan_prompt(
    task: &Task,
    last_error: Option<&str>,
    extra_constraints: &[String],
    patterns: &[(String, String)],
    lessons: &[(String, String)],
) -> String {
    let all_constraints: Vec<&str> = task
        .constraints
        .iter()
        .chain(extra_constraints.iter())
        .map(String::as_str)
        .collect();
    let allowed: Vec<&str> = task.allowed_dirs.iter().map(String::as_str).collect();
    let forbidden: Vec<&str> = task.forbidden_dirs.iter().map(String::as_str).collect();
    // Convert lessons from Vec<(String, String)> to Vec<(&str, &str)> for TOON.
    let lesson_refs: Vec<(&str, &str)> = lessons
        .iter()
        .map(|(cat, content)| (cat.as_str(), content.as_str()))
        .collect();
    let ctx = encode_task_context(
        &task.goal,
        &allowed,
        &forbidden,
        &all_constraints,
        patterns,
        &lesson_refs,
    );
    let mut prompt = format!(
        "You are running inside lopi. \
         Produce a concise implementation plan. \
         Output a numbered list of steps only.\n\n\
         ## Task context (TOON)\n\
         {ctx}"
    );
    if let Some(err) = last_error {
        prompt.push_str(&format!(
            "\n\n## Previous attempt failed\nAnalyze this error and adjust your approach:\n{err}"
        ));
    }
    prompt
}

/// Build the implementation prompt: a TOON-encoded scope plus the plan.
/// Shared by `ClaudeCode`'s `implement` and `implement_streamed` paths.
pub(crate) fn build_implement_prompt(task: &Task, plan: &str) -> String {
    let allowed: Vec<&str> = task.allowed_dirs.iter().map(String::as_str).collect();
    let forbidden: Vec<&str> = task.forbidden_dirs.iter().map(String::as_str).collect();
    let scope = encode_task_context(&task.goal, &allowed, &forbidden, &[], &[], &[]);
    format!(
        "Implement the plan below in the current repository.\n\n\
         ## Scope (TOON)\n\
         {scope}\n\
         ## Plan\n\
         {plan}"
    )
}

/// Apply the caps shared by all three `claude -p` spawn sites — `--model`,
/// `--max-turns`, `--max-budget-usd`, `--allowedTools`, `--disallowedTools`
/// — to `cmd`. Each site still adds its own `-p <prompt>` and
/// `--dangerously-skip-permissions` (their positions/doc comments differ
/// enough not to share), but the optional-cap block was identical
/// copy-paste across `ClaudeCode::run`, `ClaudeCode::run_streamed`, and
/// `claude_stream::plan_streaming` — a fourth spawn site could easily drop
/// one by hand-copying the block again.
pub(crate) fn apply_cli_caps(
    cmd: &mut Command,
    model: Option<&str>,
    max_turns: Option<u32>,
    max_budget_usd: Option<f64>,
    allowed_tools: &[String],
    disallowed_tools: &[String],
) {
    if let Some(m) = model {
        cmd.arg("--model").arg(m);
        // Pin Task-tool sub-agents to the card's model too. `--model`
        // governs only the top-level `claude -p` process; a sub-agent whose
        // `.claude/agents/*.md` frontmatter pins `model:` (e.g. a research
        // agent set to `sonnet`) ignores `--model` and runs on that pricier
        // model — so a "Haiku" card silently fans out Sonnet-billed
        // sub-agents, the confirmed cause of a Haiku run costing several
        // dollars. `CLAUDE_CODE_SUBAGENT_MODEL` is the only lever that
        // overrides an agent's frontmatter (and the Task tool's
        // per-invocation model), forcing every sub-agent onto the card's
        // chosen model. Set explicitly so an inherited value from lopi's own
        // env can't leak in. See code.claude.com/docs/en/model-config.
        cmd.env("CLAUDE_CODE_SUBAGENT_MODEL", m);
    }
    if let Some(turns) = max_turns {
        cmd.arg("--max-turns").arg(turns.to_string());
    }
    if let Some(usd) = max_budget_usd {
        cmd.arg("--max-budget-usd").arg(format!("{usd}"));
    }
    if !allowed_tools.is_empty() {
        cmd.arg("--allowedTools").args(allowed_tools);
    }
    if !disallowed_tools.is_empty() {
        cmd.arg("--disallowedTools").args(disallowed_tools);
    }
}

/// Build the error `ClaudeCode::run` bails with on a non-zero CLI exit.
/// Parses the JSON failure envelope the CLI writes to stdout on rate-limit/
/// auth/billing errors when present (surfacing the human-readable `result`
/// field and API status code instead of raw JSON noise), hard-stops with
/// [`ERR_CREDIT_EXHAUSTED`](crate::claude::ERR_CREDIT_EXHAUSTED) on a
/// credit-exhausted account, and falls back to raw stderr/stdout otherwise.
pub(crate) fn build_cli_error(
    stdout: &str,
    stderr: &str,
    status: ExitStatus,
    cwd: &Path,
    prompt_len: usize,
) -> anyhow::Error {
    let parsed_msg: Option<(String, Option<u16>)> =
        serde_json::from_str::<serde_json::Value>(stdout)
            .ok()
            .and_then(|v| {
                let result = v.get("result")?.as_str()?.to_string();
                let api_status = v
                    .get("api_error_status")
                    .and_then(serde_json::Value::as_u64)
                    .map(|s| s as u16);
                Some((result, api_status))
            });

    if let Some((msg, api_status)) = parsed_msg {
        // Hard stop for billing failure — retrying just stalls the agent.
        // The run loop matches on ERR_CREDIT_EXHAUSTED to short-circuit
        // instead of burning the retry budget.
        if msg.to_lowercase().contains("credit balance") || api_status == Some(402) {
            return anyhow::anyhow!(
                "{}: {msg}. Add credits at https://console.anthropic.com/settings/billing",
                crate::claude::ERR_CREDIT_EXHAUSTED
            );
        }
        let api = api_status
            .map(|s| format!(" (api_error_status={s})"))
            .unwrap_or_default();
        return anyhow::anyhow!("claude api error{api}: {msg}");
    }

    let detail = match (stderr.trim().is_empty(), stdout.trim().is_empty()) {
        (false, false) => format!("stderr={stderr}; stdout={stdout}"),
        (false, true) => format!("stderr={stderr}"),
        (true, false) => format!("stdout={stdout}"),
        (true, true) => "no output on stderr or stdout".to_string(),
    };
    anyhow::anyhow!(
        "claude cli exited {status} (cwd={}, prompt={prompt_len}B): {detail}",
        cwd.display(),
    )
}

/// Names of environment variables that, when inherited from the parent
/// process, cause the spawned `claude` CLI to bypass the user's interactive
/// subscription auth and route through the per-token billed API (or a custom
/// gateway). lopi must NOT silently bill against the user's API balance —
/// the design intent is to drive their Claude Code subscription. We strip
/// these from the child process env so the CLI falls back to its on-disk
/// credentials at `~/.claude/`.
const ANTHROPIC_ROUTING_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_BEDROCK_BASE_URL",
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
];

/// Remove inherited Anthropic routing/auth env vars from a spawned-process
/// command. Used for both the one-shot `run()` path and the streaming plan
/// path so neither accidentally bills against a user's API credits.
pub(crate) fn scrub_inherited_anthropic_env(cmd: &mut Command) {
    for var in ANTHROPIC_ROUTING_ENV {
        cmd.env_remove(var);
    }
}

/// Strip Rust backtrace noise and deduplicate repeated error blocks to reduce fix-prompt token count.
/// Removes lines matching `at src/`, `note: run with RUST_BACKTRACE`, and limits each error to
/// 30 lines. Identical adjacent blocks are collapsed to one copy.
pub(crate) fn compress_errors(errors: &[String]) -> String {
    let mut seen: Vec<String> = Vec::with_capacity(errors.len());
    for err in errors {
        let compressed: String = err
            .lines()
            .filter(|line| {
                let t = line.trim();
                !t.starts_with("note: run with RUST_BACKTRACE")
                    && !t.starts_with("stack backtrace:")
                    && !(t.starts_with("at ") && (t.contains("src/") || t.contains(".rs:")))
            })
            .take(30)
            .collect::<Vec<_>>()
            .join("\n");
        if !seen.contains(&compressed) {
            seen.push(compressed);
        }
    }
    seen.join("\n---\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn status(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    /// Collect the `(key, value)` env overrides set on a `Command`.
    fn env_overrides(cmd: &Command) -> Vec<(String, String)> {
        cmd.as_std()
            .get_envs()
            .filter_map(|(k, v)| {
                v.map(|v| {
                    (
                        k.to_string_lossy().into_owned(),
                        v.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect()
    }

    #[test]
    fn apply_cli_caps_omits_flags_for_none_and_empty() {
        let mut cmd = Command::new("true");
        apply_cli_caps(&mut cmd, None, None, None, &[], &[]);
        let argv: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(argv.is_empty(), "argv={argv:?}");
        // No model ⇒ no sub-agent pin: sub-agents inherit the CLI default.
        assert!(
            !env_overrides(&cmd)
                .iter()
                .any(|(k, _)| k == "CLAUDE_CODE_SUBAGENT_MODEL"),
            "sub-agent model must not be pinned when no --model is set"
        );
    }

    #[test]
    fn apply_cli_caps_pins_subagent_model_to_the_session_model() {
        let mut cmd = Command::new("true");
        apply_cli_caps(&mut cmd, Some("haiku"), None, None, &[], &[]);
        assert!(
            env_overrides(&cmd)
                .iter()
                .any(|(k, v)| k == "CLAUDE_CODE_SUBAGENT_MODEL" && v == "haiku"),
            "sub-agents must be pinned to the card's model so a Haiku card \
             can't fan out pricier sub-agents"
        );
    }

    #[test]
    fn apply_cli_caps_includes_every_configured_flag() {
        let mut cmd = Command::new("true");
        apply_cli_caps(
            &mut cmd,
            Some("claude-opus-4-7"),
            Some(5),
            Some(2.5),
            &["Bash".to_string()],
            &["Workflow".to_string()],
        );
        let argv: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            argv,
            vec![
                "--model",
                "claude-opus-4-7",
                "--max-turns",
                "5",
                "--max-budget-usd",
                "2.5",
                "--allowedTools",
                "Bash",
                "--disallowedTools",
                "Workflow",
            ]
        );
    }

    #[test]
    fn build_cli_error_hard_stops_on_credit_exhaustion() {
        let stdout = r#"{"result":"Your credit balance is too low","api_error_status":402}"#;
        let err = build_cli_error(stdout, "", status(1), Path::new("."), 10);
        assert!(err
            .to_string()
            .contains(crate::claude::ERR_CREDIT_EXHAUSTED));
    }

    #[test]
    fn build_cli_error_surfaces_the_parsed_api_message() {
        let stdout = r#"{"result":"rate limited","api_error_status":429}"#;
        let err = build_cli_error(stdout, "", status(1), Path::new("."), 10);
        let msg = err.to_string();
        assert!(msg.contains("rate limited"));
        assert!(msg.contains("429"));
    }

    #[test]
    fn build_cli_error_falls_back_to_raw_streams_when_unparseable() {
        let err = build_cli_error("not json", "boom", status(1), Path::new("."), 10);
        let msg = err.to_string();
        assert!(msg.contains("boom"));
        assert!(msg.contains("not json"));
    }

    #[test]
    fn compress_errors_removes_backtrace_noise() {
        let errors = vec![
            "error[E0308]: mismatched types\n  at src/main.rs:10\nnote: run with RUST_BACKTRACE=1\nstack backtrace:\n  at src/foo.rs:5".to_string(),
        ];
        let out = compress_errors(&errors);
        assert!(!out.contains("RUST_BACKTRACE"));
        assert!(!out.contains("stack backtrace:"));
        assert!(!out.contains("at src/"));
        assert!(out.contains("mismatched types"));
    }

    #[test]
    fn compress_errors_deduplicates_identical_blocks() {
        let block = "error: cannot borrow as mutable".to_string();
        let errors = vec![block.clone(), block.clone(), block.clone()];
        let out = compress_errors(&errors);
        // Only one copy should survive deduplication
        assert_eq!(out.matches("cannot borrow").count(), 1);
    }
}
