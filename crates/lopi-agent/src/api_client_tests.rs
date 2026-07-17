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
