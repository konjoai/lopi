//! Sprint T0, Phase 4 — per-task cognition state the TUI now retains
//! instead of silently dropping. Before this sprint, `AppState::handle_event`
//! no-op'd `TurnMetrics`, `BudgetExceeded`, `BudgetSoftWarn`,
//! `VerifierVerdict`, `PlanProposed`, and the whole `ToolCall`/`ToolResult`/
//! `TokenDelta`/`ApiRetry`/`Cost`/`Phase` cluster, with comments pointing at
//! the web Forge instead.
//!
//! No widget renders any of this yet — T5 (Live Cognition Surface) builds
//! the text-native panel. This module only stops throwing the signal away,
//! so T1-T3's widget work never has to re-touch `AppState::handle_event`'s
//! match statement to add the retention this sprint already did.
//!
//! Every sample struct's fields are populated from real `AgentEvent` data
//! (`tui.rs::AppState::handle_event`, exercised by `tui_tests.rs`) but not
//! yet *read* by any renderer — the same "populated now, consumed by a
//! later sprint" shape as `lopi-agent::api_client_wire`'s SSE wire types,
//! which use the identical `#[allow(dead_code)]` pattern for the same
//! reason.

#![allow(dead_code)]

use std::collections::VecDeque;

/// How many streaming samples (turn metrics, phases, costs, tool calls) to
/// retain per task before evicting the oldest. Mirrors `tui.rs::MAX_LOG_LINES`'s
/// bounded-retention pattern.
const MAX_SAMPLES: usize = 50;

/// One `AgentEvent::TurnMetrics` sample.
#[derive(Debug, Clone, Copy)]
pub(super) struct TurnMetricsSample {
    pub(super) pressure: f32,
    pub(super) activity: f32,
    pub(super) tokens_per_sec: f32,
    pub(super) cost_usd: f32,
}

/// One `AgentEvent::Cost` sample.
#[derive(Debug, Clone)]
pub(super) struct CostSample {
    pub(super) cost_usd: f64,
    pub(super) num_turns: u32,
    pub(super) session_id: String,
}

/// One `AgentEvent::ToolCall`, updated in place by a matching
/// `AgentEvent::ToolResult` when it arrives.
#[derive(Debug, Clone)]
pub(super) struct ToolCallSample {
    pub(super) tool: String,
    pub(super) summary: String,
    pub(super) result: Option<ToolResultSample>,
}

/// The `AgentEvent::ToolResult` half of a [`ToolCallSample`].
#[derive(Debug, Clone)]
pub(super) struct ToolResultSample {
    pub(super) is_error: bool,
    pub(super) preview: String,
}

/// Latest `AgentEvent::TokenDelta` — a live gauge, not a stream, so only the
/// most recent value is kept.
#[derive(Debug, Clone, Copy)]
pub(super) struct TokenDeltaSample {
    pub(super) output_tokens: u32,
    pub(super) input_tokens: u32,
    pub(super) cache_read_tokens: u32,
}

/// Latest `AgentEvent::ApiRetry`.
#[derive(Debug, Clone)]
pub(super) struct ApiRetrySample {
    pub(super) status: String,
    pub(super) limit_type: String,
    pub(super) utilization: f32,
    pub(super) resets_at: Option<i64>,
}

/// Latest `AgentEvent::PlanProposed`.
#[derive(Debug, Clone)]
pub(super) struct PlanSample {
    pub(super) attempt: u8,
    pub(super) steps: Vec<String>,
    pub(super) plan: String,
}

/// Latest `AgentEvent::VerifierVerdict`.
#[derive(Debug, Clone)]
pub(super) struct VerifierVerdictSample {
    pub(super) passed: bool,
    pub(super) gaps: Vec<String>,
    pub(super) fix_hints: Vec<String>,
    pub(super) confidence: f64,
}

/// Latest `AgentEvent::BudgetExceeded` breach for a task.
#[derive(Debug, Clone)]
pub(super) struct BudgetExceededSample {
    pub(super) limit_usd: f64,
    pub(super) burned_usd: f64,
}

/// Latest `AgentEvent::BudgetSoftWarn` for a task.
#[derive(Debug, Clone, Copy)]
pub(super) struct BudgetSoftWarnSample {
    pub(super) estimated_usd: f64,
    pub(super) cap_usd: f64,
}

/// Everything the previously-dropped event variants carry for one task.
#[derive(Debug, Clone, Default)]
pub(super) struct AgentCognition {
    pub(super) turn_metrics: VecDeque<TurnMetricsSample>,
    pub(super) phases: VecDeque<String>,
    pub(super) costs: VecDeque<CostSample>,
    pub(super) tool_calls: VecDeque<ToolCallSample>,
    pub(super) last_token_delta: Option<TokenDeltaSample>,
    pub(super) last_api_retry: Option<ApiRetrySample>,
    pub(super) last_plan: Option<PlanSample>,
    pub(super) last_verifier_verdict: Option<VerifierVerdictSample>,
    pub(super) last_budget_exceeded: Option<BudgetExceededSample>,
    pub(super) last_budget_soft_warn: Option<BudgetSoftWarnSample>,
}

fn push_bounded<T>(queue: &mut VecDeque<T>, item: T) {
    queue.push_back(item);
    if queue.len() > MAX_SAMPLES {
        queue.pop_front();
    }
}

impl AgentCognition {
    pub(super) fn push_turn_metrics(&mut self, sample: TurnMetricsSample) {
        push_bounded(&mut self.turn_metrics, sample);
    }

    pub(super) fn push_phase(&mut self, phase: String) {
        push_bounded(&mut self.phases, phase);
    }

    pub(super) fn push_cost(&mut self, sample: CostSample) {
        push_bounded(&mut self.costs, sample);
    }

    pub(super) fn push_tool_call(&mut self, sample: ToolCallSample) {
        push_bounded(&mut self.tool_calls, sample);
    }

    /// Attach a tool result to the most recent tool call for the same tool
    /// name that doesn't already have one. Falls back to a no-op if a
    /// result arrives with no matching call (e.g. after eviction).
    pub(super) fn apply_tool_result(&mut self, tool: &str, result: ToolResultSample) {
        if let Some(call) = self
            .tool_calls
            .iter_mut()
            .rev()
            .find(|c| c.tool == tool && c.result.is_none())
        {
            call.result = Some(result);
        }
    }
}

#[cfg(test)]
#[path = "cognition_tests.rs"]
mod tests;
