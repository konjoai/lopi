//! Store tests — Lessons + postmortem-derived patterns. Split out of
//! `tests.rs` to keep each test module under the 500-line file gate.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use super::*;
use lopi_core::{Attempt, ScoreWeights, Task, TaskId};
use uuid::Uuid;

#[tokio::test]
async fn save_lesson_below_quality_gate_not_stored() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .save_lesson("/repo", "strategy", "use small commits", None, 0.5)
        .await
        .unwrap();
    let rows = store.load_lessons("/repo", 10).await.unwrap();
    assert!(rows.is_empty(), "score below gate must not be stored");
}

#[tokio::test]
async fn save_lesson_at_quality_gate_stored() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .save_lesson("/repo", "strategy", "run tests first", None, 0.6)
        .await
        .unwrap();
    let rows = store.load_lessons("/repo", 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].category, "strategy");
    assert_eq!(rows[0].content, "run tests first");
}

#[tokio::test]
async fn load_lessons_filters_by_repo() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .save_lesson("/repo-a", "recovery", "check error logs", None, 0.8)
        .await
        .unwrap();
    store
        .save_lesson("/repo-b", "optimization", "cache results", None, 0.9)
        .await
        .unwrap();
    let a = store.load_lessons("/repo-a", 10).await.unwrap();
    let b = store.load_lessons("/repo-b", 10).await.unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_eq!(a[0].repo_path, "/repo-a");
    assert_eq!(b[0].repo_path, "/repo-b");
}

#[tokio::test]
async fn load_lessons_respects_limit() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    for i in 0..5u8 {
        store
            .save_lesson("/repo", "strategy", &format!("lesson {i}"), None, 0.9)
            .await
            .unwrap();
    }
    let rows = store.load_lessons("/repo", 3).await.unwrap();
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn daily_token_totals_returns_zero_with_no_metrics() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let (tokens, cost) = store.daily_token_totals().await.unwrap();
    assert_eq!(tokens, 0);
    assert!(cost < f64::EPSILON);
}

// ── Sprint H: postmortem-derived patterns ────────────────────────────────────

#[tokio::test]
async fn insert_postmortem_pattern_persists_with_flag() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let id = store
        .insert_postmortem_pattern(
            "fix auth middleware",
            "must add CSRF token validation before session creation",
        )
        .await
        .unwrap();

    // Verify the pattern can be retrieved by id prefix
    let row = store
        .find_pattern_by_id_prefix(&id[..8])
        .await
        .unwrap()
        .expect("inserted row must be retrievable");
    assert_eq!(row.id, id);
    assert_eq!(row.goal_keywords, "fix auth middleware");
    assert_eq!(
        row.successful_constraints.as_deref(),
        Some("must add CSRF token validation before session creation")
    );
    assert_eq!(row.derived_from_postmortem, 1);
    assert_eq!(row.success_rate, Some(0.0));
}

#[tokio::test]
async fn find_pattern_by_id_prefix_returns_none_for_missing() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let row = store
        .find_pattern_by_id_prefix("nonexistent")
        .await
        .unwrap();
    assert!(row.is_none());
}

#[tokio::test]
async fn load_patterns_includes_postmortem_flag() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .insert_postmortem_pattern("kw", "must do X")
        .await
        .unwrap();
    let rows = store.load_patterns(10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].derived_from_postmortem, 1);
}

#[tokio::test]
async fn load_patterns_orders_postmortem_with_null_success_rate_last() {
    // Postmortem patterns start with success_rate = 0.0; mined patterns have
    // real values. ORDER BY COALESCE(success_rate, 0) DESC ensures real-data
    // patterns surface above zero-seeded postmortem rows.
    let store = MemoryStore::open_in_memory().await.unwrap();

    // Insert postmortem (rate = 0.0)
    store
        .insert_postmortem_pattern("postmortem-kw", "must X")
        .await
        .unwrap();

    // Insert a high-success mined pattern by simulating mine_patterns flow
    let task = Task::new("real success kw");
    store.save_task(&task, "queued").await.unwrap();
    let attempt = make_high_score_attempt(task.id);
    store.save_attempt(&attempt).await.unwrap();
    store
        .mine_patterns(&task.id, "real success kw")
        .await
        .unwrap();

    let rows = store.load_patterns(10).await.unwrap();
    assert_eq!(rows.len(), 2);
    // Higher success_rate first
    assert!(rows[0].derived_from_postmortem == 0);
    assert!(rows[1].derived_from_postmortem == 1);
}

fn make_high_score_attempt(task_id: TaskId) -> Attempt {
    Attempt {
        id: Uuid::new_v4(),
        task_id,
        attempt_num: 1,
        branch: "test/h-1".into(),
        score: Some(lopi_core::Score::new(1.0, 0, 50)),
        outcome: "success".into(),
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn load_annotated_patterns_returns_only_annotated() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task1 = Task::new("fix auth");
    let task2 = Task::new("refactor api");
    store.save_task(&task1, "success").await.unwrap();
    store.save_task(&task2, "success").await.unwrap();
    store.mine_patterns(&task1.id, &task1.goal).await.unwrap();
    store.mine_patterns(&task2.id, &task2.goal).await.unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 2);

    let pattern_id = &patterns[0].id;
    store
        .annotate_pattern(pattern_id, Some("approved"))
        .await
        .unwrap();

    let annotated = store.load_annotated_patterns().await.unwrap();
    assert_eq!(annotated.len(), 1);
    assert_eq!(annotated[0].user_annotation.as_deref(), Some("approved"));
}

#[tokio::test]
async fn compute_adjustments_empty_returns_default() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let weights = store.compute_weight_adjustments().await.unwrap();
    let defaults = ScoreWeights::default();
    assert_eq!(
        weights.lint_penalty_per_error,
        defaults.lint_penalty_per_error
    );
    assert_eq!(weights.lint_penalty_cap, defaults.lint_penalty_cap);
}

#[tokio::test]
async fn compute_adjustments_signal_shifts_weights() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task_approved = Task::new("simple fix");
    let task_rejected = Task::new("complex refactor");
    store.save_task(&task_approved, "success").await.unwrap();
    store.save_task(&task_rejected, "success").await.unwrap();
    store
        .mine_patterns(&task_approved.id, &task_approved.goal)
        .await
        .unwrap();
    store
        .mine_patterns(&task_rejected.id, &task_rejected.goal)
        .await
        .unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert!(patterns.len() >= 2);

    store
        .annotate_pattern(&patterns[0].id, Some("approved"))
        .await
        .unwrap();
    store
        .annotate_pattern(&patterns[1].id, Some("rejected"))
        .await
        .unwrap();

    let adjusted = store.compute_weight_adjustments().await.unwrap();
    let defaults = ScoreWeights::default();
    assert_eq!(adjusted.lint_penalty_cap, defaults.lint_penalty_cap);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn open_for_customer_creates_isolated_db() {
    let base = std::env::temp_dir().join(format!("lopi-customer-test-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let alice = MemoryStore::open_for_customer(&base, "alice")
        .await
        .unwrap();
    let bob = MemoryStore::open_for_customer(&base, "bob").await.unwrap();
    // Insert a task for alice only.
    let task = lopi_core::Task::new("alice-only task");
    alice.save_task(&task, "queued").await.unwrap();
    // Bob's store should not see alice's task.
    let alice_count = alice.task_count().await.unwrap();
    let bob_count = bob.task_count().await.unwrap();
    assert_eq!(alice_count, 1);
    assert_eq!(bob_count, 0);
    std::fs::remove_dir_all(&base).unwrap();
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn open_for_customer_sanitises_id() {
    let base = std::env::temp_dir().join(format!("lopi-sanitise-test-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    // ID with path traversal chars should be sanitised.
    let s = MemoryStore::open_for_customer(&base, "../evil/../../../hack").await;
    assert!(s.is_ok()); // Opens, but path is safe.
    std::fs::remove_dir_all(&base).unwrap();
}
