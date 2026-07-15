//! Kill-test harness prep — replays the real `STREAM_CAPTURE.jsonl` (the same
//! fixture `parser_robustness.rs` uses for G2) through `parse_line` and
//! `QuotaKillLogScanner`, end to end. Unit tests in
//! `src/quota_kill_log_tests.rs` cover the scanner's logic against synthetic
//! sequences; this proves the whole pipeline — real capture text →
//! `parse_line` → `QuotaKillLogScanner::observe` → `to_ndjson_line` — on a
//! real capture, not just hand-built `StreamEvent`s.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use lopi_agent::claude_events::parse_line;
use lopi_agent::quota_kill_log::{to_ndjson_line, QuotaKillLogScanner};

const CAPTURE: &str = include_str!("fixtures/stream_capture.jsonl");

#[test]
fn real_capture_yields_exactly_one_observation_with_the_real_threshold_fields() {
    let mut scanner = QuotaKillLogScanner::new();
    let mut records = Vec::new();
    for line in CAPTURE.lines() {
        if line.trim().is_empty() {
            continue;
        }
        for ev in parse_line(line) {
            if let Some(record) = scanner.observe(&ev, 1_782_000_000) {
                records.push(record);
            }
        }
    }

    // The fixture has exactly one `rate_limit_event` line (grep-verified).
    assert_eq!(
        records.len(),
        1,
        "expected exactly one observation, got {records:?}"
    );
    let record = &records[0];

    // These are the real captured values (`tests/fixtures/stream_capture.jsonl`
    // line 3) — this is what kill test 1 needs to see preserved verbatim,
    // including the two fields `AgentEvent::ApiRetry` doesn't carry.
    assert_eq!(record.status, "allowed_warning");
    assert_eq!(record.limit_type, "seven_day");
    assert!((record.utilization - 0.92).abs() < f32::EPSILON);
    assert_eq!(record.resets_at, Some(1_782_691_200));
    assert_eq!(record.surpassed_threshold, Some(0.75));
    assert_eq!(record.is_using_overage, Some(false));

    // And it must actually serialize — the format a real session's log file
    // would contain.
    let line = to_ndjson_line(record).expect("record must serialize");
    assert!(line.contains("\"surpassed_threshold\":0.75"));
}
