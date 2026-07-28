//! Sprint T0, Phase 4 — the six previously-dropped `AgentEvent` variants now
//! mutate `AppState::cognition` instead of no-op'ing. One test per variant,
//! proving the retained state round-trips every field the event carries.
//! Split out of `tui_tests.rs` to keep that file under the 500-line CI
//! file-size gate.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn turn_metrics_is_retained_per_task() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::TurnMetrics {
        task_id: id,
        pressure: 0.8,
        activity: 0.5,
        tokens_per_sec: 42.0,
        cost_usd: 0.03,
    });
    let cog = state.cognition.get(&id).expect("cognition entry created");
    assert_eq!(cog.turn_metrics.len(), 1);
    let sample = cog.turn_metrics.back().unwrap();
    assert!((sample.pressure - 0.8).abs() < f32::EPSILON);
    assert!((sample.activity - 0.5).abs() < f32::EPSILON);
    assert!((sample.tokens_per_sec - 42.0).abs() < f32::EPSILON);
    assert!((sample.cost_usd - 0.03).abs() < f32::EPSILON);
}

#[test]
fn budget_exceeded_with_a_task_id_is_retained() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::BudgetExceeded {
        task_id: Some(id),
        scope: lopi_core::budget::BudgetScope::Task,
        limit_usd: 5.0,
        burned_usd: 5.5,
    });
    let sample = state.cognition.get(&id).unwrap().last_budget_exceeded.as_ref().unwrap();
    assert!((sample.limit_usd - 5.0).abs() < f64::EPSILON);
    assert!((sample.burned_usd - 5.5).abs() < f64::EPSILON);
}

#[test]
fn budget_exceeded_fleet_wide_has_no_task_to_key_on() {
    let mut state = AppState::new();
    state.handle_event(AgentEvent::BudgetExceeded {
        task_id: None,
        scope: lopi_core::budget::BudgetScope::Fleet,
        limit_usd: 100.0,
        burned_usd: 101.0,
    });
    assert!(state.cognition.is_empty());
}

#[test]
fn budget_soft_warn_is_retained() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::BudgetSoftWarn {
        task_id: id,
        estimated_usd: 0.8,
        cap_usd: 1.0,
    });
    let sample = state.cognition.get(&id).unwrap().last_budget_soft_warn.unwrap();
    assert!((sample.estimated_usd - 0.8).abs() < f64::EPSILON);
    assert!((sample.cap_usd - 1.0).abs() < f64::EPSILON);
}

#[test]
fn verifier_verdict_is_retained() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::VerifierVerdict {
        task_id: id,
        passed: false,
        gaps: vec!["missing test".to_string()],
        fix_hints: vec!["add a test".to_string()],
        confidence: 0.9,
    });
    let verdict = state.cognition.get(&id).unwrap().last_verifier_verdict.as_ref().unwrap();
    assert!(!verdict.passed);
    assert_eq!(verdict.gaps, vec!["missing test".to_string()]);
    assert_eq!(verdict.fix_hints, vec!["add a test".to_string()]);
    assert!((verdict.confidence - 0.9).abs() < f64::EPSILON);
}

#[test]
fn plan_proposed_is_retained() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::PlanProposed {
        task_id: id,
        attempt: 1,
        steps: vec!["step one".to_string()],
        plan: "full plan text".to_string(),
    });
    let plan = state.cognition.get(&id).unwrap().last_plan.as_ref().unwrap();
    assert_eq!(plan.attempt, 1);
    assert_eq!(plan.steps, vec!["step one".to_string()]);
    assert_eq!(plan.plan, "full plan text");
}

#[test]
fn tool_call_and_tool_result_are_retained_and_linked() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::ToolCall {
        task_id: id,
        tool: "Bash".to_string(),
        summary: "ls -la".to_string(),
    });
    state.handle_event(AgentEvent::ToolResult {
        task_id: id,
        tool: "Bash".to_string(),
        is_error: false,
        preview: "total 0".to_string(),
    });
    let cog = state.cognition.get(&id).unwrap();
    assert_eq!(cog.tool_calls.len(), 1);
    let call = &cog.tool_calls[0];
    assert_eq!(call.tool, "Bash");
    assert_eq!(call.summary, "ls -la");
    let result = call.result.as_ref().expect("result attached to the call");
    assert!(!result.is_error);
    assert_eq!(result.preview, "total 0");
}

#[test]
fn token_delta_keeps_only_the_latest_value() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::TokenDelta {
        task_id: id,
        output_tokens: 10,
        input_tokens: 100,
        cache_read_tokens: 50,
    });
    state.handle_event(AgentEvent::TokenDelta {
        task_id: id,
        output_tokens: 20,
        input_tokens: 100,
        cache_read_tokens: 50,
    });
    let latest = state.cognition.get(&id).unwrap().last_token_delta.unwrap();
    assert_eq!(latest.output_tokens, 20, "must overwrite, not accumulate a history");
}

#[test]
fn api_retry_is_retained() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::ApiRetry {
        task_id: id,
        status: "allowed_warning".to_string(),
        limit_type: "five_hour".to_string(),
        utilization: 0.9,
        resets_at: Some(12345),
    });
    let retry = state.cognition.get(&id).unwrap().last_api_retry.as_ref().unwrap();
    assert_eq!(retry.status, "allowed_warning");
    assert_eq!(retry.limit_type, "five_hour");
    assert!((retry.utilization - 0.9).abs() < f32::EPSILON);
    assert_eq!(retry.resets_at, Some(12345));
}

#[test]
fn cost_events_accumulate_as_a_bounded_stream() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::Cost {
        task_id: id,
        cost_usd: 0.10,
        num_turns: 1,
        session_id: "sess-1".to_string(),
    });
    state.handle_event(AgentEvent::Cost {
        task_id: id,
        cost_usd: 0.25,
        num_turns: 2,
        session_id: "sess-1".to_string(),
    });
    let cog = state.cognition.get(&id).unwrap();
    assert_eq!(cog.costs.len(), 2);
    assert!((cog.costs.back().unwrap().cost_usd - 0.25).abs() < f64::EPSILON);
}

#[test]
fn phase_events_accumulate_as_a_bounded_stream() {
    let mut state = AppState::new();
    let id = TaskId::new();
    state.handle_event(AgentEvent::Phase {
        task_id: id,
        phase: "requesting".to_string(),
    });
    state.handle_event(AgentEvent::Phase {
        task_id: id,
        phase: "review_ready".to_string(),
    });
    let cog = state.cognition.get(&id).unwrap();
    assert_eq!(cog.phases.len(), 2);
    assert_eq!(cog.phases.back().unwrap(), "review_ready");
}

#[test]
fn report_ready_remains_a_no_op_but_does_not_panic() {
    let mut state = AppState::new();
    state.handle_event(AgentEvent::ReportReady {
        task_id: TaskId::new(),
        channel: "telegram".to_string(),
        summary: "done".to_string(),
    });
    assert!(state.cognition.is_empty());
}
