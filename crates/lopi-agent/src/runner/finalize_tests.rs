//! Finalize tests — split out of `finalize.rs` to keep that file under the
//! 500-line CI file-size gate. Covers autonomy-level PR decisions, the
//! zero-diff intent-aware success path, Report on Finish, and Sprint
//! Successor-1's `derive_and_stash_successor` wiring.
#![allow(clippy::expect_used)]

use super::{
    build_report_summary, pr_decision, requires_verifier, should_auto_merge, zero_diff_is_success,
    AgentRunner, PrDecision,
};
use lopi_core::loop_config::AutonomyLevel;
use lopi_core::{AgentEvent, Deliverable, Score, Task};
use std::path::PathBuf;

#[test]
fn each_level_maps_to_its_decision() {
    assert_eq!(
        pr_decision(AutonomyLevel::ReportOnly),
        PrDecision::ReportOnly
    );
    assert_eq!(pr_decision(AutonomyLevel::DraftPr), PrDecision::Draft);
    assert_eq!(pr_decision(AutonomyLevel::VerifiedPr), PrDecision::Normal);
    assert_eq!(pr_decision(AutonomyLevel::AutoMerge), PrDecision::AutoMerge);
}

#[test]
fn only_l1_skips_the_pr() {
    let no_pr: Vec<_> = AutonomyLevel::all()
        .into_iter()
        .filter(|l| pr_decision(*l) == PrDecision::ReportOnly)
        .collect();
    assert_eq!(no_pr, vec![AutonomyLevel::ReportOnly]);
}

#[test]
fn only_l4_auto_merges() {
    let merges: Vec<_> = AutonomyLevel::all()
        .into_iter()
        .filter(|l| pr_decision(*l) == PrDecision::AutoMerge)
        .collect();
    assert_eq!(merges, vec![AutonomyLevel::AutoMerge]);
}

#[test]
fn only_l2_opens_a_draft() {
    let drafts: Vec<_> = AutonomyLevel::all()
        .into_iter()
        .filter(|l| pr_decision(*l) == PrDecision::Draft)
        .collect();
    assert_eq!(drafts, vec![AutonomyLevel::DraftPr]);
}

#[test]
fn l3_and_l4_force_the_verifier_even_when_disabled() {
    assert!(requires_verifier(false, AutonomyLevel::VerifiedPr));
    assert!(requires_verifier(false, AutonomyLevel::AutoMerge));
}

#[test]
fn l1_and_l2_only_verify_when_explicitly_enabled() {
    assert!(!requires_verifier(false, AutonomyLevel::ReportOnly));
    assert!(!requires_verifier(false, AutonomyLevel::DraftPr));
    assert!(requires_verifier(true, AutonomyLevel::ReportOnly));
    assert!(requires_verifier(true, AutonomyLevel::DraftPr));
}

#[test]
fn auto_merge_only_when_l4_and_pr_opened() {
    // L4 + PR opened → merge.
    assert!(should_auto_merge(PrDecision::AutoMerge, true));
    // L4 but the PR failed to open → never merge a branch with no PR.
    assert!(!should_auto_merge(PrDecision::AutoMerge, false));
    // Lower levels never auto-merge, even with a PR open.
    for d in [
        PrDecision::ReportOnly,
        PrDecision::Draft,
        PrDecision::Normal,
    ] {
        assert!(!should_auto_merge(d, true));
    }
}

// ── Intent-aware zero-diff success ──────────────────────────────────────

#[test]
fn review_only_zero_diff_is_a_success() {
    // A review/analysis goal legitimately produces no file changes.
    assert!(zero_diff_is_success(Deliverable::ReviewOnly, false));
}

#[test]
fn file_changes_zero_diff_is_not_a_success() {
    // A goal that must edit files but produced nothing is a failure to
    // retry — the phantom-`goal_met` regression this guards against.
    assert!(!zero_diff_is_success(Deliverable::FileChanges, false));
}

#[test]
fn until_fired_concludes_even_a_file_changes_goal() {
    // The loop's `until` exit condition ends the loop early regardless of
    // this attempt's own (empty) output.
    assert!(zero_diff_is_success(Deliverable::FileChanges, true));
    assert!(zero_diff_is_success(Deliverable::ReviewOnly, true));
}

// ── Report on Finish (Sprint 3) ─────────────────────────────────────────

fn drain_report_ready(
    rx: &mut tokio::sync::broadcast::Receiver<AgentEvent>,
) -> Option<(String, String)> {
    let mut found = None;
    while let Ok(ev) = rx.try_recv() {
        if let AgentEvent::ReportReady {
            channel, summary, ..
        } = ev
        {
            found = Some((channel, summary));
        }
    }
    found
}

#[test]
fn emit_report_routes_to_the_declared_channel() {
    let mut task = Task::new("ship the report");
    task.report = Some("telegram".to_string());
    let (runner, bus) = AgentRunner::standalone(task, PathBuf::from("."));
    let mut rx = bus.subscribe();
    let score = Score::new(1.0, 0, 10);

    runner.emit_report("lopi/feature/x", &score, 2);

    let (channel, summary) =
        drain_report_ready(&mut rx).expect("a ReportReady event should have been sent");
    assert_eq!(channel, "telegram");
    assert!(summary.contains("ship the report"));
    assert!(summary.contains("pass"));
}

#[test]
fn emit_report_with_no_channel_sends_nothing() {
    let task = Task::new("quiet run"); // report defaults to None
    let (runner, bus) = AgentRunner::standalone(task, PathBuf::from("."));
    let mut rx = bus.subscribe();
    let score = Score::new(1.0, 0, 10);

    runner.emit_report("lopi/feature/x", &score, 1);

    assert!(
        drain_report_ready(&mut rx).is_none(),
        "no channel declared → no report broadcast"
    );
}

#[test]
fn emit_report_warns_and_sends_nothing_for_an_unrecognized_channel() {
    let mut task = Task::new("misconfigured run");
    task.report = Some("carrier-pigeon".to_string());
    let (runner, bus) = AgentRunner::standalone(task, PathBuf::from("."));
    let mut rx = bus.subscribe();
    let score = Score::new(1.0, 0, 10);

    runner.emit_report("lopi/feature/x", &score, 1);

    assert!(
        drain_report_ready(&mut rx).is_none(),
        "an unparseable channel must warn, not silently send"
    );
}

#[test]
fn build_report_summary_contains_goal_and_pass_verdict() {
    let score = Score::new(0.9, 1, 42);
    let summary = build_report_summary("fix the bug", "lopi/feature/y", &score, 3);
    assert!(summary.contains("fix the bug"));
    assert!(summary.contains("pass"));
    assert!(summary.contains("lopi/feature/y"));
}

// ── Sprint Successor-1: derive_and_stash_successor ──────────────────

fn fixture_successor(goal: &str) -> lopi_core::Successor {
    lopi_core::Successor {
        goal: goal.to_string(),
        when: lopi_core::SuccessorCondition::OnSuccess,
        rationale: "follow-up identified during the run".to_string(),
        allowed_dirs: vec![],
    }
}

#[test]
fn derive_and_stash_successor_does_nothing_when_not_enabled() {
    let task = Task::new("plain task"); // successor_enabled defaults false
    let (mut runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));
    runner.derive_and_stash_successor();
    assert!(runner.take_pending_successor().is_none());
}

#[test]
fn derive_and_stash_successor_does_nothing_without_a_fixture() {
    let mut task = Task::new("enabled but no fixture");
    task.successor_enabled = true;
    let (mut runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));
    runner.derive_and_stash_successor();
    assert!(runner.take_pending_successor().is_none());
}

#[test]
fn derive_and_stash_successor_stashes_a_gated_child_when_enabled_with_a_fixture() {
    let mut task = Task::new("parent task");
    task.successor_enabled = true;
    task.autonomy_level = AutonomyLevel::DraftPr;
    task.forbidden_dirs = vec!["secrets/".to_string()];
    task.successor_fixture = Some(fixture_successor("follow-up work"));
    let parent_id = task.id;
    let (mut runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));

    runner.derive_and_stash_successor();
    let child = runner
        .take_pending_successor()
        .expect("a valid fixture at a safe depth must derive a successor");
    assert_eq!(child.parent_task, Some(parent_id));
    assert_eq!(child.chain_depth, 1);
    assert!(child.forbidden_dirs.iter().any(|d| d == "secrets/"));
    assert!(child.autonomy_level.rank() <= AutonomyLevel::DraftPr.rank());

    // Taken once — the pool must not enqueue the same successor twice.
    assert!(runner.take_pending_successor().is_none());
}

#[test]
fn derive_and_stash_successor_stashes_nothing_when_the_depth_cap_is_exceeded() {
    let mut task = Task::new("already deep in its chain");
    task.successor_enabled = true;
    task.chain_depth = lopi_core::DEFAULT_MAX_CHAIN_DEPTH; // next hop would exceed the cap
    task.successor_fixture = Some(fixture_successor("go one hop too far"));
    let (mut runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));

    runner.derive_and_stash_successor();
    assert!(
        runner.take_pending_successor().is_none(),
        "depth cap must reject, not silently derive"
    );
}
