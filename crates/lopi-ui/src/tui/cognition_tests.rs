use super::*;

#[test]
fn bounded_push_evicts_oldest_past_max_samples() {
    let mut cog = AgentCognition::default();
    for i in 0..(MAX_SAMPLES + 10) {
        cog.push_phase(format!("phase-{i}"));
    }
    assert_eq!(cog.phases.len(), MAX_SAMPLES);
    // The oldest 10 were evicted; the front is now phase-10.
    assert_eq!(cog.phases.front(), Some(&"phase-10".to_string()));
    assert_eq!(cog.phases.back(), Some(&format!("phase-{}", MAX_SAMPLES + 9)));
}

#[test]
fn tool_result_attaches_to_the_most_recent_unmatched_call() {
    let mut cog = AgentCognition::default();
    cog.push_tool_call(ToolCallSample {
        tool: "Bash".to_string(),
        summary: "ls".to_string(),
        result: None,
    });
    cog.push_tool_call(ToolCallSample {
        tool: "Read".to_string(),
        summary: "file.rs".to_string(),
        result: None,
    });

    cog.apply_tool_result(
        "Bash",
        ToolResultSample {
            is_error: false,
            preview: "ok".to_string(),
        },
    );

    assert!(cog.tool_calls[0].result.is_some());
    assert!(cog.tool_calls[1].result.is_none());
}

#[test]
fn tool_result_with_no_matching_call_is_a_no_op() {
    let mut cog = AgentCognition::default();
    cog.apply_tool_result(
        "Bash",
        ToolResultSample {
            is_error: true,
            preview: "boom".to_string(),
        },
    );
    assert!(cog.tool_calls.is_empty());
}

#[test]
fn default_cognition_has_no_latest_samples() {
    let cog = AgentCognition::default();
    assert!(cog.last_token_delta.is_none());
    assert!(cog.last_api_retry.is_none());
    assert!(cog.last_plan.is_none());
    assert!(cog.last_verifier_verdict.is_none());
    assert!(cog.last_budget_exceeded.is_none());
    assert!(cog.last_budget_soft_warn.is_none());
}
