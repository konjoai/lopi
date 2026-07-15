//! Single decoder for `claude -p --output-format stream-json` NDJSON output.
//!
//! Built against a real capture (`artifacts/STREAM_CAPTURE.jsonl`), not docs.
//! Every line decodes to zero or more [`StreamEvent`]s; anything unrecognized
//! becomes [`StreamEvent::Other`] and never panics. Two views are derived from
//! the same decode:
//!   - [`StreamEvent::log_line`] — the human status string for the log panel
//!     (text verbatim, `💭` thinking, `🔧` tool calls, `●` turn summaries).
//!   - [`StreamEvent::structured_events`] — `lopi_core::AgentEvent`s that drive
//!     the Forge panes (tool timeline, token gauge, cost, phase, rate-limit).

use lopi_core::{AgentEvent, TaskId};
use serde_json::Value;

/// Characters kept from a tool argument or result preview before truncation.
const ARG_CAP: usize = 80;
const PREVIEW_CAP: usize = 240;

/// One decoded, UI-relevant item from a single stream-json NDJSON line.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// `system/init` — session bootstrap. Carries the resumable session UUID.
    Init {
        /// CLI session UUID; resumable with `--resume`.
        session_id: String,
        /// Model id the session is running.
        model: String,
    },
    /// `system/status` — coarse activity, e.g. `requesting`.
    Status(String),
    /// `system/post_turn_summary` — a phase category plus a human detail line.
    TurnSummary {
        /// Coarse category, e.g. `review_ready` — drives the phase signal.
        category: String,
        /// Human detail line for the log panel.
        detail: String,
    },
    /// An assistant text block — surfaced verbatim in the log.
    Text(String),
    /// An assistant thinking block — surfaced as `💭` in the log.
    Thinking(String),
    /// An assistant tool invocation.
    ToolUse {
        /// Tool name, e.g. `Bash`, `Read`.
        tool: String,
        /// Truncated salient input argument (may be empty).
        arg: String,
    },
    /// A tool result (`user` line carrying `tool_use_result`).
    ToolResult {
        /// Whether the tool reported an error.
        is_error: bool,
        /// Truncated preview of the result.
        preview: String,
    },
    /// Incremental token usage (`stream_event/message_delta.usage`).
    TokenUsage {
        /// Output tokens this turn.
        output_tokens: u32,
        /// Input tokens this turn.
        input_tokens: u32,
        /// Cache-read tokens this turn.
        cache_read_tokens: u32,
        /// Cache-creation (cache-write) tokens this turn. Billed and counted in
        /// `daily_token_totals`, so it must be captured for the cost/token
        /// surfaces to reflect real spend (bug #3).
        cache_write_tokens: u32,
    },
    /// Throttle/quota signal (`rate_limit_event`).
    RateLimit {
        /// Status string, e.g. `allowed_warning`.
        status: String,
        /// Window type, e.g. `seven_day`.
        limit_type: String,
        /// Window utilization in `[0.0, 1.0]`.
        utilization: f32,
        /// When this window resets, unix seconds — `None` if the CLI omitted
        /// `resetsAt`. Five-hour windows are rolling from first use, not
        /// wall-clock fixed, so this is the only reliable way to know when.
        resets_at: Option<i64>,
        /// The CLI's own `surpassedThreshold`, if present — the answer to
        /// MAXX kill test 1 (is the event threshold-gated or does it fire
        /// every turn) lives in this field, not in whether the event fired at
        /// all. `None` for a build of the CLI that omits it; not forwarded
        /// into `AgentEvent::ApiRetry` today (no consumer needs it yet) but
        /// kept here so `quota_kill_log` can log it verbatim.
        surpassed_threshold: Option<f32>,
        /// The CLI's own `isUsingOverage`, if present — same rationale as
        /// `surpassed_threshold`.
        is_using_overage: Option<bool>,
    },
    /// Terminal envelope (`result`).
    Result {
        /// CLI session UUID.
        session_id: String,
        /// Result subtype, e.g. `success` or an error/cap subtype. A value
        /// other than `success` means the CLI halted early (turn/budget cap).
        subtype: String,
        /// Canonical final response text.
        final_text: String,
        /// Cumulative cost in USD.
        total_cost_usd: f64,
        /// Number of turns completed.
        num_turns: u32,
    },
    /// Any unrecognized or unparseable line — a no-op, never panics.
    Other,
}

/// Parse one NDJSON line into zero or more [`StreamEvent`]s.
///
/// A single `assistant` line may carry several content blocks, so this returns
/// a `Vec`. Blank lines yield an empty `Vec`; malformed JSON yields
/// `[StreamEvent::Other]` rather than an error.
#[must_use]
pub fn parse_line(line: &str) -> Vec<StreamEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
        return vec![StreamEvent::Other];
    };
    match v.get("type").and_then(Value::as_str) {
        Some("system") => vec![parse_system(&v)],
        Some("assistant") => parse_assistant(&v),
        Some("user") => parse_user(&v),
        Some("stream_event") => parse_stream_event(&v),
        Some("rate_limit_event") => vec![parse_rate_limit(&v)],
        Some("result") => vec![parse_result(&v)],
        _ => vec![StreamEvent::Other],
    }
}

fn str_at(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn parse_system(v: &Value) -> StreamEvent {
    match v.get("subtype").and_then(Value::as_str) {
        Some("init") => StreamEvent::Init {
            session_id: str_at(v, "session_id"),
            model: str_at(v, "model"),
        },
        Some("status") => StreamEvent::Status(str_at(v, "status")),
        Some("post_turn_summary") => StreamEvent::TurnSummary {
            category: str_at(v, "status_category"),
            detail: str_at(v, "status_detail"),
        },
        _ => StreamEvent::Other,
    }
}

fn parse_assistant(v: &Value) -> Vec<StreamEvent> {
    let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) else {
        return vec![StreamEvent::Other];
    };
    let out: Vec<StreamEvent> = blocks.iter().filter_map(parse_assistant_block).collect();
    if out.is_empty() {
        vec![StreamEvent::Other]
    } else {
        out
    }
}

fn parse_assistant_block(block: &Value) -> Option<StreamEvent> {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => non_empty(str_at(block, "text")).map(StreamEvent::Text),
        Some("thinking") => non_empty(str_at(block, "thinking")).map(StreamEvent::Thinking),
        Some("tool_use") => Some(StreamEvent::ToolUse {
            tool: str_at(block, "name"),
            arg: tool_arg(&str_at(block, "name"), block.get("input")),
        }),
        _ => None,
    }
}

/// Pick the salient input field for a tool and truncate it for display.
fn tool_arg(name: &str, input: Option<&Value>) -> String {
    let Some(input) = input else {
        return String::new();
    };
    let field = match name {
        "Read" | "Edit" | "Write" | "NotebookEdit" => "file_path",
        "Bash" => "command",
        "Grep" | "Glob" => "pattern",
        "Task" | "Agent" => "description",
        "WebFetch" => "url",
        "WebSearch" => "query",
        _ => "",
    };
    if field.is_empty() {
        return String::new();
    }
    input
        .get(field)
        .and_then(Value::as_str)
        .map(|s| truncate(s, ARG_CAP))
        .unwrap_or_default()
}

fn parse_user(v: &Value) -> Vec<StreamEvent> {
    let result = v.get("tool_use_result");
    let is_error = result
        .and_then(|r| r.get("is_error"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || result
            .and_then(|r| r.get("stderr"))
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
    vec![StreamEvent::ToolResult {
        is_error,
        preview: tool_result_preview(result),
    }]
}

fn tool_result_preview(result: Option<&Value>) -> String {
    let Some(r) = result else {
        return String::new();
    };
    if let Some(s) = r.get("stdout").and_then(Value::as_str) {
        return truncate(s, PREVIEW_CAP);
    }
    if let Some(f) = r.pointer("/file/filePath").and_then(Value::as_str) {
        return truncate(f, PREVIEW_CAP);
    }
    truncate(&r.to_string(), PREVIEW_CAP)
}

fn parse_stream_event(v: &Value) -> Vec<StreamEvent> {
    let Some(event) = v.get("event") else {
        return vec![StreamEvent::Other];
    };
    match event.get("type").and_then(Value::as_str) {
        Some("message_delta") => vec![parse_usage(event.get("usage"))],
        _ => Vec::new(), // block deltas are coalesced into the assistant line
    }
}

fn parse_usage(usage: Option<&Value>) -> StreamEvent {
    let Some(u) = usage else {
        return StreamEvent::Other;
    };
    let get = |k: &str| {
        u.get(k)
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32
    };
    StreamEvent::TokenUsage {
        output_tokens: get("output_tokens"),
        input_tokens: get("input_tokens"),
        cache_read_tokens: get("cache_read_input_tokens"),
        cache_write_tokens: get("cache_creation_input_tokens"),
    }
}

fn parse_rate_limit(v: &Value) -> StreamEvent {
    let info = v.get("rate_limit_info");
    let util = info
        .and_then(|i| i.get("utilization"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0) as f32;
    let resets_at = info.and_then(|i| i.get("resetsAt")).and_then(Value::as_i64);
    let surpassed_threshold = info
        .and_then(|i| i.get("surpassedThreshold"))
        .and_then(Value::as_f64)
        .map(|f| f as f32);
    let is_using_overage = info
        .and_then(|i| i.get("isUsingOverage"))
        .and_then(Value::as_bool);
    StreamEvent::RateLimit {
        status: info.map(|i| str_at(i, "status")).unwrap_or_default(),
        limit_type: info.map(|i| str_at(i, "rateLimitType")).unwrap_or_default(),
        utilization: util,
        resets_at,
        surpassed_threshold,
        is_using_overage,
    }
}

fn parse_result(v: &Value) -> StreamEvent {
    StreamEvent::Result {
        session_id: str_at(v, "session_id"),
        subtype: str_at(v, "subtype"),
        final_text: str_at(v, "result"),
        total_cost_usd: v
            .get("total_cost_usd")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        num_turns: v.get("num_turns").and_then(Value::as_u64).unwrap_or(0) as u32,
    }
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn truncate(s: &str, cap: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= cap {
        return s.to_string();
    }
    s.chars().take(cap).collect::<String>() + "…"
}

impl StreamEvent {
    /// Human status string for the log panel, or `None` if this event has no
    /// log-facing text. Preserves the original `claude_stream_parse` formatting.
    #[must_use]
    pub fn log_line(&self) -> Option<String> {
        match self {
            StreamEvent::Text(t) => Some(t.clone()),
            StreamEvent::Thinking(t) => Some(format!("💭 {t}")),
            StreamEvent::ToolUse { tool, arg } if arg.is_empty() => Some(format!("🔧 {tool}")),
            StreamEvent::ToolUse { tool, arg } => Some(format!("🔧 {tool}({arg})")),
            StreamEvent::TurnSummary { detail, .. } if !detail.is_empty() => {
                Some(format!("● {detail}"))
            }
            // A non-success terminal means the CLI halted early — surface the
            // reason (e.g. a turn or budget cap) instead of failing silently.
            StreamEvent::Result {
                subtype, num_turns, ..
            } if subtype != "success" => {
                Some(format!("⛔ halted ({subtype}) after {num_turns} turns"))
            }
            _ => None,
        }
    }

    /// The canonical final response text, if this is the terminal `result`.
    #[must_use]
    pub fn final_text(&self) -> Option<&str> {
        match self {
            StreamEvent::Result { final_text, .. } => Some(final_text),
            _ => None,
        }
    }

    /// The session UUID carried by `init`/`result`, for `--resume` persistence.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        match self {
            StreamEvent::Init { session_id, .. } | StreamEvent::Result { session_id, .. } => {
                Some(session_id)
            }
            _ => None,
        }
    }

    /// Structured `AgentEvent`s that drive the Forge panes. Text/thinking are
    /// excluded here — they reach the UI through [`Self::log_line`] as
    /// `LogLine`s, so the thought stream is not double-fed.
    #[must_use]
    pub fn structured_events(&self, task_id: TaskId) -> Vec<AgentEvent> {
        match self {
            StreamEvent::ToolUse { tool, arg } => vec![AgentEvent::ToolCall {
                task_id,
                tool: tool.clone(),
                summary: arg.clone(),
            }],
            StreamEvent::ToolResult { is_error, preview } => vec![AgentEvent::ToolResult {
                task_id,
                tool: String::new(),
                is_error: *is_error,
                preview: preview.clone(),
            }],
            StreamEvent::TokenUsage {
                output_tokens,
                input_tokens,
                cache_read_tokens,
                ..
            } => vec![AgentEvent::TokenDelta {
                task_id,
                output_tokens: *output_tokens,
                input_tokens: *input_tokens,
                cache_read_tokens: *cache_read_tokens,
            }],
            StreamEvent::RateLimit {
                status,
                limit_type,
                utilization,
                resets_at,
                ..
            } => vec![AgentEvent::ApiRetry {
                task_id,
                status: status.clone(),
                limit_type: limit_type.clone(),
                utilization: *utilization,
                resets_at: *resets_at,
            }],
            StreamEvent::Status(phase) => vec![AgentEvent::Phase {
                task_id,
                phase: phase.clone(),
            }],
            StreamEvent::TurnSummary { category, .. } => vec![AgentEvent::Phase {
                task_id,
                phase: category.clone(),
            }],
            StreamEvent::Result {
                session_id,
                total_cost_usd,
                num_turns,
                ..
            } => vec![AgentEvent::Cost {
                task_id,
                cost_usd: *total_cost_usd,
                num_turns: *num_turns,
                session_id: session_id.clone(),
            }],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "claude_events_tests.rs"]
mod tests;
