//! Seed tests — split out of `seed.rs` to keep that file under the 500-line
//! CI file-size gate. Covers the pure constraint-formatting helpers, the
//! Constraint-Capture-2 promotion gate (`is_promotable`), and two
//! integration-level checks that drive a real `MemoryStore` through
//! `gather_seed`.
#![allow(clippy::unwrap_used)]

use super::{
    consensus_plan_constraint, is_promotable, non_empty_constraint, reflection_constraint,
    skill_constraint_blocks, AgentRunner, MIN_PATTERN_OCCURRENCES, MIN_PATTERN_SUCCESS_RATE,
    REFLECTION_INJECTION_CAP,
};
use lopi_core::{AgentEvent, Attempt, EventBus, Score, Task};
use lopi_memory::{MemoryStore, PatternRow};
use lopi_skill::Skill;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

/// A promotable baseline row (postmortem-exempt fields at their
/// gate-clearing values) that individual tests tweak one field at a
/// time — keeps each test's intent to a single changed field instead of
/// repeating the whole literal.
fn promotable_row() -> PatternRow {
    PatternRow {
        id: "p1".into(),
        goal_keywords: "auth middleware".into(),
        successful_constraints: Some("always mock the token clock".into()),
        avg_attempts: Some(1.0),
        success_rate: Some(0.8),
        last_seen: "2026-01-01T00:00:00Z".into(),
        derived_from_postmortem: 0,
        user_annotation: None,
        occurrence_count: MIN_PATTERN_OCCURRENCES,
    }
}

async fn save_high_score_attempt(store: &MemoryStore, task: &Task) {
    store
        .save_attempt(&Attempt {
            id: uuid::Uuid::new_v4(),
            task_id: task.id,
            attempt_num: 1,
            branch: "test/branch".into(),
            score: Some(Score::new(1.0, 0, 10)),
            outcome: "success".into(),
            created_at: chrono::Utc::now(),
        })
        .await
        .unwrap();
}

#[test]
fn reflection_constraint_labels_the_prior_failure() {
    let c = reflection_constraint("the mutex was held across an await");
    assert!(c.starts_with("Past learning"));
    assert!(c.contains("the mutex was held across an await"));
}

#[test]
fn injection_cap_is_small_and_bounded() {
    assert!(
        (1..=5).contains(&REFLECTION_INJECTION_CAP),
        "the cap must stay small — bounded injection is the §2 discipline"
    );
}

#[test]
fn keeps_non_empty_drops_empty_and_none() {
    assert_eq!(non_empty_constraint(Some("x")), Some("x".to_string()));
    assert_eq!(non_empty_constraint(Some("")), None);
    assert_eq!(non_empty_constraint(None), None);
}

#[test]
fn skill_blocks_label_name_version_description_and_body() {
    let skill = Skill {
        name: "refactor".into(),
        description: "safe refactors".into(),
        user_invocable: false,
        version: "1.0.0".into(),
        triggers: vec!["refactor".into()],
        body: "Always run tests after.".into(),
        source: PathBuf::from("x/SKILL.md"),
    };
    let blocks = skill_constraint_blocks(&[&skill]);
    assert_eq!(blocks.len(), 1);
    assert_eq!(
        blocks[0],
        "Skill «refactor» (v1.0.0) — safe refactors\nAlways run tests after."
    );
}

#[test]
fn skill_blocks_empty_for_no_matches() {
    assert!(skill_constraint_blocks(&[]).is_empty());
}

#[test]
fn consensus_plan_constraint_labels_it_as_a_starting_point_and_carries_the_text() {
    let c = consensus_plan_constraint("1. Read the file\n2. Fix the bug");
    assert!(c.contains("starting point"));
    assert!(c.contains("1. Read the file\n2. Fix the bug"));
}

#[test]
fn is_promotable_true_when_both_thresholds_are_met() {
    assert!(is_promotable(&promotable_row()));
}

#[test]
fn is_promotable_false_below_occurrence_threshold() {
    let mut row = promotable_row();
    row.occurrence_count = MIN_PATTERN_OCCURRENCES - 1;
    assert!(!is_promotable(&row));
}

#[test]
fn is_promotable_false_below_success_rate_threshold() {
    let mut row = promotable_row();
    row.success_rate = Some(MIN_PATTERN_SUCCESS_RATE - 0.01);
    assert!(!is_promotable(&row));
}

#[test]
fn is_promotable_false_when_success_rate_is_absent() {
    let mut row = promotable_row();
    row.success_rate = None;
    assert!(!is_promotable(&row));
}

#[test]
fn is_promotable_true_for_postmortem_pattern_regardless_of_stats() {
    // A fresh postmortem pattern starts at success_rate = 0.0 and
    // occurrence_count = 1 by construction (insert_postmortem_pattern) —
    // both would fail the mined-pattern gate. Postmortem patterns are
    // exempt: one curated failure lesson has always been enough.
    let mut row = promotable_row();
    row.derived_from_postmortem = 1;
    row.occurrence_count = 1;
    row.success_rate = Some(0.0);
    assert!(is_promotable(&row));
}

/// Exit-gate live check (in miniature): a pattern mined twice with a
/// consistently high score clears the promotion gate and its constraint
/// reaches `gather_seed`'s output; a pattern mined only once — the exact
/// "every one-off task becomes an equally-weighted template" case the
/// gate exists to stop — does not, even though both are similar enough
/// to the query goal to be candidates in the first place.
#[tokio::test]
async fn gather_seed_only_injects_promotable_pattern_constraints() {
    let store = MemoryStore::open_in_memory().await.unwrap();

    // Promotable: mined twice under the same goal fingerprint, each
    // time from a clean high-score attempt.
    for _ in 0..2 {
        let t = Task::new("update authentication middleware routing");
        store.save_task(&t, "queued").await.unwrap();
        save_high_score_attempt(&store, &t).await;
        store
            .mine_patterns(
                &t.id,
                &t.goal,
                Some("Always mock the token clock in auth middleware tests"),
            )
            .await
            .unwrap();
    }

    // One-off: mined exactly once, occurrence_count stays at 1.
    let one_off = Task::new("update authentication middleware configuration");
    store.save_task(&one_off, "queued").await.unwrap();
    save_high_score_attempt(&store, &one_off).await;
    store
        .mine_patterns(
            &one_off.id,
            &one_off.goal,
            Some("Reload config from disk on every request"),
        )
        .await
        .unwrap();

    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let (_tx, rx) = tokio::sync::oneshot::channel();
    let mut runner = AgentRunner::new(
        Task::new("update authentication middleware handling"),
        PathBuf::from("/repo"),
        bus,
        Some(store),
        rx,
        Arc::new(AtomicUsize::new(0)),
    );

    let seed = runner.gather_seed().await;
    assert!(
        seed.extra_constraints
            .iter()
            .any(|c| c.contains("Always mock the token clock")),
        "the twice-mined, high-success pattern's constraint should be injected: {:?}",
        seed.extra_constraints
    );
    assert!(
        seed.extra_constraints
            .iter()
            .all(|c| !c.contains("Reload config from disk")),
        "the once-mined pattern's constraint must not be injected: {:?}",
        seed.extra_constraints
    );
}

/// Session Prompt 2's own exit gate: "a passing unit test alone does not
/// close this sprint — the whole point is that `seed.rs` was silently
/// getting nothing before this." This test goes one step further than
/// `gather_seed_only_injects_promotable_pattern_constraints` above: it
/// backfills a **file-backed** SQLite store (not `:memory:`) — the same
/// on-disk shape a real repo's `.lopi/lopi.db` has — by running a
/// *simulated* prior task through the real production sequence
/// (`AgentRunner::success_constraint` deriving a constraint from a real
/// `last_plan`, then the real `mine_patterns` write, exactly as
/// `pool::run_loop::run_one` does on a clean success), then drives a
/// **second**, fresh task through the real `gather_seed()` →
/// `claude_support::build_plan_prompt()` pipeline and asserts the
/// literal text that would be handed to `claude -p` for planning
/// contains the prior task's constraint. This sandbox has no live
/// Anthropic API session to run `claude -p` itself against (the same
/// standing constraint every prior sprint's LEDGER entry records), so
/// this is as far into the real pipeline as a session without one can
/// verify — everything after this point is the CLI subprocess handing
/// this exact string to Claude unmodified.
#[tokio::test]
async fn live_check_backfilled_pattern_constraint_reaches_the_real_planning_prompt() {
    let db_path = std::env::temp_dir().join(format!(
        "lopi-constraint-capture-2-live-check-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store = MemoryStore::open(&db_path).await.unwrap();

    // Simulate a completed prior task in a Rust toolchain repo, run
    // through the exact production sequence: a runner with a real
    // `last_plan`, `success_constraint()` deriving the constraint from
    // it, then `mine_patterns` persisting it on a clean success — twice,
    // so the pattern clears the promotion gate's occurrence threshold.
    for _ in 0..2 {
        let prior_task = Task::new("add retry backoff to the postgres connection pool");
        store.save_task(&prior_task, "queued").await.unwrap();
        save_high_score_attempt(&store, &prior_task).await;

        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let (_tx, rx) = tokio::sync::oneshot::channel();
        let mut prior_runner = AgentRunner::new(
            prior_task.clone(),
            PathBuf::from("/repo"),
            bus,
            None,
            rx,
            Arc::new(AtomicUsize::new(0)),
        );
        prior_runner.last_plan = Some(
            "Wrap the pool acquire call in exponential backoff with jitter\n\
             then add a connection-health check before returning it to the caller."
                .to_string(),
        );
        let constraint = prior_runner.success_constraint();
        assert!(
            constraint.is_some(),
            "a real plan always yields a constraint"
        );

        store
            .mine_patterns(&prior_task.id, &prior_task.goal, constraint.as_deref())
            .await
            .unwrap();
    }

    // A fresh task in the same toolchain, similar enough to surface the
    // backfilled pattern (shares "postgres"/"connection"/"pool").
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let (_tx, rx) = tokio::sync::oneshot::channel();
    let mut runner = AgentRunner::new(
        Task::new("fix a postgres connection pool leak under load"),
        PathBuf::from("/repo"),
        bus,
        Some(store),
        rx,
        Arc::new(AtomicUsize::new(0)),
    );

    let seed = runner.gather_seed().await;
    assert!(
        !seed.extra_constraints.is_empty(),
        "gather_seed must inject at least the backfilled pattern's constraint"
    );

    // The real planning-prompt builder — the exact function
    // `ClaudeCode::build_plan_prompt` calls for both the one-shot and
    // streaming plan paths — not a stand-in.
    let prompt = crate::claude_support::build_plan_prompt(
        &runner.task,
        None,
        &seed.extra_constraints,
        &seed.pattern_pairs,
        &seed.lessons_data,
    );

    println!("\n--- live planning prompt (Constraint-Capture-2 verification) ---");
    println!("{prompt}");
    println!("--- end planning prompt ---\n");

    assert!(
        prompt.contains("exponential backoff") || prompt.contains("connection-health check"),
        "the real planning prompt must contain the backfilled pattern's constraint, \
         not just PlanningSeed's in-memory fields — got:\n{prompt}"
    );

    // Best-effort cleanup — a leftover temp db is harmless but pointless
    // to keep, since this test's whole point was exercising a real
    // on-disk store, not accumulating one.
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}
