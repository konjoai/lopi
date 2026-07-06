//! Gate G2 — statistical parser robustness against the real stream capture.
//!
//! Feeds every line of the real `STREAM_CAPTURE.jsonl` (44 lines) plus
//! hand-crafted malformed / truncated / unknown-type lines through
//! `claude_events::parse_line`. Asserts zero panics, that unknown shapes land
//! in `StreamEvent::Other`, and that the real families are recognized.

use lopi_agent::claude_events::{parse_line, StreamEvent};
use lopi_core::TaskId;

const CAPTURE: &str = include_str!("fixtures/stream_capture.jsonl");

#[test]
fn real_capture_parses_without_panic_and_maps_cleanly() {
    let id = TaskId::new();
    let mut lines = 0usize;
    let mut other = 0usize;
    let mut recognized = 0usize;
    let mut structured = 0usize;
    let mut saw_session_id = false;
    let mut saw_cost = false;

    for line in CAPTURE.lines() {
        if line.trim().is_empty() {
            continue;
        }
        lines += 1;
        for ev in parse_line(line) {
            // Mapping must never panic on any real event.
            let mapped = ev.structured_events(id);
            structured += mapped.len();
            let _ = ev.log_line();
            if ev.session_id().is_some() {
                saw_session_id = true;
            }
            if matches!(ev, StreamEvent::Result { .. }) {
                saw_cost = true;
            }
            if matches!(ev, StreamEvent::Other) {
                other += 1;
            } else {
                recognized += 1;
            }
        }
    }

    assert!(lines >= 30, "expected >= 30 capture lines, got {lines}");
    assert!(recognized > 0, "expected some recognized events");
    assert!(saw_session_id, "init/result must surface a session_id");
    assert!(saw_cost, "capture must contain a terminal result");
    assert!(structured > 0, "expected structured pane events");
    // Sanity: the real capture is mostly well-formed, so Other should be a
    // small minority (thinking/signature deltas decode to nothing, not Other).
    assert!(other <= recognized, "Other={other} recognized={recognized}");
}

#[test]
fn adversarial_lines_never_panic() {
    let bad = [
        "",
        "   ",
        "}{",
        "null",
        "[1,2,3]",
        "\"just a string\"",
        "42",
        r#"{"type":"#,
        r#"{"type":123}"#,
        r#"{"type":"assistant"}"#,
        r#"{"type":"assistant","message":{"content":"not-an-array"}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text"}]}}"#,
        r#"{"type":"stream_event"}"#,
        r#"{"type":"stream_event","event":{"type":"message_delta"}}"#,
        r#"{"type":"rate_limit_event"}"#,
        r#"{"type":"result"}"#,
        r#"{"type":"user"}"#,
        r#"{"type":"totally_unknown_future_type","data":{"nested":true}}"#,
    ];
    let id = TaskId::new();
    for line in bad {
        // Must not panic; mapping the result must not panic either.
        for ev in parse_line(line) {
            let _ = ev.structured_events(id);
            let _ = ev.log_line();
        }
    }
}

#[test]
fn unknown_type_decodes_to_other() {
    assert_eq!(
        parse_line(r#"{"type":"some_new_2027_event"}"#),
        vec![StreamEvent::Other]
    );
}
