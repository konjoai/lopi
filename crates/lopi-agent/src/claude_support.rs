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

/// Canonicalize a reasoning-effort string to one of the levels
/// `claude --effort` accepts (`low`/`medium`/`high`/`xhigh`/`max`),
/// lowercasing and trimming first. Returns `None` for anything the CLI
/// would reject, so a malformed `Task.effort` is dropped rather than
/// spawned as an invalid flag.
pub(crate) fn normalize_effort(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        "xhigh" => Some("xhigh"),
        "max" => Some("max"),
        _ => None,
    }
}

/// Session-continuity mode for a `claude -p` spawn (Sprint F4). An enum
/// rather than two `Option<&str>` params because the three states are
/// mutually exclusive by construction — a spawn either starts a fresh,
/// unlabeled session (`None`), starts a fresh session under an explicit id
/// (`New`, `--session-id`), or continues an existing one (`Resume`,
/// `--resume`) — a type that cannot represent "both at once" needs no
/// runtime check to keep that invariant.
///
/// `New(id)`: KT-4.2 (`.konjo/killtests/F4/KT-4.2.md`) confirmed live that
/// the CLI accepts an arbitrary UUID (not just one it generated itself) and
/// round-trips it unchanged into the `Init`/`Result` events
/// `StreamEvent::session_id()` already parses. `AgentRunner` uses this to
/// mint a fresh per-*attempt* UUID (never a reused `TaskId` — a retried
/// attempt would collide with its predecessor's still-live session id) so
/// the id is known before the first spawn even happens, for Phase 4
/// correlation.
///
/// `Resume(id)`: KT-4.1 confirmed a resumed session — spawned in the same
/// worktree cwd, `--permission-mode` set, no `--bare`, subscription auth —
/// retains prior context and does not re-read files the first session
/// already read (verified against the tool-call stream, not by asking the
/// model). Keyed on the id a prior `New` (or the CLI's own generated id) set.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SessionMode<'a> {
    /// No session-continuity flag — every spawn site's behavior before this
    /// sprint, and still the only mode the verifier/post-mortem checker
    /// paths ever use (Phase 3 — a fresh, unlabeled session per call is what
    /// makes them checkers rather than continuations of the maker's own
    /// context).
    None,
    /// `--session-id <id>` — start a fresh session under a caller-chosen id.
    New(&'a str),
    /// `--resume <id>` — continue an existing session.
    Resume(&'a str),
}

impl SessionMode<'_> {
    fn apply(self, cmd: &mut Command) {
        match self {
            SessionMode::None => {}
            SessionMode::New(id) => {
                cmd.arg("--session-id").arg(id);
            }
            SessionMode::Resume(id) => {
                cmd.arg("--resume").arg(id);
            }
        }
    }
}

/// Whether a `claude -p` result envelope looks like the *session itself*
/// failed to establish, rather than a genuine mid-session failure (a real
/// implementation/test bug, a tool denial, a timeout after real work).
/// Sprint F4's resume fallback (Phase 2) only wants to retry cold when a
/// resumed session never got off the ground — confirmed live
/// (`.konjo/killtests/F4/KT-4.1.md`): an unresumable `--resume <id>` exits
/// non-zero with `is_error: true` and `num_turns: 0`, before a single turn
/// runs. Retrying cold on *any* failure a resumed call happens to hit would
/// silently double-spend on a genuine bug that has nothing to do with the
/// session, which is not what "fall back on resume failure" is asking for.
pub(crate) fn looks_like_session_establishment_failure(is_error: bool, num_turns: u32) -> bool {
    is_error && num_turns == 0
}

/// Apply the caps shared by all `claude -p` spawn sites — `--model`,
/// `--permission-mode`, `--max-turns`, `--max-budget-usd`, `--allowedTools`,
/// `--disallowedTools`, and (Sprint F4) session continuity — to `cmd`. Each
/// site still adds its own `-p <prompt>` (their positions/doc comments differ
/// enough not to share), but the optional-cap block was identical copy-paste
/// across `ClaudeCode::run`, `ClaudeCode::run_streamed`, and
/// `claude_stream::plan_streaming` — a fourth spawn site could easily drop
/// one by hand-copying the block again.
///
/// `--permission-mode` folded in here (Permission-Modes-1), reversing this
/// function's own prior doc comment that kept `--dangerously-skip-permissions`
/// per-site: unlike the caps above (each genuinely optional, `None`/empty
/// meaning "add nothing"), permission mode is *never* absent from the spawned
/// argv — every site must emit some value — which makes it a true shared
/// cap, not a per-site concern. `permission_mode: None` falls back to
/// [`lopi_core::PermissionMode::default()`] (`bypassPermissions`), so an
/// unconfigured task reproduces the old unconditional
/// `--dangerously-skip-permissions` behavior exactly.
///
/// `bare` is **never optional either, in the opposite sense from
/// `permission_mode`** — Sprint F2 Phase 6. `--bare` skips hook/LSP/plugin
/// sync/CLAUDE.md auto-discovery; every one of lopi's three current spawn
/// sites is a *worker* session (plan/implement/fix), which must load the
/// target repo's own `CLAUDE.md`/skills to behave like a normal Claude Code
/// session on that repo — so all three call this with `bare: false`
/// explicitly, not by leaving a flag unset and relying on today's default.
/// Anthropic's own CLI help documents `--bare` as recommended for scripted
/// calls and **slated to become the default** — the day it flips, every
/// spawn site that never made a call either way would silently stop loading
/// repo context with no error and no code change. Pinning `false` here now
/// means that flip is a no-op for lopi. Checker/post-mortem sessions (F1)
/// should construct a fourth spawn site — or extend this signature — with
/// `bare: true`; see `LEDGER.md` for the full one-way-door writeup.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_cli_caps(
    cmd: &mut Command,
    model: Option<&str>,
    effort: Option<&str>,
    permission_mode: Option<&str>,
    max_turns: Option<u32>,
    max_budget_usd: Option<f64>,
    allowed_tools: &[String],
    disallowed_tools: &[String],
    bare: bool,
    session: SessionMode<'_>,
) {
    if bare {
        cmd.arg("--bare");
    }
    let mode = permission_mode.unwrap_or(lopi_core::PermissionMode::default().as_str());
    cmd.arg("--permission-mode").arg(mode);
    session.apply(cmd);
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
    // The card's `Effort` knob (`Task.effort`) was previously stored but
    // never reached the worker — only the verifier's grading pass honored
    // it — so "Low" had zero effect on a run's reasoning depth or cost.
    // `--effort` is a CLI-path flag independent of the direct-API path's
    // cached-system-prompt constraint that kept this unwired. Callers pass
    // an already-validated level (see `normalize_effort`).
    if let Some(e) = effort {
        cmd.arg("--effort").arg(e);
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

/// Names of environment variables that carry a *parent* Claude Code
/// session's own identity. Sprint F4's session-continuity spawns now pass
/// `--session-id`/`--resume` explicitly, which should dominate any inherited
/// env — but scrubbing this defensively closes a real, live-reproduced gap
/// (`.konjo/killtests/F4/KT-4.1.md`): when lopi itself runs nested inside
/// another Claude Code session (or a CI runner that sets these), an
/// unscrubbed child `claude -p` process silently adopted the *parent's*
/// session id instead of getting a fresh one — confirmed live, the child's
/// `Init` event reported the exact same UUID as the outer session.
const INHERITED_SESSION_ENV: &[&str] = &["CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION"];

/// Remove inherited Anthropic routing/auth env vars, and (Sprint F4) a
/// parent Claude Code session's own identity, from a spawned-process
/// command. Used for both the one-shot `run()` path and the streaming plan
/// path so neither accidentally bills against a user's API credits or
/// silently inherits a session id lopi never chose.
pub(crate) fn scrub_inherited_anthropic_env(cmd: &mut Command) {
    for var in ANTHROPIC_ROUTING_ENV {
        cmd.env_remove(var);
    }
    for var in INHERITED_SESSION_ENV {
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
#[path = "claude_support_tests.rs"]
mod tests;
