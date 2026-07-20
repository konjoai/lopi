#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::claude::MODEL_SONNET;

#[test]
fn usage_cost_sonnet() {
    let u = ApiUsage {
        input_tokens: 1_000_000,
        ..ApiUsage::default()
    };
    let cost = u.estimated_cost(MODEL_SONNET);
    assert!(
        (cost - 3.0).abs() < 0.01,
        "sonnet input rate should be $3/MTok"
    );
}

/// Part 4.1 — `MODEL_OPUS` (`claude-opus-4-7`, live) must price at the
/// current $5/$25 rate, not the retired Opus 4.1 $15/$75 rate this
/// estimator carried before — every burn chart computed against a real
/// Opus session was over-reporting spend by roughly 3x.
#[test]
fn usage_cost_opus_uses_current_not_retired_rate() {
    let input = ApiUsage {
        input_tokens: 1_000_000,
        ..ApiUsage::default()
    };
    let output = ApiUsage {
        output_tokens: 1_000_000,
        ..ApiUsage::default()
    };
    assert!(
        (input.estimated_cost(crate::claude::MODEL_OPUS) - 5.0).abs() < 0.01,
        "opus input rate should be $5/MTok, not the retired $15/MTok"
    );
    assert!(
        (output.estimated_cost(crate::claude::MODEL_OPUS) - 25.0).abs() < 0.01,
        "opus output rate should be $25/MTok, not the retired $75/MTok"
    );
}

#[test]
fn usage_cost_haiku_rate() {
    let input = ApiUsage {
        input_tokens: 1_000_000,
        ..ApiUsage::default()
    };
    let output = ApiUsage {
        output_tokens: 1_000_000,
        ..ApiUsage::default()
    };
    assert!((input.estimated_cost(MODEL_HAIKU) - 1.0).abs() < 0.01);
    assert!((output.estimated_cost(MODEL_HAIKU) - 5.0).abs() < 0.01);
}

#[test]
fn usage_cost_sonnet_output_rate() {
    let output = ApiUsage {
        output_tokens: 1_000_000,
        ..ApiUsage::default()
    };
    assert!((output.estimated_cost(MODEL_SONNET) - 15.0).abs() < 0.01);
}

/// Cache rates scale off each model's own input rate (~10% read, ~1.25x
/// write) — pinned per-model so a future input-rate change can't silently
/// leave the cache multipliers stale relative to it.
#[test]
fn usage_cost_cache_rates_scale_with_model_input_rate() {
    let read = ApiUsage {
        cache_read_tokens: 1_000_000,
        ..ApiUsage::default()
    };
    let write = ApiUsage {
        cache_write_tokens: 1_000_000,
        ..ApiUsage::default()
    };
    assert!((read.estimated_cost(crate::claude::MODEL_OPUS) - 0.50).abs() < 0.01);
    assert!((write.estimated_cost(crate::claude::MODEL_OPUS) - 6.25).abs() < 0.01);
    assert!((read.estimated_cost(MODEL_HAIKU) - 0.10).abs() < 0.01);
    assert!((write.estimated_cost(MODEL_HAIKU) - 1.25).abs() < 0.01);
    assert!((read.estimated_cost(MODEL_SONNET) - 0.30).abs() < 0.01);
    assert!((write.estimated_cost(MODEL_SONNET) - 3.75).abs() < 0.01);
}

#[test]
fn usage_cost_cache_hit_cheaper() {
    let full = ApiUsage {
        input_tokens: 100_000,
        ..ApiUsage::default()
    };
    let cached = ApiUsage {
        cache_read_tokens: 100_000,
        ..ApiUsage::default()
    };
    assert!(
        cached.estimated_cost(MODEL_SONNET) < full.estimated_cost(MODEL_SONNET),
        "cache read must be cheaper than full input"
    );
}

#[test]
fn shared_http_returns_same_instance() {
    let a = shared_http();
    let b = shared_http();
    assert!(Arc::ptr_eq(&a, &b), "shared_http must return the same Arc");
}

// ── decode_sse_stream ────────────────────────────────────────────────────────

fn reader_for(sse: &str) -> tokio::io::BufReader<std::io::Cursor<Vec<u8>>> {
    tokio::io::BufReader::new(std::io::Cursor::new(sse.as_bytes().to_vec()))
}

fn text_delta_line(text: &str) -> String {
    format!(
        "data: {}\n",
        serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": text},
        })
    )
}

#[tokio::test]
async fn decode_sse_stream_accumulates_text_and_invokes_on_delta() {
    let sse = format!(
        "{}{}",
        text_delta_line("Hello, "),
        text_delta_line("world!")
    );
    let mut deltas = Vec::new();
    let (text, _usage) = decode_sse_stream(reader_for(&sse), &mut |t| deltas.push(t.to_string()))
        .await
        .unwrap();
    assert_eq!(text, "Hello, world!");
    assert_eq!(deltas, vec!["Hello, ".to_string(), "world!".to_string()]);
}

#[tokio::test]
async fn decode_sse_stream_ignores_event_lines() {
    let sse = format!("event: content_block_delta\n{}", text_delta_line("hi"));
    let (text, _usage) = decode_sse_stream(reader_for(&sse), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(text, "hi");
}

#[tokio::test]
async fn decode_sse_stream_stops_at_done_marker() {
    let sse = format!(
        "{}data: [DONE]\n{}",
        text_delta_line("before"),
        text_delta_line("after")
    );
    let (text, _usage) = decode_sse_stream(reader_for(&sse), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(
        text, "before",
        "[DONE] must stop processing, dropping anything after it"
    );
}

/// A malformed `data:` line must be skipped, not abort the whole stream —
/// the surrounding well-formed deltas still accumulate.
#[tokio::test]
async fn decode_sse_stream_skips_malformed_json_and_continues() {
    let sse = format!(
        "{}data: {{not valid json\n{}",
        text_delta_line("before"),
        text_delta_line("after")
    );
    let (text, _usage) = decode_sse_stream(reader_for(&sse), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(text, "beforeafter");
}

/// An SSE `error` event must propagate as an `Err`, not be silently
/// swallowed like an unrecognized event type.
#[tokio::test]
async fn decode_sse_stream_propagates_error_event() {
    let sse = format!(
        "data: {}\n",
        serde_json::json!({
            "type": "error",
            "error": {"type": "overloaded_error", "message": "server overloaded"},
        })
    );
    let err = decode_sse_stream(reader_for(&sse), &mut |_| {})
        .await
        .unwrap_err();
    assert!(err.to_string().contains("server overloaded"));
}

/// A stream that ends without `[DONE]` or a terminal `message_stop` (the
/// connection just closes) must still return whatever accumulated, not error.
#[tokio::test]
async fn decode_sse_stream_returns_accumulated_state_when_stream_ends_abruptly() {
    let sse = text_delta_line("partial");
    let (text, _usage) = decode_sse_stream(reader_for(&sse), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(text, "partial");
}

#[tokio::test]
async fn decode_sse_stream_accumulates_usage_from_message_start_and_message_delta() {
    let sse = format!(
        "data: {}\ndata: {}\n",
        serde_json::json!({
            "type": "message_start",
            "message": {
                "usage": {
                    "input_tokens": 100,
                    "cache_read_input_tokens": 20,
                    "cache_creation_input_tokens": 5,
                }
            },
        }),
        serde_json::json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"output_tokens": 50},
        }),
    );
    let (_text, usage) = decode_sse_stream(reader_for(&sse), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.cache_read_tokens, 20);
    assert_eq!(usage.cache_write_tokens, 5);
    assert_eq!(usage.output_tokens, 50);
}

/// `message_delta` events without a `usage` field (mid-stream deltas that
/// only carry `stop_reason` progress) must not affect the accumulated usage.
#[tokio::test]
async fn decode_sse_stream_message_delta_without_usage_is_a_no_op() {
    let sse = format!(
        "data: {}\n",
        serde_json::json!({
            "type": "message_delta",
            "delta": {"stop_reason": null},
            "usage": null,
        }),
    );
    let (_text, usage) = decode_sse_stream(reader_for(&sse), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(usage.output_tokens, 0);
}

/// Unrecognized/ignored event types (`ping`, `content_block_start`,
/// `content_block_stop`, `message_stop`) must not affect accumulated state.
#[tokio::test]
async fn decode_sse_stream_ignores_unhandled_event_types() {
    let sse = format!(
        "data: {}\ndata: {}\n{}",
        serde_json::json!({"type": "ping"}),
        serde_json::json!({"type": "message_stop"}),
        text_delta_line("still works"),
    );
    let (text, _usage) = decode_sse_stream(reader_for(&sse), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(text, "still works");
}
