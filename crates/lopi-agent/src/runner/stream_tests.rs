//! Unit tests for `stream.rs` — split out to keep that module under the
//! 500-line CI file-size gate. Included via `#[path]` from `stream.rs`,
//! so `super::*` resolves to the streaming module’s items.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn token_usage_event_sums_input_and_output() {
    let ev = StreamEvent::TokenUsage {
        output_tokens: 120,
        input_tokens: 80,
        cache_read_tokens: 5000,
        cache_write_tokens: 200,
    };
    // Cache reads/writes are excluded; only input + output count.
    assert_eq!(event_tokens(&ev), 200);
}

#[test]
fn non_usage_events_contribute_no_tokens() {
    assert_eq!(event_tokens(&StreamEvent::Other), 0);
    assert_eq!(event_tokens(&StreamEvent::Status("x".into())), 0);
}

fn usage(output: u32, input: u32, read: u32, write: u32) -> StreamEvent {
    StreamEvent::TokenUsage {
        output_tokens: output,
        input_tokens: input,
        cache_read_tokens: read,
        cache_write_tokens: write,
    }
}

fn result(cost: f64) -> StreamEvent {
    StreamEvent::Result {
        session_id: "s".into(),
        subtype: "success".into(),
        final_text: String::new(),
        total_cost_usd: cost,
        num_turns: 3,
        usage: None,
    }
}

fn result_with_usage(cost: f64, usage: crate::claude_events::ResultUsage) -> StreamEvent {
    StreamEvent::Result {
        session_id: "s".into(),
        subtype: "success".into(),
        final_text: String::new(),
        total_cost_usd: cost,
        num_turns: 3,
        usage: Some(usage),
    }
}

#[test]
fn accrual_sums_token_deltas_across_events() {
    let acc = UsageAccrual::default();
    acc.observe(&usage(100, 3, 16_000, 6_000));
    acc.observe(&usage(50, 1, 22_000, 100));
    assert_eq!(acc.output.saturating_u32(), 150);
    assert_eq!(acc.input.saturating_u32(), 4);
    assert_eq!(acc.cache_read.saturating_u32(), 38_000);
    assert_eq!(acc.cache_write.saturating_u32(), 6_100);
}

#[test]
fn accrual_captures_billed_cost_from_result() {
    let acc = UsageAccrual::default();
    acc.observe(&usage(10, 1, 0, 0));
    acc.observe(&result(0.047_891_6));
    assert!((acc.cost_usd() - 0.047_891_6).abs() < f64::EPSILON);
    assert!(acc.has_usage());
}

#[test]
fn accrual_falls_back_to_deltas_without_authoritative_usage() {
    // No `result.usage` (parent-only run): the persisted counts are the
    // summed `TokenUsage` deltas, unchanged from before this field.
    let acc = UsageAccrual::default();
    acc.observe(&usage(150, 4, 38_000, 6_100));
    acc.observe(&result(0.05));
    assert_eq!(acc.input_tokens(), 4);
    assert_eq!(acc.output_tokens(), 150);
    assert_eq!(acc.cache_read_tokens(), 38_000);
    assert_eq!(acc.cache_write_tokens(), 6_100);
}

#[test]
fn accrual_prefers_authoritative_usage_over_parent_deltas() {
    // A fan-out run: the parent's `TokenUsage` deltas are tiny, but the
    // terminal `result` reports the sub-agent-inclusive total. The
    // persisted counts must reflect the authoritative total, not the
    // parent-only deltas — the bug this field fixes.
    let acc = UsageAccrual::default();
    acc.observe(&usage(3_795, 737, 150_875, 0)); // parent only
    acc.observe(&result_with_usage(
        6.79,
        crate::claude_events::ResultUsage {
            input_tokens: 45_000,
            output_tokens: 220_000,
            cache_read_tokens: 4_100_000,
            cache_write_tokens: 90_000,
        },
    ));
    assert_eq!(acc.input_tokens(), 45_000);
    assert_eq!(acc.output_tokens(), 220_000);
    assert_eq!(acc.cache_read_tokens(), 4_100_000);
    assert_eq!(acc.cache_write_tokens(), 90_000);
    // Cost is still the authoritative billed total from the same envelope.
    assert!((acc.cost_usd() - 6.79).abs() < f64::EPSILON);
}

#[test]
fn accrual_reports_no_usage_when_empty() {
    let acc = UsageAccrual::default();
    assert!(!acc.has_usage());
    assert_eq!(acc.cost_usd(), 0.0);
}

#[test]
fn accrual_has_usage_on_result_even_with_zero_tokens() {
    let acc = UsageAccrual::default();
    acc.observe(&result(0.0));
    // A completed run that happened to spend nothing is still a real turn.
    assert!(acc.has_usage());
}

// ── Budget & Guardrail Controls Part 4.2 — soft-warn at 80% of cap ──────

#[test]
fn check_soft_warn_disabled_when_cap_is_zero() {
    let acc = UsageAccrual::default();
    acc.observe(&usage(1_000_000, 1_000_000, 0, 0));
    assert_eq!(acc.check_soft_warn(crate::claude::MODEL_OPUS, 0.0), None);
}

#[test]
fn check_soft_warn_none_under_the_threshold() {
    let acc = UsageAccrual::default();
    // A trickle of tokens on a generous cap — nowhere near 80%.
    acc.observe(&usage(10, 10, 0, 0));
    assert_eq!(acc.check_soft_warn(crate::claude::MODEL_SONNET, 10.0), None);
}

#[test]
fn check_soft_warn_fires_once_at_80_percent() {
    let acc = UsageAccrual::default();
    // Sonnet: 300K output tokens * $15/MTok = $4.50 — 90% of a $5 cap.
    acc.observe(&usage(300_000, 0, 0, 0));
    let first = acc.check_soft_warn(crate::claude::MODEL_SONNET, 5.0);
    assert!(first.is_some(), "must fire once 80% of the cap is crossed");
    assert!((first.unwrap() - 4.5).abs() < 0.01);

    // A second observation at the same (or higher) usage must not
    // re-fire — the warn latches per stream.
    acc.observe(&usage(10, 0, 0, 0));
    assert_eq!(
        acc.check_soft_warn(crate::claude::MODEL_SONNET, 5.0),
        None,
        "must not fire a second time for the same stream"
    );
}

#[test]
fn emit_budget_soft_warn_sends_the_structured_event() {
    let bus: lopi_core::EventBus<AgentEvent> = lopi_core::EventBus::new(4);
    let mut rx = bus.subscribe();
    let tid = lopi_core::TaskId::new();
    emit_budget_soft_warn(&bus, tid, 4.5, 5.0);
    let ev = rx.try_recv().expect("event should have been sent");
    match ev {
        AgentEvent::BudgetSoftWarn {
            task_id,
            estimated_usd,
            cap_usd,
        } => {
            assert_eq!(task_id, tid);
            assert!((estimated_usd - 4.5).abs() < f64::EPSILON);
            assert!((cap_usd - 5.0).abs() < f64::EPSILON);
        }
        other => panic!("expected BudgetSoftWarn, got {other:?}"),
    }
}

// ── Budget hard-stop — lopi's own kill switch at 100% of cap ────────────

#[test]
fn check_hard_stop_disabled_when_cap_is_zero() {
    let acc = UsageAccrual::default();
    acc.observe(&usage(1_000_000, 1_000_000, 0, 0));
    assert_eq!(acc.check_hard_stop(crate::claude::MODEL_OPUS, 0.0), None);
}

#[test]
fn check_hard_stop_none_under_the_cap() {
    let acc = UsageAccrual::default();
    // 90% of cap (same fixture as the soft-warn test) must not hard-stop —
    // 80% and 100% are genuinely different thresholds.
    acc.observe(&usage(300_000, 0, 0, 0));
    assert_eq!(acc.check_hard_stop(crate::claude::MODEL_SONNET, 5.0), None);
}

#[test]
fn check_hard_stop_fires_at_the_95_percent_margin_not_only_at_100() {
    let acc = UsageAccrual::default();
    // Sonnet: 317K output tokens * $15/MTok = $4.755 — 95.1% of a $5 cap,
    // short of 100%. The margin exists so a burst of tokens that streams
    // in between this check and the subprocess actually exiting can't push
    // realized spend past the cap — pins that it fires here, not only once
    // the estimate reaches the cap outright.
    acc.observe(&usage(317_000, 0, 0, 0));
    let first = acc.check_hard_stop(crate::claude::MODEL_SONNET, 5.0);
    assert!(
        first.is_some(),
        "must fire once the margin (95% of cap) is crossed, before 100%"
    );
}

#[test]
fn check_hard_stop_fires_once_at_100_percent() {
    let acc = UsageAccrual::default();
    // Sonnet: 400K output tokens * $15/MTok = $6.00 — over a $5 cap.
    acc.observe(&usage(400_000, 0, 0, 0));
    let first = acc.check_hard_stop(crate::claude::MODEL_SONNET, 5.0);
    assert!(first.is_some(), "must fire once the cap is reached/crossed");
    assert!((first.unwrap() - 6.0).abs() < 0.01);

    // Latches — a second poll for the same stream (about to be killed)
    // must not re-request the abort.
    assert_eq!(
        acc.check_hard_stop(crate::claude::MODEL_SONNET, 5.0),
        None,
        "must not fire a second time for the same stream"
    );
}

#[test]
fn emit_budget_hard_stop_sends_a_budget_exceeded_event() {
    let bus: lopi_core::EventBus<AgentEvent> = lopi_core::EventBus::new(8);
    let mut rx = bus.subscribe();
    let tid = lopi_core::TaskId::new();
    emit_budget_hard_stop(&bus, tid, 6.0, 5.0);

    // A log line precedes the structured event — drain it first.
    rx.try_recv().expect("log line should have been sent");
    let ev = rx
        .try_recv()
        .expect("structured event should have been sent");
    match ev {
        AgentEvent::BudgetExceeded {
            task_id,
            scope,
            limit_usd,
            burned_usd,
        } => {
            assert_eq!(task_id, Some(tid));
            assert_eq!(scope, lopi_core::BudgetScope::Task);
            assert!((limit_usd - 5.0).abs() < f64::EPSILON);
            assert!((burned_usd - 6.0).abs() < f64::EPSILON);
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
}
