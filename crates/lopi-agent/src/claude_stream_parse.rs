//! Parsing for the `claude --output-format stream-json` event stream.
//!
//! The CLI emits one NDJSON object per line. This module turns those raw events
//! into the human-readable status lines the cockpit shows live — replacing every
//! hardcoded phase label with Claude's own reporting of what it is doing.

/// One UI-relevant item extracted from a single `stream-json` NDJSON line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StreamItem {
    /// A human-readable status/text line to surface live in the log panel.
    Log(String),
    /// The canonical final response text from the terminal `result` event.
    Final(String),
    /// Total cost in USD from the `result` event.
    Cost(f64),
}

/// Parse one `--output-format stream-json` line into zero or more UI items.
///
/// This is the single source of the live status text — it maps Claude's own
/// events to readable lines:
///   - `assistant` text blocks   → the response text, verbatim
///   - `assistant` thinking       → `💭 <thinking>`
///   - `assistant` tool_use       → `🔧 <Tool>(<arg>)` — what Claude is doing now
///   - `system/post_turn_summary` → `● <status_detail>` — Claude's own summary
///   - `result`                   → final text + total cost
///
/// Blank lines and non-JSON noise yield nothing.
pub(crate) fn parse_stream_event(line: &str) -> Vec<StreamItem> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return vec![];
    };
    match v.get("type").and_then(serde_json::Value::as_str) {
        Some("assistant") => parse_assistant_content(&v),
        Some("system") => parse_system_summary(&v),
        Some("result") => parse_result(&v),
        _ => vec![],
    }
}

/// Extract log lines from an `assistant` event's content blocks.
fn parse_assistant_content(v: &serde_json::Value) -> Vec<StreamItem> {
    let Some(content) = v.pointer("/message/content").and_then(|c| c.as_array()) else {
        return vec![];
    };
    let mut out = vec![];
    for block in content {
        match block.get("type").and_then(serde_json::Value::as_str) {
            Some("text") => push_trimmed(&mut out, block.get("text"), ToString::to_string),
            Some("thinking") => {
                push_trimmed(&mut out, block.get("thinking"), |t| format!("💭 {t}"));
            }
            Some("tool_use") => {
                let name = block
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("tool");
                out.push(StreamItem::Log(format!(
                    "🔧 {}",
                    summarize_tool_input(name, block.get("input"))
                )));
            }
            _ => {}
        }
    }
    out
}

/// Push a `Log` item from an optional string field, trimmed and formatted, when non-empty.
fn push_trimmed(
    out: &mut Vec<StreamItem>,
    field: Option<&serde_json::Value>,
    fmt: impl Fn(&str) -> String,
) {
    if let Some(t) = field.and_then(serde_json::Value::as_str) {
        let t = t.trim();
        if !t.is_empty() {
            out.push(StreamItem::Log(fmt(t)));
        }
    }
}

/// Extract the human-readable `status_detail` from a `post_turn_summary` event.
fn parse_system_summary(v: &serde_json::Value) -> Vec<StreamItem> {
    if v.get("subtype").and_then(serde_json::Value::as_str) != Some("post_turn_summary") {
        return vec![];
    }
    let mut out = vec![];
    push_trimmed(&mut out, v.get("status_detail"), |d| format!("● {d}"));
    out
}

/// Extract the final text and cost from a terminal `result` event.
fn parse_result(v: &serde_json::Value) -> Vec<StreamItem> {
    let mut out = vec![];
    if let Some(t) = v.get("result").and_then(serde_json::Value::as_str) {
        out.push(StreamItem::Final(t.to_string()));
    }
    if let Some(c) = v.get("total_cost_usd").and_then(serde_json::Value::as_f64) {
        out.push(StreamItem::Cost(c));
    }
    out
}

/// Summarize a tool_use block as `Tool(arg)` using the most salient input field,
/// truncated so a long command or path doesn't flood the log.
fn summarize_tool_input(name: &str, input: Option<&serde_json::Value>) -> String {
    let Some(input) = input else {
        return name.to_string();
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
    if !field.is_empty() {
        if let Some(val) = input.get(field).and_then(serde_json::Value::as_str) {
            let short: String = val.chars().take(80).collect();
            let ellipsis = if val.chars().count() > 80 { "…" } else { "" };
            return format!("{name}({short}{ellipsis})");
        }
    }
    name.to_string()
}

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_stream_event_ignores_blank_and_non_json() {
        assert!(parse_stream_event("").is_empty());
        assert!(parse_stream_event("   ").is_empty());
        assert!(parse_stream_event("not json at all").is_empty());
    }

    #[test]
    fn parse_stream_event_ignores_unknown_and_init_events() {
        assert!(parse_stream_event(r#"{"type":"system","subtype":"init"}"#).is_empty());
        assert!(parse_stream_event(r#"{"type":"rate_limit_event"}"#).is_empty());
        assert!(parse_stream_event(r#"{"type":"mystery"}"#).is_empty());
        assert!(parse_stream_event(r#"{"no_type":true}"#).is_empty());
    }

    #[test]
    fn parse_stream_event_extracts_assistant_text() {
        let line =
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"  Hello world  "}]}}"#;
        assert_eq!(
            parse_stream_event(line),
            vec![StreamItem::Log("Hello world".to_string())]
        );
    }

    #[test]
    fn parse_stream_event_skips_empty_text_block() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"   "}]}}"#;
        assert!(parse_stream_event(line).is_empty());
    }

    #[test]
    fn parse_stream_event_assistant_without_content_is_empty() {
        assert!(parse_stream_event(r#"{"type":"assistant","message":{}}"#).is_empty());
    }

    #[test]
    fn parse_stream_event_formats_thinking() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"let me check"}]}}"#;
        assert_eq!(
            parse_stream_event(line),
            vec![StreamItem::Log("💭 let me check".to_string())]
        );
    }

    #[test]
    fn parse_stream_event_summarizes_tool_use_with_known_field() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"src/main.rs"}}]}}"#;
        assert_eq!(
            parse_stream_event(line),
            vec![StreamItem::Log("🔧 Read(src/main.rs)".to_string())]
        );
    }

    #[test]
    fn parse_stream_event_truncates_long_tool_arg() {
        let cmd = "x".repeat(120);
        let line = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Bash","input":{{"command":"{cmd}"}}}}]}}}}"#
        );
        let items = parse_stream_event(&line);
        match &items[0] {
            StreamItem::Log(s) => {
                assert!(s.ends_with("…)"));
                assert!(s.starts_with("🔧 Bash(xxx"));
                assert_eq!(s.matches('x').count(), 80);
            }
            other => panic!("unexpected item: {other:?}"),
        }
    }

    #[test]
    fn parse_stream_event_tool_use_unknown_tool_falls_back_to_name() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Frobnicate","input":{"x":1}}]}}"#;
        assert_eq!(
            parse_stream_event(line),
            vec![StreamItem::Log("🔧 Frobnicate".to_string())]
        );
    }

    #[test]
    fn parse_stream_event_tool_use_no_input_falls_back_to_name() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read"}]}}"#;
        assert_eq!(
            parse_stream_event(line),
            vec![StreamItem::Log("🔧 Read".to_string())]
        );
    }

    #[test]
    fn parse_stream_event_post_turn_summary() {
        let line = r#"{"type":"system","subtype":"post_turn_summary","status_detail":"responded with greeting"}"#;
        assert_eq!(
            parse_stream_event(line),
            vec![StreamItem::Log("● responded with greeting".to_string())]
        );
    }

    #[test]
    fn parse_stream_event_other_system_subtype_ignored() {
        let line = r#"{"type":"system","subtype":"something_else","status_detail":"x"}"#;
        assert!(parse_stream_event(line).is_empty());
    }

    #[test]
    fn parse_stream_event_result_yields_final_and_cost() {
        let line =
            r#"{"type":"result","subtype":"success","result":"the plan","total_cost_usd":0.042}"#;
        assert_eq!(
            parse_stream_event(line),
            vec![
                StreamItem::Final("the plan".to_string()),
                StreamItem::Cost(0.042),
            ]
        );
    }

    #[test]
    fn parse_stream_event_multiple_blocks_in_order() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"step 1"},{"type":"tool_use","name":"Edit","input":{"file_path":"a.rs"}}]}}"#;
        assert_eq!(
            parse_stream_event(line),
            vec![
                StreamItem::Log("step 1".to_string()),
                StreamItem::Log("🔧 Edit(a.rs)".to_string()),
            ]
        );
    }
}
