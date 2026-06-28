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
    },
    /// Throttle/quota signal (`rate_limit_event`).
    RateLimit {
        /// Status string, e.g. `allowed_warning`.
        status: String,
        /// Window type, e.g. `seven_day`.
        limit_type: String,
        /// Window utilization in `[0.0, 1.0]`.
        utilization: f32,
    },
    /// Terminal envelope (`result`).
    Result {
        /// CLI session UUID.
        session_id: String,
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
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
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
    let get = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0).min(u32::MAX as u64) as u32;
    StreamEvent::TokenUsage {
        output_tokens: get("output_tokens"),
        input_tokens: get("input_tokens"),
        cache_read_tokens: get("cache_read_input_tokens"),
    }
}

fn parse_rate_limit(v: &Value) -> StreamEvent {
    let info = v.get("rate_limit_info");
    let util = info
        .and_then(|i| i.get("utilization"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0) as f32;
    StreamEvent::RateLimit {
        status: info.map(|i| str_at(i, "status")).unwrap_or_default(),
        limit_type: info.map(|i| str_at(i, "rateLimitType")).unwrap_or_default(),
        utilization: util,
    }
}

fn parse_result(v: &Value) -> StreamEvent {
    StreamEvent::Result {
        session_id: str_at(v, "session_id"),
        final_text: str_at(v, "result"),
        total_cost_usd: v.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
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
            } => vec![AgentEvent::ApiRetry {
                task_id,
                status: status.clone(),
                limit_type: limit_type.clone(),
                utilization: *utilization,
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
#[allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn one(line: &str) -> StreamEvent {
        let mut v = parse_line(line);
        assert_eq!(v.len(), 1, "expected exactly one event from: {line}");
        v.remove(0)
    }

    #[test]
    fn blank_and_malformed_lines_never_panic() {
        assert!(parse_line("").is_empty());
        assert!(parse_line("   ").is_empty());
        assert_eq!(parse_line("not json at all"), vec![StreamEvent::Other]);
        assert_eq!(parse_line(r#"{"truncated":"#), vec![StreamEvent::Other]);
        assert_eq!(parse_line(r#"{"no_type":true}"#), vec![StreamEvent::Other]);
        assert_eq!(parse_line(r#"{"type":"mystery"}"#), vec![StreamEvent::Other]);
    }

    #[test]
    fn init_carries_session_id_but_no_log_or_structured() {
        let ev = one(r#"{"type":"system","subtype":"init","session_id":"abc","model":"claude-x"}"#);
        assert_eq!(ev.session_id(), Some("abc"));
        assert!(ev.log_line().is_none());
        assert!(ev.structured_events(TaskId::new()).is_empty());
    }

    #[test]
    fn status_maps_to_phase() {
        let ev = one(r#"{"type":"system","subtype":"status","status":"requesting"}"#);
        let id = TaskId::new();
        match &ev.structured_events(id)[0] {
            AgentEvent::Phase { phase, .. } => assert_eq!(phase, "requesting"),
            other => panic!("expected Phase, got {other:?}"),
        }
    }

    #[test]
    fn post_turn_summary_logs_detail_and_phases_category() {
        let ev = one(
            r#"{"type":"system","subtype":"post_turn_summary","status_category":"review_ready","status_detail":"listed dir"}"#,
        );
        assert_eq!(ev.log_line().as_deref(), Some("● listed dir"));
        match &ev.structured_events(TaskId::new())[0] {
            AgentEvent::Phase { phase, .. } => assert_eq!(phase, "review_ready"),
            other => panic!("expected Phase, got {other:?}"),
        }
    }

    #[test]
    fn assistant_text_and_thinking_format_for_log_only() {
        let text = one(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"  hi  "}]}}"#);
        assert_eq!(text.log_line().as_deref(), Some("hi"));
        assert!(text.structured_events(TaskId::new()).is_empty());

        let think = one(
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"let me look"}]}}"#,
        );
        assert_eq!(think.log_line().as_deref(), Some("💭 let me look"));
    }

    #[test]
    fn assistant_tool_use_logs_and_maps_to_tool_call() {
        let ev = one(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"src/main.rs"}}]}}"#,
        );
        assert_eq!(ev.log_line().as_deref(), Some("🔧 Read(src/main.rs)"));
        match &ev.structured_events(TaskId::new())[0] {
            AgentEvent::ToolCall { tool, summary, .. } => {
                assert_eq!(tool, "Read");
                assert_eq!(summary, "src/main.rs");
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_unknown_tool_or_no_input_has_bare_log() {
        let ev = one(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Frobnicate","input":{"x":1}}]}}"#,
        );
        assert_eq!(ev.log_line().as_deref(), Some("🔧 Frobnicate"));
        let bare = one(r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read"}]}}"#);
        assert_eq!(bare.log_line().as_deref(), Some("🔧 Read"));
    }

    #[test]
    fn long_tool_arg_truncates_at_cap() {
        let cmd = "x".repeat(120);
        let line = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Bash","input":{{"command":"{cmd}"}}}}]}}}}"#
        );
        let ev = one(&line);
        let log = ev.log_line().unwrap();
        assert!(log.ends_with("…)"));
        assert_eq!(log.matches('x').count(), ARG_CAP);
    }

    #[test]
    fn multiple_assistant_blocks_decode_in_order() {
        let evs = parse_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"step 1"},{"type":"tool_use","name":"Edit","input":{"file_path":"a.rs"}}]}}"#,
        );
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].log_line().as_deref(), Some("step 1"));
        assert_eq!(evs[1].log_line().as_deref(), Some("🔧 Edit(a.rs)"));
    }

    #[test]
    fn user_tool_result_flags_error_on_stderr() {
        let ok = one(r#"{"type":"user","tool_use_result":{"stdout":"all good"}}"#);
        match &ok.structured_events(TaskId::new())[0] {
            AgentEvent::ToolResult { is_error, preview, .. } => {
                assert!(!is_error);
                assert_eq!(preview, "all good");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
        let err = one(r#"{"type":"user","tool_use_result":{"stdout":"","stderr":"boom"}}"#);
        match &err.structured_events(TaskId::new())[0] {
            AgentEvent::ToolResult { is_error, .. } => assert!(is_error),
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn message_delta_usage_maps_to_token_delta() {
        let ev = one(
            r#"{"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":118,"input_tokens":3,"cache_read_input_tokens":16312}}}"#,
        );
        match &ev.structured_events(TaskId::new())[0] {
            AgentEvent::TokenDelta { output_tokens, cache_read_tokens, .. } => {
                assert_eq!(*output_tokens, 118);
                assert_eq!(*cache_read_tokens, 16312);
            }
            other => panic!("expected TokenDelta, got {other:?}"),
        }
    }

    #[test]
    fn content_block_delta_is_not_pane_facing() {
        // Token-level text deltas coalesce into the assistant line, so a bare
        // content_block_delta produces no events (avoids double-logging).
        assert!(parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hel"}}}"#
        )
        .is_empty());
    }

    #[test]
    fn rate_limit_clamps_utilization_and_maps_to_api_retry() {
        let ev = one(
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","rateLimitType":"seven_day","utilization":1.4}}"#,
        );
        match &ev.structured_events(TaskId::new())[0] {
            AgentEvent::ApiRetry { status, limit_type, utilization, .. } => {
                assert_eq!(status, "allowed_warning");
                assert_eq!(limit_type, "seven_day");
                assert!((*utilization - 1.0).abs() < f32::EPSILON, "clamped to 1.0");
            }
            other => panic!("expected ApiRetry, got {other:?}"),
        }
    }

    #[test]
    fn result_yields_final_text_cost_and_session() {
        let ev = one(
            r#"{"type":"result","subtype":"success","result":"done","total_cost_usd":0.048,"num_turns":3,"session_id":"sess-1"}"#,
        );
        assert_eq!(ev.final_text(), Some("done"));
        assert_eq!(ev.session_id(), Some("sess-1"));
        match &ev.structured_events(TaskId::new())[0] {
            AgentEvent::Cost { cost_usd, num_turns, session_id, .. } => {
                assert!((*cost_usd - 0.048).abs() < 1e-9);
                assert_eq!(*num_turns, 3);
                assert_eq!(session_id, "sess-1");
            }
            other => panic!("expected Cost, got {other:?}"),
        }
    }
}
