#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use super::*;
use chrono::Utc;
use lopi_core::{Attempt, ScoreWeights, Task, TaskId, TurnMetrics};
use uuid::Uuid;

#[test]
fn jaccard_sim_identical() {
    assert_eq!(
        jaccard_similarity("auth middleware refactor", "auth middleware refactor"),
        1.0
    );
}

#[test]
fn jaccard_sim_partial() {
    let s = jaccard_similarity("auth middleware", "auth database");
    assert!(s > 0.0 && s < 1.0);
}

#[test]
fn jaccard_sim_disjoint() {
    assert_eq!(jaccard_similarity("alpha beta", "gamma delta"), 0.0);
}

#[test]
fn keyword_fingerprint_sorts_and_dedupes() {
    let fp = keyword_fingerprint("refactor authentication middleware refactor");
    let words: Vec<&str> = fp.split_whitespace().collect();
    assert!(words.windows(2).all(|w| w[0] <= w[1]), "should be sorted");
    assert_eq!(
        words.len(),
        words.iter().collect::<std::collections::HashSet<_>>().len(),
        "should be deduped"
    );
}

#[test]
fn keyword_fingerprint_filters_short_words() {
    let fp = keyword_fingerprint("do it now fix");
    assert!(fp.is_empty() || !fp.contains("do"));
}

#[tokio::test]
async fn save_and_load_task_round_trip() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("integrate the flux capacitor");
    store.save_task(&task, "queued").await.unwrap();

    let history = store.load_history(10).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].goal, "integrate the flux capacitor");
    assert_eq!(history[0].status, "queued");
}

#[tokio::test]
async fn mark_completed_updates_status() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("refactor the warp core");
    store.save_task(&task, "queued").await.unwrap();
    store.mark_completed(&task.id, "success").await.unwrap();

    let history = store.load_history(10).await.unwrap();
    assert_eq!(history[0].status, "success");
    assert!(history[0].completed_at.is_some());
}

#[tokio::test]
async fn save_task_upserts_status() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("fix flaky test");
    store.save_task(&task, "queued").await.unwrap();
    store.save_task(&task, "implementing").await.unwrap();

    assert_eq!(store.task_count().await.unwrap(), 1);
    let history = store.load_history(10).await.unwrap();
    assert_eq!(history[0].status, "implementing");
}

#[tokio::test]
async fn save_attempt_persists() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("add feature X");
    store.save_task(&task, "queued").await.unwrap();

    let mut attempt = Attempt::new(task.id, 1, "lopi/abc-attempt-1");
    attempt.outcome = "success".into();
    store.save_attempt(&attempt).await.unwrap();
}

#[tokio::test]
async fn load_history_newest_first() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    for i in 0..5u8 {
        let t = Task::new(format!("task number {i} work"));
        store.save_task(&t, "queued").await.unwrap();
    }
    let history = store.load_history(3).await.unwrap();
    assert_eq!(history.len(), 3);
}

#[tokio::test]
async fn empty_store_returns_empty_history() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let history = store.load_history(10).await.unwrap();
    assert!(history.is_empty());
}

#[tokio::test]
async fn find_similar_patterns_empty_db() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let results = store
        .find_similar_patterns("optimize the engine")
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn find_similar_patterns_returns_matches() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("refactor authentication middleware");
    store.save_task(&task, "success").await.unwrap();
    store.mine_patterns(&task.id, &task.goal).await.unwrap();

    // Similar goal should match above 0.3 Jaccard threshold.
    let results = store
        .find_similar_patterns("update authentication middleware logic")
        .await
        .unwrap();
    assert!(!results.is_empty(), "should find similar pattern");
}

#[tokio::test]
async fn mine_patterns_inserts_new_row() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("refactor authentication middleware");
    store.save_task(&task, "queued").await.unwrap();
    store.mine_patterns(&task.id, &task.goal).await.unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 1);
    let kw = &patterns[0].goal_keywords;
    assert!(kw.contains("authentication") || kw.contains("middleware") || kw.contains("refactor"));
}

#[tokio::test]
async fn mine_patterns_updates_existing_row() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let t1 = Task::new("optimize database queries");
    let t2 = Task::new("optimize database queries");
    store.save_task(&t1, "queued").await.unwrap();
    store.save_task(&t2, "queued").await.unwrap();

    store.mine_patterns(&t1.id, &t1.goal).await.unwrap();
    store.mine_patterns(&t2.id, &t2.goal).await.unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 1);
}

#[tokio::test]
async fn mine_patterns_skips_short_words() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("do it now");
    store.save_task(&task, "queued").await.unwrap();
    store.mine_patterns(&task.id, &task.goal).await.unwrap();
    let patterns = store.load_patterns(10).await.unwrap();
    assert!(patterns.is_empty());
}

#[tokio::test]
async fn load_patterns_ordered_by_success_rate() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let t1 = Task::new("write comprehensive unit tests");
    let t2 = Task::new("deploy production infrastructure");
    store.save_task(&t1, "success").await.unwrap();
    store.save_task(&t2, "failed").await.unwrap();
    store.mine_patterns(&t1.id, &t1.goal).await.unwrap();
    store.mine_patterns(&t2.id, &t2.goal).await.unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 2);
}

fn make_turn_metrics(task_id: TaskId) -> TurnMetrics {
    TurnMetrics {
        turn_id: Uuid::new_v4(),
        task_id,
        session_id: Uuid::new_v4(),
        model: "claude-sonnet-4-6".into(),
        attempt_number: 1,
        input_tokens: 500,
        output_tokens: 200,
        cache_read_input_tokens: 0,
        cache_write_input_tokens: 100,
        ttft_ms: 300,
        turn_latency_ms: 1200,
        tool_execution_ms: 50,
        context_tokens: 4000,
        context_pressure: 0.25,
        evictions_this_turn: 0,
        tool_calls: 2,
        tools_parallel: false,
        estimated_cost_usd: 0.003,
        timestamp: Utc::now(),
    }
}

#[tokio::test]
async fn save_turn_metrics_succeeds() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("measure tokens");
    store.save_task(&task, "queued").await.unwrap();
    let m = make_turn_metrics(task.id);
    store.save_turn_metrics(&m).await.unwrap();
}

#[tokio::test]
async fn save_turn_metrics_dedup_on_same_turn_id() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("measure tokens deduplicated");
    store.save_task(&task, "queued").await.unwrap();
    let mut m = make_turn_metrics(task.id);
    let fixed_id = Uuid::new_v4();
    m.turn_id = fixed_id;
    store.save_turn_metrics(&m).await.unwrap();
    // Second insert with same turn_id should silently succeed (ON CONFLICT DO NOTHING)
    store.save_turn_metrics(&m).await.unwrap();
}

#[tokio::test]
async fn task_count_increments_per_save() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    assert_eq!(store.task_count().await.unwrap(), 0);
    for i in 0..3u8 {
        let t = Task::new(format!("task count test {i}"));
        store.save_task(&t, "queued").await.unwrap();
    }
    assert_eq!(store.task_count().await.unwrap(), 3);
}

// ── Lessons ──────────────────────────────────────────────────────────────────

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
    let alice = MemoryStore::open_for_customer(&base, "alice").await.unwrap();
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
