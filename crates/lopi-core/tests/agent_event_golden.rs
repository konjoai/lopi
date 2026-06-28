#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Golden-fixture decode test (G3, Rust side).
//!
//! The same `tests/fixtures/agent_event_golden.json` is decoded here, in the
//! TypeScript parser test, and in the Swift decoder test. All three must agree
//! on field values so a new `AgentEvent` variant cannot drift between the Rust
//! enum, the web wire parser, and the macOS client.

use lopi_core::AgentEvent;

const GOLDEN: &str = include_str!("fixtures/agent_event_golden.json");
const TASK_ID: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn golden_agent_events_decode_with_expected_fields() {
    let events: Vec<AgentEvent> =
        serde_json::from_str(GOLDEN).expect("golden fixture must decode into AgentEvent");
    assert_eq!(events.len(), 6, "golden fixture covers all six new variants");

    match &events[0] {
        AgentEvent::ToolCall { task_id, tool, summary } => {
            assert_eq!(task_id.0.to_string(), TASK_ID);
            assert_eq!(tool, "Bash");
            assert_eq!(summary, "ls -la");
        }
        other => panic!("event 0 should be ToolCall, got {other:?}"),
    }
    match &events[1] {
        AgentEvent::ToolResult { tool, is_error, preview, .. } => {
            assert_eq!(tool, "Bash");
            assert!(!is_error);
            assert!(preview.contains("notes.txt"));
        }
        other => panic!("event 1 should be ToolResult, got {other:?}"),
    }
    match &events[2] {
        AgentEvent::TokenDelta { output_tokens, input_tokens, cache_read_tokens, .. } => {
            assert_eq!(*output_tokens, 118);
            assert_eq!(*input_tokens, 3);
            assert_eq!(*cache_read_tokens, 16312);
        }
        other => panic!("event 2 should be TokenDelta, got {other:?}"),
    }
    match &events[3] {
        AgentEvent::ApiRetry { status, limit_type, utilization, .. } => {
            assert_eq!(status, "allowed_warning");
            assert_eq!(limit_type, "seven_day");
            assert!((*utilization - 0.92).abs() < 1e-6);
        }
        other => panic!("event 3 should be ApiRetry, got {other:?}"),
    }
    match &events[4] {
        AgentEvent::Cost { cost_usd, num_turns, session_id, .. } => {
            assert!((*cost_usd - 0.0479).abs() < 1e-9);
            assert_eq!(*num_turns, 3);
            assert_eq!(session_id, "4fa68a55-05cf-4878-aa2f-d0edaec6b8a6");
        }
        other => panic!("event 4 should be Cost, got {other:?}"),
    }
    match &events[5] {
        AgentEvent::Phase { phase, .. } => assert_eq!(phase, "review_ready"),
        other => panic!("event 5 should be Phase, got {other:?}"),
    }
}
