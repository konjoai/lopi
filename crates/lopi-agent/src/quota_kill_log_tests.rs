//! Unit tests for `quota_kill_log.rs`, replayed against synthetic
//! `StreamEvent` sequences (no live session, no real `claude` auth — see the
//! module doc comment). The acceptance bar per the sprint brief: feed a
//! threshold-gated pattern and an every-turn pattern, and confirm the output
//! makes the two distinguishable at a glance — operationalized below as an
//! assertion on `events_since_last`'s magnitude, since that's the number a
//! human actually reads.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn rate_limit(utilization: f32, surpassed_threshold: Option<f32>) -> StreamEvent {
    StreamEvent::RateLimit {
        status: "allowed_warning".to_string(),
        limit_type: "seven_day".to_string(),
        utilization,
        resets_at: Some(1_782_691_200),
        surpassed_threshold,
        is_using_overage: Some(false),
    }
}

fn text_turn() -> StreamEvent {
    StreamEvent::Text("a turn's worth of assistant text".to_string())
}

#[test]
fn every_turn_pattern_has_small_constant_gaps() {
    // Fires after every single turn: Text, RateLimit, Text, RateLimit, ...
    let mut scanner = QuotaKillLogScanner::new();
    let mut gaps = Vec::new();
    for _ in 0..6 {
        assert!(scanner.observe(&text_turn(), 1000).is_none());
        let record = scanner
            .observe(&rate_limit(0.4, None), 1000)
            .expect("rate_limit_event should be observed every turn");
        gaps.push(record.events_since_last);
    }
    // Every gap is the same small size (one text turn + the event itself) —
    // this is the "fires every turn" signature: uniform, small.
    assert!(
        gaps.iter().all(|g| *g == 2),
        "expected uniform small gaps, got {gaps:?}"
    );
}

#[test]
fn threshold_gated_pattern_has_one_large_gap_then_small_ones() {
    // 50 turns pass with no rate_limit_event at all (below threshold), then
    // it starts firing every turn once utilization crosses 0.75 — the
    // "threshold-gated" signature: one large gap, then small ones.
    let mut scanner = QuotaKillLogScanner::new();
    for _ in 0..50 {
        assert!(scanner.observe(&text_turn(), 1000).is_none());
    }
    let first = scanner
        .observe(&rate_limit(0.76, Some(0.75)), 1000)
        .expect("first observation once past threshold");
    assert_eq!(
        first.events_since_last, 51,
        "the 50 silent turns plus this event"
    );
    assert_eq!(first.surpassed_threshold, Some(0.75));

    let mut later_gaps = Vec::new();
    for _ in 0..3 {
        assert!(scanner.observe(&text_turn(), 1000).is_none());
        let record = scanner.observe(&rate_limit(0.8, Some(0.75)), 1000).unwrap();
        later_gaps.push(record.events_since_last);
    }

    // The defining contrast: the pre-threshold gap dwarfs every post-threshold
    // one — a human scanning `events_since_last` sees this immediately,
    // whereas the every-turn pattern above never produces a gap this large.
    assert!(
        first.events_since_last > later_gaps.iter().max().copied().unwrap_or(0) * 10,
        "expected the first threshold-crossing gap ({}) to dwarf later gaps ({later_gaps:?})",
        first.events_since_last
    );
}

#[test]
fn non_rate_limit_events_advance_counters_but_log_nothing() {
    let mut scanner = QuotaKillLogScanner::new();
    assert!(scanner
        .observe(
            &StreamEvent::ToolUse {
                tool: "Bash".into(),
                arg: String::new()
            },
            1000
        )
        .is_none());
    assert!(scanner
        .observe(&StreamEvent::Thinking("hmm".into()), 1000)
        .is_none());
    assert!(scanner.observe(&text_turn(), 1000).is_none());

    let record = scanner.observe(&rate_limit(0.5, None), 1000).unwrap();
    // All three prior non-matching events count toward the gap, but only the
    // one `Text` counts as a turn.
    assert_eq!(record.events_since_last, 4);
    assert_eq!(record.text_turns_since_last, 1);
}

#[test]
fn resets_at_reliability_survives_verbatim_for_both_window_types() {
    // Kill test 1's second question: is `resetsAt` reliably present for both
    // `five_hour` and `seven_day`. The scanner doesn't answer this itself —
    // it just has to not lose the information, so a human can scan the
    // logged column across a real session.
    let mut scanner = QuotaKillLogScanner::new();
    let five_hour = StreamEvent::RateLimit {
        status: "allowed".to_string(),
        limit_type: "five_hour".to_string(),
        utilization: 0.3,
        resets_at: None, // the exact case kill test 1 asks about
        surpassed_threshold: None,
        is_using_overage: None,
    };
    let record = scanner.observe(&five_hour, 1000).unwrap();
    assert_eq!(record.limit_type, "five_hour");
    assert_eq!(record.resets_at, None);

    let seven_day = rate_limit(0.92, Some(0.75));
    let record = scanner.observe(&seven_day, 1000).unwrap();
    assert_eq!(record.limit_type, "seven_day");
    assert_eq!(record.resets_at, Some(1_782_691_200));
}

#[test]
fn ndjson_line_round_trips_and_keeps_raw_threshold_fields() {
    let mut scanner = QuotaKillLogScanner::new();
    let record = scanner
        .observe(&rate_limit(0.92, Some(0.75)), 1_700_000_000)
        .unwrap();
    let line = to_ndjson_line(&record).expect("plain-data struct always serializes");
    assert!(
        !line.contains('\n'),
        "one NDJSON line must not embed a newline"
    );

    let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(parsed["surpassed_threshold"], serde_json::json!(0.75));
    assert_eq!(parsed["utilization"], serde_json::json!(0.92));
    assert_eq!(
        parsed["observed_at_unix"],
        serde_json::json!(1_700_000_000_i64)
    );
}

#[test]
fn first_observation_counts_from_scanner_creation() {
    let mut scanner = QuotaKillLogScanner::new();
    let record = scanner.observe(&rate_limit(0.1, None), 1000).unwrap();
    assert_eq!(record.events_since_last, 1);
    assert_eq!(record.text_turns_since_last, 0);
}
