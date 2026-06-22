//! Phase 16.6 — per-run token-budget enforcement.
//!
//! Wires [`LoopConfig::budget_tokens`](lopi_core::LoopConfig) to the Anthropic
//! **task budget** beta (`output_config.task_budget`). A task budget tells the
//! model how many tokens it has for the whole agentic loop so it *paces itself*
//! and finishes gracefully, instead of being hard-cut mid-thought by
//! `max_tokens`. This is the safety lever the loop-engineering design calls out:
//! the model self-regulates rather than the harness truncating it.
//!
//! All decision logic lives here as pure functions so it is unit-tested and
//! mutation-covered; [`api_client`](crate::api_client) only forwards the result
//! onto the wire.

/// Anthropic beta header that enables the `task_budget` output config.
pub const TASK_BUDGETS_BETA: &str = "task-budgets-2026-03-13";

/// The API's minimum accepted `task_budget.total`. Requests below this are
/// rejected with a 400, so a smaller configured budget is clamped up to it.
pub const TASK_BUDGET_MIN: u64 = 20_000;

/// Whether `model` accepts the `task_budget` parameter.
///
/// Task budgets are a beta feature limited to the most capable models
/// (Opus 4.7 / 4.8 and Fable 5). Sending the parameter to any other model —
/// e.g. the Haiku / Sonnet tiers lopi uses for cheap early attempts — is
/// rejected with a 400, so the budget is silently dropped there.
#[must_use]
pub fn supports_task_budget(model: &str) -> bool {
    model.contains("opus-4-7") || model.contains("opus-4-8") || model.contains("fable")
}

/// Resolve the effective `task_budget.total` to send for a `(model, requested)`
/// pair, or `None` when no budget should be attached.
///
/// Returns `None` when the caller requested no budget (`None`) or the model
/// does not support task budgets. Otherwise the requested value is clamped up
/// to [`TASK_BUDGET_MIN`] so an under-minimum config never produces a 400.
#[must_use]
pub fn effective_task_budget(model: &str, requested: Option<u64>) -> Option<u64> {
    let total = requested?;
    if !supports_task_budget(model) {
        return None;
    }
    Some(total.max(TASK_BUDGET_MIN))
}

/// The `output_config` request value carrying a token task budget of `total`,
/// in wire shape: `{"task_budget": {"type": "tokens", "total": N}}`.
#[must_use]
pub fn task_budget_output_config(total: u64) -> serde_json::Value {
    serde_json::json!({ "task_budget": { "type": "tokens", "total": total } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_47_and_48_and_fable_support_budgets() {
        assert!(supports_task_budget("claude-opus-4-7"));
        assert!(supports_task_budget("claude-opus-4-8"));
        assert!(supports_task_budget("claude-fable-5"));
    }

    #[test]
    fn sonnet_and_haiku_do_not_support_budgets() {
        assert!(!supports_task_budget("claude-sonnet-4-6"));
        assert!(!supports_task_budget("claude-haiku-4-5-20251001"));
        assert!(!supports_task_budget("claude-opus-4-6"));
    }

    #[test]
    fn effective_budget_is_none_without_a_request() {
        assert_eq!(effective_task_budget("claude-opus-4-7", None), None);
    }

    #[test]
    fn effective_budget_is_none_on_unsupported_model() {
        // A configured budget is dropped — not clamped — for an unsupported model.
        assert_eq!(effective_task_budget("claude-haiku-4-5", Some(50_000)), None);
        assert_eq!(effective_task_budget("claude-sonnet-4-6", Some(50_000)), None);
    }

    #[test]
    fn effective_budget_clamps_below_minimum_up() {
        assert_eq!(
            effective_task_budget("claude-opus-4-7", Some(5_000)),
            Some(TASK_BUDGET_MIN),
        );
        // Exactly the minimum passes through unchanged.
        assert_eq!(
            effective_task_budget("claude-opus-4-7", Some(TASK_BUDGET_MIN)),
            Some(TASK_BUDGET_MIN),
        );
    }

    #[test]
    fn effective_budget_passes_through_above_minimum() {
        assert_eq!(
            effective_task_budget("claude-opus-4-8", Some(128_000)),
            Some(128_000),
        );
    }

    #[test]
    fn output_config_has_wire_shape() {
        let v = task_budget_output_config(64_000);
        assert_eq!(v["task_budget"]["type"], "tokens");
        assert_eq!(v["task_budget"]["total"], 64_000);
    }
}
