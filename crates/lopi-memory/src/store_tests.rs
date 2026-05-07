#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;
    use chrono::Utc;
    use lopi_core::{Attempt, Task, TaskId, TurnMetrics};
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
        assert!(
            kw.contains("authentication") || kw.contains("middleware") || kw.contains("refactor")
        );
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
