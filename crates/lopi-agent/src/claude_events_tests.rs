//! Unit tests for `claude_events.rs` — split out to keep the decoder
//! module under the 500-line file gate. Included via `#[path]` from
//! `claude_events.rs` so `super::*` still resolves to the decoder's items.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

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
    assert_eq!(
        parse_line(r#"{"type":"mystery"}"#),
        vec![StreamEvent::Other]
    );
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
    let text =
        one(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"  hi  "}]}}"#);
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
    let bare =
        one(r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read"}]}}"#);
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
        AgentEvent::ToolResult {
            is_error, preview, ..
        } => {
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
        r#"{"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":118,"input_tokens":3,"cache_read_input_tokens":16312,"cache_creation_input_tokens":6291}}}"#,
    );
    // Cache-creation (write) tokens are captured on the raw event so the
    // billed-token totals stay accurate (bug #3), even though the pane-facing
    // TokenDelta intentionally omits them.
    match &ev {
        StreamEvent::TokenUsage {
            cache_write_tokens, ..
        } => assert_eq!(*cache_write_tokens, 6291),
        other => panic!("expected TokenUsage, got {other:?}"),
    }
    match &ev.structured_events(TaskId::new())[0] {
        AgentEvent::TokenDelta {
            output_tokens,
            cache_read_tokens,
            ..
        } => {
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
        AgentEvent::ApiRetry {
            status,
            limit_type,
            utilization,
            ..
        } => {
            assert_eq!(status, "allowed_warning");
            assert_eq!(limit_type, "seven_day");
            assert!((*utilization - 1.0).abs() < f32::EPSILON, "clamped to 1.0");
        }
        other => panic!("expected ApiRetry, got {other:?}"),
    }
}

#[test]
fn non_success_result_reports_the_halt_reason() {
    // A turn/budget cap halt is surfaced as a log line, not swallowed.
    let ev = one(
        r#"{"type":"result","subtype":"error_max_turns","result":"","total_cost_usd":0.01,"num_turns":1,"session_id":"s"}"#,
    );
    let log = ev
        .log_line()
        .expect("non-success result must produce a log line");
    assert!(log.contains("halted"), "log: {log}");
    assert!(log.contains("error_max_turns"), "log: {log}");
    // It still maps to a Cost event so the UI sees the spend.
    assert!(matches!(
        ev.structured_events(TaskId::new())[0],
        AgentEvent::Cost { .. }
    ));
}

#[test]
fn success_result_has_no_halt_log() {
    let ev = one(
        r#"{"type":"result","subtype":"success","result":"done","total_cost_usd":0.01,"num_turns":1,"session_id":"s"}"#,
    );
    assert!(
        ev.log_line().is_none(),
        "clean success should not log a halt"
    );
}

#[test]
fn result_yields_final_text_cost_and_session() {
    let ev = one(
        r#"{"type":"result","subtype":"success","result":"done","total_cost_usd":0.048,"num_turns":3,"session_id":"sess-1"}"#,
    );
    assert_eq!(ev.final_text(), Some("done"));
    assert_eq!(ev.session_id(), Some("sess-1"));
    match &ev.structured_events(TaskId::new())[0] {
        AgentEvent::Cost {
            cost_usd,
            num_turns,
            session_id,
            ..
        } => {
            assert!((*cost_usd - 0.048).abs() < 1e-9);
            assert_eq!(*num_turns, 3);
            assert_eq!(session_id, "sess-1");
        }
        other => panic!("expected Cost, got {other:?}"),
    }
}
