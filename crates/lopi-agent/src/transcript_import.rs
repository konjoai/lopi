//! Onboarding-Import-1 (Phase 1/4) — defensive decoder for historical
//! Claude Code session transcripts at `~/.claude/projects/**/*.jsonl`.
//!
//! Distinct from [`crate::claude_events`], which decodes
//! `claude -p --output-format stream-json` NDJSON (the live-stream
//! format). This decodes the separate, related schema Claude Code's own
//! session transcripts use — built against a real captured sample (see
//! `LEDGER.md`'s Onboarding-Import-1 entry), not docs. Same discipline as
//! `claude_events.rs`: unrecognized shapes become [`TranscriptEvent::Other`]
//! and this module never panics on malformed input.
//!
//! **KT-A finding (confirmed against a live sample):** a transcript line
//! with `"type": "user"` is *not* always a genuine human turn — it also
//! carries tool-result entries, the historical-transcript equivalent of
//! `claude_events.rs`'s `ToolResult` handling for the live-stream format.
//! The distinguishing signal is `message.content`'s shape, not the
//! top-level `type` field: a plain JSON string means a real human turn; a
//! JSON array containing a `tool_result`-typed block means a tool result
//! wrapped in a `type: "user"` envelope. **Not confirmed:** whether any
//! transcript ever carries a `type: "summary"` entry (the mission brief
//! raised this as a possible richer goal source) — no such entry appeared
//! in the one live sample available to this session (a single in-progress
//! session's own transcript, not a corpus of historical projects; see
//! `NEXT_SESSION_PROMPT.md`), so this module does not special-case it.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

/// Characters kept from an assistant text block before truncation into a
/// `successful_constraints` string.
const CONSTRAINT_CAP: usize = 400;
/// How many of a session's trailing tool results are inspected for the
/// completion heuristic — recent errors are what should block a "success"
/// verdict, not one flaky tool call from earlier in a long session.
const TAIL_WINDOW: usize = 5;

/// One decoded, backfill-relevant item from a single transcript line.
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptEvent {
    /// A genuine human-authored turn.
    HumanTurn {
        /// The turn's text.
        text: String,
        /// The JSONL's own `cwd` at the time of this turn, if present.
        cwd: Option<String>,
        /// The JSONL's own `sessionId`, if present.
        session_id: Option<String>,
    },
    /// An assistant turn's concatenated text blocks (thinking/tool-use
    /// blocks are dropped — only prose is relevant to the completion
    /// heuristic and goal derivation).
    AssistantTurn {
        /// The turn's text.
        text: String,
    },
    /// A `type: "user"` entry whose `message.content` is a list containing
    /// a `tool_result` block — a tool result, not a human turn.
    ToolResult {
        /// Whether the wrapped tool result reported an error.
        is_error: bool,
    },
    /// Any unrecognized or unparseable line — a no-op, never panics.
    Other,
}

/// Parse one NDJSON line into a [`TranscriptEvent`]. Malformed JSON or an
/// unrecognized `type` yields [`TranscriptEvent::Other`], never an error.
#[must_use]
pub fn parse_line(line: &str) -> TranscriptEvent {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return TranscriptEvent::Other;
    }
    let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
        return TranscriptEvent::Other;
    };
    match v.get("type").and_then(Value::as_str) {
        Some("user") => parse_user(&v),
        Some("assistant") => parse_assistant(&v),
        // queue-operation / attachment / last-prompt / anything else this
        // sprint has no use for.
        _ => TranscriptEvent::Other,
    }
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn text_blocks(blocks: &[Value]) -> String {
    blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_user(v: &Value) -> TranscriptEvent {
    match v.pointer("/message/content") {
        Some(Value::String(text)) => TranscriptEvent::HumanTurn {
            text: text.clone(),
            cwd: str_field(v, "cwd"),
            session_id: str_field(v, "sessionId"),
        },
        Some(Value::Array(blocks)) => {
            let tool_result = blocks
                .iter()
                .find(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"));
            if let Some(tr) = tool_result {
                let is_error = tr.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                return TranscriptEvent::ToolResult { is_error };
            }
            // A list `content` that isn't a tool result (e.g. a human turn
            // with an image attachment alongside text) is still a genuine
            // human turn — concatenate its text blocks defensively.
            let text = text_blocks(blocks);
            if text.trim().is_empty() {
                TranscriptEvent::Other
            } else {
                TranscriptEvent::HumanTurn {
                    text,
                    cwd: str_field(v, "cwd"),
                    session_id: str_field(v, "sessionId"),
                }
            }
        }
        _ => TranscriptEvent::Other,
    }
}

fn parse_assistant(v: &Value) -> TranscriptEvent {
    let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) else {
        return TranscriptEvent::Other;
    };
    let text = text_blocks(blocks);
    if text.trim().is_empty() {
        TranscriptEvent::Other
    } else {
        TranscriptEvent::AssistantTurn { text }
    }
}

/// One historical session assembled from a transcript file's lines —
/// the unit [`crate::transcript_import`] hands off to the onboarding
/// backfill (Phase 3).
#[derive(Debug, Clone, Default)]
pub struct HistoricalSession {
    /// The JSONL's own `sessionId` (falls back to the filename stem if no
    /// line carried one).
    pub session_id: String,
    /// The JSONL's own `cwd` — the project directory this session ran in.
    /// `None` if no line in the file carried a `cwd`.
    pub project_dir: Option<String>,
    /// Every genuine human turn's text, in order.
    pub human_turns: Vec<String>,
    /// Every assistant turn's text, in order.
    pub assistant_texts: Vec<String>,
    /// `is_error` flags for every tool result, in order.
    pub tool_result_errors: Vec<bool>,
}

/// Read and parse one transcript file into a [`HistoricalSession`].
/// A malformed line is skipped (via [`TranscriptEvent::Other`]), never
/// aborts the read — one bad line should not lose an entire session's
/// otherwise-good history.
///
/// # Errors
/// Returns `Err` if the file cannot be read at all.
pub fn parse_session_file(path: &Path) -> Result<HistoricalSession> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading transcript {}", path.display()))?;
    let mut session = HistoricalSession {
        session_id: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string(),
        ..Default::default()
    };
    for line in content.lines() {
        match parse_line(line) {
            TranscriptEvent::HumanTurn {
                text,
                cwd,
                session_id,
            } => {
                session.human_turns.push(text);
                if session.project_dir.is_none() {
                    session.project_dir = cwd;
                }
                if let Some(sid) = session_id {
                    session.session_id = sid;
                }
            }
            TranscriptEvent::AssistantTurn { text } => session.assistant_texts.push(text),
            TranscriptEvent::ToolResult { is_error } => {
                session.tool_result_errors.push(is_error);
            }
            TranscriptEvent::Other => {}
        }
    }
    Ok(session)
}

/// Find every `*.jsonl` transcript directly inside `<claude_dir>/projects/*/`
/// (one directory per project, one file per session — the shape confirmed
/// against a real sample; see the module doc for what remains unconfirmed).
/// Missing or unreadable directories yield an empty list rather than an
/// error — a fresh machine with no `~/.claude/projects` yet is a normal,
/// not exceptional, onboarding state.
#[must_use]
pub fn discover_transcripts(claude_dir: &Path) -> Vec<std::path::PathBuf> {
    let projects_dir = claude_dir.join("projects");
    let Ok(project_entries) = std::fs::read_dir(&projects_dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for project_entry in project_entries.flatten() {
        let Ok(file_type) = project_entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Ok(session_files) = std::fs::read_dir(project_entry.path()) else {
            continue;
        };
        for session_entry in session_files.flatten() {
            let path = session_entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                found.push(path);
            }
        }
    }
    found
}

/// Phase 4 completion heuristic: does this session's tail look like a clean
/// completion? Requires **both** signals, deliberately conservative — a
/// false positive here pollutes `successful_constraints` with bad guidance,
/// which is worse than under-populating it:
///
/// 1. None of the session's last `TAIL_WINDOW` tool results are errors
///    (an empty tail — no tool calls at all — passes this check vacuously).
/// 2. The final non-empty assistant text contains an explicit success
///    signal and no failure signal (case-insensitive substring match).
#[must_use]
pub fn session_looks_successful(session: &HistoricalSession) -> bool {
    const SUCCESS_SIGNALS: &[&str] = &[
        "tests pass",
        "all tests passing",
        "✅",
        "successfully",
        "fixed",
        "resolved",
        "shipped",
    ];
    const FAILURE_SIGNALS: &[&str] = &[
        "error",
        "fail",
        "broken",
        "doesn't work",
        "still failing",
        "❌",
    ];

    let tail_clean = session
        .tool_result_errors
        .iter()
        .rev()
        .take(TAIL_WINDOW)
        .all(|is_error| !is_error);
    if !tail_clean {
        return false;
    }

    let Some(final_text) = session.assistant_texts.last() else {
        return false;
    };
    let lower = final_text.to_lowercase();
    let has_success = SUCCESS_SIGNALS.iter().any(|s| lower.contains(s));
    let has_failure = FAILURE_SIGNALS.iter().any(|s| lower.contains(s));
    has_success && !has_failure
}

/// Extract a short "what worked" string for `successful_constraints` —
/// only when [`session_looks_successful`] passes. `None` otherwise, since
/// an unqualified extraction would be a guess, not a signal.
#[must_use]
pub fn extract_success_constraint(session: &HistoricalSession) -> Option<String> {
    if !session_looks_successful(session) {
        return None;
    }
    session
        .assistant_texts
        .last()
        .map(|t| truncate(t, CONSTRAINT_CAP))
}

fn truncate(s: &str, cap: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= cap {
        return s.to_string();
    }
    s.chars().take(cap).collect::<String>() + "…"
}

#[cfg(test)]
#[path = "transcript_import_tests.rs"]
mod tests;
