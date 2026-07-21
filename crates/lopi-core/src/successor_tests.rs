//! Successor tests — split out of `successor.rs` to keep that file under the
//! 500-line CI file-size gate. Covers `Successor`/`SuccessorCondition`
//! validation, all four `derive_successor_task` containment gates, and the
//! KT-A inversion (see `LEDGER.md`'s Sprint Successor-1 entry).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn sample(goal: impl Into<String>) -> Successor {
    Successor {
        goal: goal.into(),
        when: SuccessorCondition::OnSuccess,
        rationale: "follow-up work identified during the run".to_string(),
        allowed_dirs: vec!["src/".to_string()],
    }
}

#[test]
fn every_condition_arm_parses_from_its_canonical_tag() {
    assert_eq!(
        SuccessorCondition::parse("on_success"),
        Some(SuccessorCondition::OnSuccess)
    );
    assert_eq!(
        SuccessorCondition::parse("on_failure"),
        Some(SuccessorCondition::OnFailure)
    );
    assert_eq!(
        SuccessorCondition::parse("always"),
        Some(SuccessorCondition::Always)
    );
}

#[test]
fn parse_accepts_aliases_case_insensitively() {
    assert_eq!(
        SuccessorCondition::parse("Success"),
        Some(SuccessorCondition::OnSuccess)
    );
    assert_eq!(
        SuccessorCondition::parse("  OnFailure "),
        Some(SuccessorCondition::OnFailure)
    );
    assert_eq!(
        SuccessorCondition::parse("FAIL"),
        Some(SuccessorCondition::OnFailure)
    );
    assert_eq!(
        SuccessorCondition::parse("ALWAYS"),
        Some(SuccessorCondition::Always)
    );
    assert_eq!(SuccessorCondition::parse("nonsense"), None);
}

#[test]
fn condition_serde_is_snake_case() {
    let json = serde_json::to_string(&SuccessorCondition::OnFailure).unwrap();
    assert_eq!(json, "\"on_failure\"");
    let back: SuccessorCondition = serde_json::from_str("\"always\"").unwrap();
    assert_eq!(back, SuccessorCondition::Always);
}

#[test]
fn successor_round_trips_through_serde() {
    let s = sample("write the migration guide");
    let json = serde_json::to_string(&s).unwrap();
    let back: Successor = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn allowed_dirs_defaults_to_empty_when_absent_from_json() {
    let json = r#"{"goal":"g","when":"always","rationale":"r"}"#;
    let s: Successor = serde_json::from_str(json).unwrap();
    assert!(s.allowed_dirs.is_empty());
}

#[test]
fn validate_accepts_a_normal_goal() {
    assert!(sample("do the thing").validate().is_ok());
}

#[test]
fn validate_rejects_an_empty_goal() {
    let err = sample("").validate().unwrap_err();
    assert_eq!(err, SuccessorError::EmptyGoal);
    assert!(err.to_string().contains("empty"));
}

#[test]
fn validate_rejects_a_whitespace_only_goal() {
    assert_eq!(
        sample("   \n\t").validate().unwrap_err(),
        SuccessorError::EmptyGoal
    );
}

#[test]
fn validate_rejects_an_over_length_goal() {
    let goal = "x".repeat(MAX_GOAL_LEN + 1);
    let err = sample(goal).validate().unwrap_err();
    match err {
        SuccessorError::GoalTooLong { len, max } => {
            assert_eq!(len, MAX_GOAL_LEN + 1);
            assert_eq!(max, MAX_GOAL_LEN);
        }
        SuccessorError::EmptyGoal => panic!("wrong variant"),
    }
    assert!(err.to_string().contains(&MAX_GOAL_LEN.to_string()));
}

#[test]
fn validate_accepts_a_goal_at_exactly_the_limit() {
    let goal = "x".repeat(MAX_GOAL_LEN);
    assert!(sample(goal).validate().is_ok());
}

// ── Gate 1: depth cap ────────────────────────────────────────────────

fn parent_task() -> Task {
    Task::new("do the original work")
}

#[test]
fn depth_cap_rejects_once_the_next_depth_exceeds_max() {
    let mut parent = parent_task();
    parent.chain_depth = 3;
    let err = derive_successor_task(&parent, &sample("go further"), 3).unwrap_err();
    assert_eq!(
        err,
        SuccessorRejection::DepthExceeded {
            next_depth: 4,
            max_depth: 3
        }
    );
}

#[test]
fn depth_cap_permits_the_exact_boundary_depth() {
    let mut parent = parent_task();
    parent.chain_depth = 2;
    let child = derive_successor_task(&parent, &sample("go further"), 3).unwrap();
    assert_eq!(child.chain_depth, 3);
}

#[test]
fn invalid_successor_goal_rejects_before_any_gate_runs() {
    let parent = parent_task();
    let err = derive_successor_task(&parent, &sample(""), 5).unwrap_err();
    assert_eq!(
        err,
        SuccessorRejection::InvalidGoal(SuccessorError::EmptyGoal)
    );
}

// ── Gate 2: autonomy ceiling ─────────────────────────────────────────

#[test]
fn clamp_autonomy_never_exceeds_the_parent_rank() {
    assert_eq!(
        clamp_autonomy_to_parent(AutonomyLevel::DraftPr, AutonomyLevel::AutoMerge),
        AutonomyLevel::DraftPr,
        "an L2 parent cannot produce an L4 child"
    );
    assert_eq!(
        clamp_autonomy_to_parent(AutonomyLevel::AutoMerge, AutonomyLevel::ReportOnly),
        AutonomyLevel::ReportOnly,
        "a narrower request is honored, not overridden upward"
    );
    assert_eq!(
        clamp_autonomy_to_parent(AutonomyLevel::VerifiedPr, AutonomyLevel::VerifiedPr),
        AutonomyLevel::VerifiedPr
    );
}

#[test]
fn derived_child_never_exceeds_the_parents_autonomy_rank() {
    let mut parent = parent_task();
    parent.autonomy_level = AutonomyLevel::ReportOnly; // L1, the strictest level
    let child = derive_successor_task(&parent, &sample("go further"), 5).unwrap();
    assert!(child.autonomy_level.rank() <= parent.autonomy_level.rank());
    assert_eq!(child.autonomy_level, AutonomyLevel::ReportOnly);
}

// ── Gate 3: directory inheritance ───────────────────────────────────

#[test]
fn successor_cannot_reach_a_directory_its_parent_was_forbidden() {
    let mut parent = parent_task();
    parent.forbidden_dirs = vec!["secrets/".to_string()];
    let child = derive_successor_task(&parent, &sample("go further"), 5).unwrap();
    assert!(child.forbidden_dirs.iter().any(|d| d == "secrets/"));
}

#[test]
fn forbidden_dirs_is_the_union_of_parent_and_the_childs_own_defaults() {
    let mut parent = parent_task();
    parent.forbidden_dirs = vec!["secrets/".to_string()];
    let child = derive_successor_task(&parent, &sample("go further"), 5).unwrap();
    // The child's own baseline defaults (`.github/`, `infra/`, `Cargo.toml`)
    // survive alongside the parent's `secrets/` — a union, not a replace.
    assert!(child.forbidden_dirs.iter().any(|d| d == "secrets/"));
    assert!(child.forbidden_dirs.iter().any(|d| d == "infra/"));
}

#[test]
fn allowed_dirs_intersects_with_a_nonempty_parent_allowlist() {
    let mut parent = parent_task();
    parent.allowed_dirs = vec!["src/".to_string(), "docs/".to_string()];
    let mut successor = sample("go further");
    successor.allowed_dirs = vec!["docs/".to_string(), "infra/".to_string()];
    let child = derive_successor_task(&parent, &successor, 5).unwrap();
    assert_eq!(child.allowed_dirs, vec!["docs/".to_string()]);
}

#[test]
fn allowed_dirs_falls_back_to_the_request_when_parent_is_unrestricted() {
    let mut parent = parent_task();
    parent.allowed_dirs = vec![]; // empty == "no restriction stated"
    let mut successor = sample("go further");
    successor.allowed_dirs = vec!["docs/".to_string()];
    let child = derive_successor_task(&parent, &successor, 5).unwrap();
    assert_eq!(child.allowed_dirs, vec!["docs/".to_string()]);
}

// ── Gate 4: untrusted-source lockdown ───────────────────────────────

#[test]
fn webhook_sourced_parent_forces_plan_approval_and_disables_the_chain() {
    let mut parent = parent_task();
    parent.source = TaskSource::Webhook {
        repo: "org/repo".into(),
        event: "check_run".into(),
    };
    parent.autonomy_level = AutonomyLevel::AutoMerge; // even at max trust
    let mut successor = sample("go further");
    successor.allowed_dirs = vec![];
    let child = derive_successor_task(&parent, &successor, 5).unwrap();
    assert!(child.require_plan_approval);
    assert!(!child.successor_enabled);
}

#[test]
fn telegram_sourced_parent_is_also_untrusted() {
    assert!(is_untrusted_source(&TaskSource::Telegram {
        chat_id: 1,
        message_id: 2
    }));
}

#[test]
fn cli_and_api_sourced_parents_are_trusted() {
    assert!(!is_untrusted_source(&TaskSource::Cli));
    assert!(!is_untrusted_source(&TaskSource::Api));
}

#[test]
fn trusted_source_parent_does_not_force_plan_approval() {
    let parent = parent_task(); // TaskSource::Cli by default
    let child = derive_successor_task(&parent, &sample("go further"), 5).unwrap();
    assert!(!child.require_plan_approval);
}

// ── Lineage bookkeeping always applied ──────────────────────────────

#[test]
fn derived_child_always_carries_parent_link_depth_and_self_authored_source() {
    let mut parent = parent_task();
    parent.chain_depth = 1;
    let child = derive_successor_task(&parent, &sample("go further"), 5).unwrap();
    assert_eq!(child.parent_task, Some(parent.id));
    assert_eq!(child.chain_depth, 2);
    match child.source {
        TaskSource::SelfAuthored { parent: p } => assert_eq!(p, parent.id),
        other => panic!("expected SelfAuthored, got {other:?}"),
    }
}

// ── KT-A inversion (Phase 2) ─────────────────────────────────────────
//
// Mirrors the exact escalation scenario `lopi_orchestrator::task_build::
// tests::kt_a_containment_is_currently_absent` demonstrated was
// unprevented before this sprint — this time run through the real
// containment gate, asserting it is now blocked.
#[test]
fn kt_a_inverted_derive_successor_task_blocks_the_escalation() {
    let parent = Task {
        source: TaskSource::Webhook {
            repo: "org/repo".into(),
            event: "check_run".into(),
        },
        autonomy_level: AutonomyLevel::DraftPr, // L2
        forbidden_dirs: vec!["infra/".to_string(), "secrets/".to_string()],
        successor_enabled: true,
        ..Task::new("fix the failing check")
    };

    let mut escalation_attempt = sample("escalate and merge everything");
    escalation_attempt.allowed_dirs = vec![]; // no proposed restriction either

    let child = derive_successor_task(&parent, &escalation_attempt, 5)
        .expect("a valid, in-bounds-depth successor is still derived");

    // The autonomy ceiling holds: no widening past the parent's L2.
    assert!(
        child.autonomy_level.rank() <= parent.autonomy_level.rank(),
        "blocked: successor cannot widen past the parent's autonomy level"
    );
    // The parent's forbidden dir survives into the child.
    assert!(
        child.forbidden_dirs.iter().any(|d| d == "secrets/"),
        "blocked: successor retains the parent's `secrets/` restriction"
    );
    // The untrusted (Webhook) source forces plan approval and kills the chain.
    assert!(child.require_plan_approval);
    assert!(!child.successor_enabled);
    // And the escalation attempt is now durably linked to its parent —
    // no longer the free-floating, unaccountable task KT-A showed.
    assert_eq!(child.parent_task, Some(parent.id));
}
