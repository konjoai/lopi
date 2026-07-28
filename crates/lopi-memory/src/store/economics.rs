//! Sprint E — the queries `lopi_orchestrator::budget::{estimate,report}` read
//! from. All of it is projections over `turn_metrics`/`eval_outcomes`/
//! `tasks` — no new tables, matching the brief's "everything derived from
//! the ledger, do not add a second source of truth for spend."

use super::MemoryStore;
use anyhow::Result;

/// One completed turn's raw token counts + actual billed cost — the input
/// `budget::report::cache_attributed_saving` needs to recompute "what would
/// this turn have cost at zero cache hit rate" per-row (the rate table
/// lives in `lopi-agent`, which `lopi-memory` cannot depend on, so this
/// stays as raw numbers for the caller to price).
#[derive(Debug, Clone, PartialEq)]
pub struct CachePricingSample {
    /// Model this turn ran on (keys the pricing tier lookup).
    pub model: String,
    /// Prompt tokens billed at the full input rate.
    pub input_tokens: i64,
    /// Prompt tokens served from cache (billed at the cheaper cache-read rate).
    pub cache_read_tokens: i64,
    /// Prompt tokens written into cache this turn (billed at the cache-write rate).
    pub cache_write_tokens: i64,
    /// Completion tokens.
    pub output_tokens: i64,
    /// What was actually billed for this turn.
    pub actual_cost_usd: f64,
}

impl MemoryStore {
    /// Total billed cost of every completed turn for one `(repo, stage,
    /// model, effort)` bucket, one entry per task — the raw sample
    /// [`crate::MemoryStore`]'s caller (`CostEstimator`) computes
    /// median/p90 over. Most-recent-first, capped at `limit`.
    ///
    /// `repo`/`effort` are matched with SQL `IS` (not `=`) so `None`
    /// correctly matches a `NULL` column instead of matching nothing.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn stage_cost_samples(
        &self,
        repo: Option<&str>,
        stage: &str,
        model: &str,
        effort: Option<&str>,
        limit: i64,
    ) -> Result<Vec<f64>> {
        let rows: Vec<(f64,)> = sqlx::query_as(
            "SELECT SUM(tm.estimated_cost_usd) FROM turn_metrics tm \
             JOIN tasks t ON t.id = tm.task_id \
             WHERE tm.stage = ?1 AND tm.model = ?2 AND tm.effort IS ?3 AND t.repo IS ?4 \
             GROUP BY tm.task_id \
             ORDER BY MAX(tm.timestamp) DESC \
             LIMIT ?5",
        )
        .bind(stage)
        .bind(model)
        .bind(effort)
        .bind(repo)
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }

    /// Cumulative billed spend across every turn ever recorded — the
    /// numerator for "cost per merged PR" / "cost per gate pass".
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn total_spend_all_time(&self) -> Result<f64> {
        let row: (Option<f64>,) =
            sqlx::query_as("SELECT SUM(estimated_cost_usd) FROM turn_metrics")
                .fetch_one(&self.read_pool)
                .await?;
        Ok(row.0.unwrap_or(0.0))
    }

    /// Count of tasks that reached `TaskStatus::Success`. **Not** "merged
    /// PR" — lopi persists only the coarse `db_status` string
    /// (`tasks.status`), never `pr_url` or GitHub's post-open merge state,
    /// so this is the closest available proxy. Documented in `LEDGER.md`'s
    /// Sprint E entry rather than silently overclaiming a merge signal
    /// lopi doesn't actually have.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn success_task_count(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tasks WHERE status = 'success'")
            .fetch_one(&self.read_pool)
            .await?;
        Ok(row.0)
    }

    /// Count of tasks whose very first attempt (`eval_outcomes.attempt =
    /// 1`) passed the verification gate (Sprint G's `EvalOutcome`,
    /// `verdict = 'pass'`) — the denominator for "cost per gate pass".
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn first_attempt_gate_pass_count(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM eval_outcomes WHERE attempt = 1 AND verdict = 'pass'",
        )
        .fetch_one(&self.read_pool)
        .await?;
        Ok(row.0)
    }

    /// Total billed spend on attempts 2 and later — pure retry waste, per
    /// the brief's "cost per retry... should trend down as the harness
    /// improves."
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn retry_spend(&self) -> Result<f64> {
        let row: (Option<f64>,) = sqlx::query_as(
            "SELECT SUM(estimated_cost_usd) FROM turn_metrics WHERE attempt_number >= 2",
        )
        .fetch_one(&self.read_pool)
        .await?;
        Ok(row.0.unwrap_or(0.0))
    }

    /// Raw per-turn token/cost samples over the trailing `days` — the input
    /// to `budget::report::cache_attributed_saving`'s per-row
    /// zero-cache-hit-rate recomputation.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn cache_pricing_samples(&self, days: i64) -> Result<Vec<CachePricingSample>> {
        let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let rows: Vec<(String, i64, i64, i64, i64, f64)> = sqlx::query_as(
            "SELECT model, input_tokens, cache_read_tokens, cache_write_tokens, \
                    output_tokens, estimated_cost_usd \
             FROM turn_metrics WHERE timestamp >= ?1",
        )
        .bind(since)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(model, input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, actual_cost_usd)| {
                    CachePricingSample {
                        model,
                        input_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        output_tokens,
                        actual_cost_usd,
                    }
                },
            )
            .collect())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::Utc;
    use lopi_core::{Task, TaskId, TurnMetrics};
    use uuid::Uuid;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    async fn seed_task(s: &MemoryStore, repo: Option<&str>) -> TaskId {
        let task = Task::new("economics fixture");
        s.save_task(&task, "queued").await.unwrap();
        if let Some(r) = repo {
            s.set_task_repo(&task.id, r).await.unwrap();
        }
        task.id
    }

    fn turn(
        task_id: TaskId,
        model: &str,
        stage: &str,
        effort: Option<&str>,
        attempt: u8,
        cost: f64,
    ) -> TurnMetrics {
        TurnMetrics {
            turn_id: Uuid::new_v4(),
            task_id,
            session_id: Uuid::new_v4(),
            model: model.into(),
            attempt_number: attempt,
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_input_tokens: 200,
            cache_write_input_tokens: 50,
            ttft_ms: 0,
            turn_latency_ms: 0,
            tool_execution_ms: 0,
            context_tokens: 0,
            context_pressure: 0.0,
            evictions_this_turn: 0,
            tool_calls: 0,
            tools_parallel: false,
            estimated_cost_usd: cost,
            timestamp: Utc::now(),
            stage: stage.into(),
            effort: effort.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn stage_cost_samples_groups_per_task_and_matches_null_effort() {
        let s = store().await;
        let t1 = seed_task(&s, Some("/repo/a")).await;
        let t2 = seed_task(&s, Some("/repo/a")).await;
        s.save_turn_metrics(&turn(t1, "claude-sonnet-5", "implement", None, 1, 0.10))
            .await
            .unwrap();
        s.save_turn_metrics(&turn(t2, "claude-sonnet-5", "implement", None, 1, 0.20))
            .await
            .unwrap();
        // Different stage — must not be included.
        s.save_turn_metrics(&turn(t1, "claude-sonnet-5", "plan", None, 1, 5.00))
            .await
            .unwrap();

        let samples = s
            .stage_cost_samples(Some("/repo/a"), "implement", "claude-sonnet-5", None, 10)
            .await
            .unwrap();
        assert_eq!(samples.len(), 2);
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((sorted[0] - 0.10).abs() < 1e-9);
        assert!((sorted[1] - 0.20).abs() < 1e-9);
    }

    #[tokio::test]
    async fn stage_cost_samples_respects_limit_and_recency_order() {
        let s = store().await;
        for i in 0..5 {
            let t = seed_task(&s, None).await;
            s.save_turn_metrics(&turn(t, "claude-sonnet-5", "implement", None, 1, f64::from(i)))
                .await
                .unwrap();
        }
        let samples = s
            .stage_cost_samples(None, "implement", "claude-sonnet-5", None, 3)
            .await
            .unwrap();
        assert_eq!(samples.len(), 3);
    }

    #[tokio::test]
    async fn stage_cost_samples_effort_bucket_is_distinct_from_none() {
        let s = store().await;
        let t1 = seed_task(&s, None).await;
        let t2 = seed_task(&s, None).await;
        s.save_turn_metrics(&turn(t1, "claude-sonnet-5", "implement", Some("high"), 1, 1.0))
            .await
            .unwrap();
        s.save_turn_metrics(&turn(t2, "claude-sonnet-5", "implement", None, 1, 2.0))
            .await
            .unwrap();

        let high = s
            .stage_cost_samples(None, "implement", "claude-sonnet-5", Some("high"), 10)
            .await
            .unwrap();
        assert_eq!(high, vec![1.0]);

        let none = s
            .stage_cost_samples(None, "implement", "claude-sonnet-5", None, 10)
            .await
            .unwrap();
        assert_eq!(none, vec![2.0]);
    }

    #[tokio::test]
    async fn total_spend_all_time_sums_every_turn() {
        let s = store().await;
        let t = seed_task(&s, None).await;
        s.save_turn_metrics(&turn(t, "claude-sonnet-5", "plan", None, 1, 1.5))
            .await
            .unwrap();
        s.save_turn_metrics(&turn(t, "claude-sonnet-5", "implement", None, 1, 2.5))
            .await
            .unwrap();
        assert!((s.total_spend_all_time().await.unwrap() - 4.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn total_spend_all_time_is_zero_on_empty_store() {
        let s = store().await;
        assert_eq!(s.total_spend_all_time().await.unwrap(), 0.0);
    }

    #[tokio::test]
    async fn success_task_count_only_counts_success_status() {
        let s = store().await;
        let t1 = Task::new("succeeds");
        s.save_task(&t1, "queued").await.unwrap();
        s.mark_completed(&t1.id, "success").await.unwrap();
        let t2 = Task::new("fails");
        s.save_task(&t2, "queued").await.unwrap();
        s.mark_completed(&t2.id, "failed").await.unwrap();

        assert_eq!(s.success_task_count().await.unwrap(), 1);
    }

    fn eval_outcome(verdict: lopi_core::Verdict) -> lopi_core::EvalOutcome {
        lopi_core::EvalOutcome {
            verdict,
            score: 1.0,
            per_check: vec![],
            critique: vec![],
        }
    }

    #[tokio::test]
    async fn first_attempt_gate_pass_count_excludes_later_attempts_and_failures() {
        let s = store().await;
        let t = seed_task(&s, None).await;
        let tid = t.0.to_string();
        s.save_eval_outcome(&tid, 1, &eval_outcome(lopi_core::Verdict::Pass))
            .await
            .unwrap();
        s.save_eval_outcome(&tid, 2, &eval_outcome(lopi_core::Verdict::Pass))
            .await
            .unwrap();
        let t2 = seed_task(&s, None).await;
        s.save_eval_outcome(
            &t2.0.to_string(),
            1,
            &eval_outcome(lopi_core::Verdict::Fail),
        )
        .await
        .unwrap();

        assert_eq!(s.first_attempt_gate_pass_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn retry_spend_excludes_first_attempts() {
        let s = store().await;
        let t = seed_task(&s, None).await;
        s.save_turn_metrics(&turn(t, "claude-sonnet-5", "implement", None, 1, 1.0))
            .await
            .unwrap();
        s.save_turn_metrics(&turn(t, "claude-sonnet-5", "implement", None, 2, 3.0))
            .await
            .unwrap();
        s.save_turn_metrics(&turn(t, "claude-sonnet-5", "implement", None, 3, 4.0))
            .await
            .unwrap();
        assert!((s.retry_spend().await.unwrap() - 7.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn cache_pricing_samples_returns_raw_token_rows() {
        let s = store().await;
        let t = seed_task(&s, None).await;
        s.save_turn_metrics(&turn(t, "claude-sonnet-5", "implement", None, 1, 1.0))
            .await
            .unwrap();
        let samples = s.cache_pricing_samples(7).await.unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].model, "claude-sonnet-5");
        assert_eq!(samples[0].input_tokens, 1000);
        assert_eq!(samples[0].cache_read_tokens, 200);
    }
}
