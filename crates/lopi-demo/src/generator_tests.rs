//! Tests for [`super::generate`] and (per the sprint spec) [`crate::scenario::replay_events`].
//! Split out of `generator.rs` purely to keep that file under the 500-line
//! CI file-size gate — same rationale as `lopi-core`'s `task_tests.rs`
//! split from `task.rs`.

use super::*;
use lopi_memory::AuditQuery;
use tempfile::tempdir;

// --- Isolation guard -------------------------------------------------------

#[tokio::test]
async fn refuses_when_dest_equals_real_store_exactly() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("lopi.db");
    let result = generate(&path, &path, 1).await;
    assert!(result.is_err());
    assert!(!path.exists(), "must write nothing on refusal");
}

#[tokio::test]
async fn refuses_when_equivalent_via_dot_dot_and_file_already_exists() {
    let dir = tempdir().unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let real = sub.join("lopi.db");
    std::fs::write(&real, b"").unwrap();
    let dest = sub.join("..").join("sub").join("lopi.db");
    assert_ne!(dest, real, "literal path strings must differ");
    let result = generate(&dest, &real, 1).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn refuses_when_neither_file_nor_parent_dir_exists_yet() {
    let dir = tempdir().unwrap();
    let real = dir.path().join("nested").join("lopi.db");
    // `..` (unlike `.`) is not normalized away by `Path`'s own equality, so
    // this is a genuinely different literal path that still lexically
    // resolves to the same file — and nothing on either path exists yet,
    // not even the `nested` directory.
    let dest = dir
        .path()
        .join("nested")
        .join("sibling")
        .join("..")
        .join("lopi.db");
    assert_ne!(dest, real, "literal path strings must differ");
    assert!(!dir.path().join("nested").exists());
    let result = generate(&dest, &real, 1).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn succeeds_for_clearly_different_paths() {
    let dir = tempdir().unwrap();
    let dest = dir.path().join("demo.db");
    let real = dir.path().join("lopi.db");
    let result = generate(&dest, &real, 1).await;
    assert!(result.is_ok());
}

// --- Determinism -------------------------------------------------------

#[tokio::test]
async fn same_seed_produces_identical_content_across_dests() {
    let dir = tempdir().unwrap();
    let dest_a = dir.path().join("a").join("demo.db");
    let dest_b = dir.path().join("b").join("demo.db");
    let real = dir.path().join("real.db");

    let summary_a = generate(&dest_a, &real, 42).await.unwrap();
    let summary_b = generate(&dest_b, &real, 42).await.unwrap();
    assert_eq!(summary_a.task_count, summary_b.task_count);

    let store_a = MemoryStore::open(&dest_a).await.unwrap();
    let store_b = MemoryStore::open(&dest_b).await.unwrap();

    let mut history_a = store_a.load_history(200).await.unwrap();
    let mut history_b = store_b.load_history(200).await.unwrap();
    history_a.sort_by(|x, y| x.id.cmp(&y.id));
    history_b.sort_by(|x, y| x.id.cmp(&y.id));
    assert_eq!(history_a.len(), history_b.len());
    for (a, b) in history_a.iter().zip(history_b.iter()) {
        assert_eq!(
            a.id, b.id,
            "task ids must be byte-identical for the same seed"
        );
        assert_eq!(a.goal, b.goal);
        assert_eq!(a.status, b.status);
        assert_eq!(a.repo, b.repo);
    }

    let repos_a: Vec<_> = store_a.load_demo_repos().await.unwrap();
    let repos_b: Vec<_> = store_b.load_demo_repos().await.unwrap();
    let names_a: Vec<_> = repos_a.iter().map(|r| r.name.clone()).collect();
    let names_b: Vec<_> = repos_b.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names_a, names_b);

    let mut patterns_a = store_a.load_patterns(20).await.unwrap();
    let mut patterns_b = store_b.load_patterns(20).await.unwrap();
    patterns_a.sort_by(|x, y| x.id.cmp(&y.id));
    patterns_b.sort_by(|x, y| x.id.cmp(&y.id));
    assert_eq!(patterns_a.len(), patterns_b.len());
    for (a, b) in patterns_a.iter().zip(patterns_b.iter()) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.goal_keywords, b.goal_keywords);
    }

    let mut lessons_a = Vec::new();
    let mut lessons_b = Vec::new();
    for repo in &repos_a {
        lessons_a.extend(store_a.load_lessons(&repo.path, 20).await.unwrap());
    }
    for repo in &repos_b {
        lessons_b.extend(store_b.load_lessons(&repo.path, 20).await.unwrap());
    }
    let mut content_a: Vec<_> = lessons_a.iter().map(|l| l.content.clone()).collect();
    let mut content_b: Vec<_> = lessons_b.iter().map(|l| l.content.clone()).collect();
    content_a.sort();
    content_b.sort();
    assert_eq!(content_a, content_b);
}

// --- Different seeds differ ---------------------------------------------

#[tokio::test]
async fn different_seeds_produce_different_task_plans() {
    let dir = tempdir().unwrap();
    let dest1 = dir.path().join("s1").join("demo.db");
    let dest2 = dir.path().join("s2").join("demo.db");
    let real = dir.path().join("real.db");

    generate(&dest1, &real, 1).await.unwrap();
    generate(&dest2, &real, 2).await.unwrap();

    let store1 = MemoryStore::open(&dest1).await.unwrap();
    let store2 = MemoryStore::open(&dest2).await.unwrap();
    let h1 = store1.load_history(200).await.unwrap();
    let h2 = store2.load_history(200).await.unwrap();
    let mut goals1: Vec<_> = h1.iter().map(|r| r.goal.clone()).collect();
    let mut goals2: Vec<_> = h2.iter().map(|r| r.goal.clone()).collect();
    goals1.sort();
    goals2.sort();
    assert_ne!(
        goals1, goals2,
        "different seeds must not produce identical goal sets"
    );
}

// --- Content coverage ----------------------------------------------------

#[tokio::test]
async fn content_coverage_hits_every_status_and_pool() {
    let dir = tempdir().unwrap();
    let dest = dir.path().join("demo.db");
    let real = dir.path().join("real.db");
    generate(&dest, &real, DEFAULT_DEMO_SEED).await.unwrap();
    let store = MemoryStore::open(&dest).await.unwrap();

    assert!(store.is_synthetic().await.unwrap());

    let history = store.load_history(200).await.unwrap();
    for status in [
        "queued",
        "running",
        "success",
        "failed",
        "rolled_back",
        "conflict",
    ] {
        assert!(
            history.iter().any(|r| r.status == status),
            "missing at least one task with status {status}"
        );
    }

    let repos = store.load_demo_repos().await.unwrap();
    assert!(repos.len() >= 4);

    let patterns = store.load_patterns(20).await.unwrap();
    assert!(patterns.len() >= 4);

    let mut lesson_count = 0;
    for repo in &repos {
        lesson_count += store.load_lessons(&repo.path, 20).await.unwrap().len();
    }
    assert!(lesson_count >= 4);

    let dead_letters = store
        .query_audit(&AuditQuery {
            action: Some("task.dead_letter".to_string()),
            limit: 10,
            ..AuditQuery::default()
        })
        .await
        .unwrap();
    assert!(dead_letters.len() >= 2);
}

// --- scenario::replay_events -----------------------------------------------

fn event_task_id(e: &lopi_core::AgentEvent) -> Option<lopi_core::TaskId> {
    use lopi_core::AgentEvent::{LogLine, StatusChanged, TaskCompleted, TaskQueued, TaskStarted};
    match e {
        TaskQueued { task_id, .. }
        | TaskStarted { task_id, .. }
        | StatusChanged { task_id, .. }
        | LogLine { task_id, .. }
        | TaskCompleted { task_id, .. } => Some(*task_id),
        _ => None,
    }
}

#[test]
fn replay_events_deterministic_count_and_task_ids() {
    let a = crate::scenario::replay_events(7);
    let b = crate::scenario::replay_events(7);
    assert_eq!(a.len(), b.len());

    let ids_a: std::collections::HashSet<_> = a.iter().filter_map(event_task_id).collect();
    let ids_b: std::collections::HashSet<_> = b.iter().filter_map(event_task_id).collect();
    assert_eq!(ids_a, ids_b);
}

#[test]
fn replay_events_queued_task_has_no_started_or_completed() {
    let plans = crate::generator_content::build_task_plans(7);
    let events = crate::scenario::replay_events(7);
    for plan in plans.iter().filter(|p| p.outcome == TaskOutcome::Queued) {
        let started = events.iter().any(|e| {
            matches!(e, lopi_core::AgentEvent::TaskStarted { task_id, .. } if *task_id == plan.id)
        });
        let completed = events.iter().any(|e| {
            matches!(e, lopi_core::AgentEvent::TaskCompleted { task_id, .. } if *task_id == plan.id)
        });
        assert!(!started, "a queued-only task must never get TaskStarted");
        assert!(
            !completed,
            "a queued-only task must never get TaskCompleted"
        );
    }
}

#[test]
fn replay_events_terminal_tasks_get_completed() {
    let plans = crate::generator_content::build_task_plans(7);
    let events = crate::scenario::replay_events(7);
    for plan in plans.iter().filter(|p| p.outcome.is_terminal()) {
        let completed = events.iter().any(|e| {
            matches!(e, lopi_core::AgentEvent::TaskCompleted { task_id, .. } if *task_id == plan.id)
        });
        assert!(
            completed,
            "every terminal-status task must get TaskCompleted"
        );
    }
}
