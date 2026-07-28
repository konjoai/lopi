//! Sprint E, Part 5 — unit economics, surfaced.
//!
//! "Cost per token is not a number anyone can act on." Everything here is
//! derived from the existing ledger (`turn_metrics`/`eval_outcomes`/
//! `tasks` via `lopi-memory::store::economics`) — no second source of
//! truth for spend, per the brief.

use super::pool::PoolState;
use anyhow::Result;
use lopi_core::Money;
use lopi_memory::MemoryStore;

/// The five unit-economics numbers plus pool runway — what `/cost`, the web
/// cost page, and the TUI header all read from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitEconomics {
    /// Total spend divided by count of tasks that reached
    /// `TaskStatus::Success`. **Labeled, not literal** — lopi persists no
    /// `pr_url`/merge signal (see `LEDGER.md`'s Sprint E entry), so this is
    /// "cost per completed task," the closest available proxy for "cost
    /// per merged PR." `None` when no task has succeeded yet.
    pub cost_per_merged_pr: Option<Money>,
    /// Total spend divided by count of first-attempt gate passes
    /// (`eval_outcomes.attempt = 1 AND verdict = 'pass'`). Rising means
    /// quality is degrading before throughput does. `None` when no
    /// first-attempt pass has ever landed.
    pub cost_per_gate_pass: Option<Money>,
    /// Total spend on attempts 2 and later — pure retry waste, literal
    /// dollars (not divided by a count; the brief specifies this as a raw
    /// spend figure, not a ratio).
    pub cost_per_retry: Money,
    /// What the sampled turns would have cost at zero cache hit rate,
    /// minus what was actually billed — the direct evidence for whether
    /// Sprint C's caching work paid for itself.
    pub cache_attributed_saving: Money,
    /// Days remaining at the trailing burn rate before the active pool's
    /// headroom hits zero. `f64::INFINITY` when there's no burn to
    /// extrapolate from.
    pub pool_runway_days: f64,
}

/// Compute [`UnitEconomics`] against the current ledger and pool state.
/// `cache_saving_window_days` bounds the cache-saving recomputation (it
/// re-prices every sampled turn, so an unbounded "all time" scan would grow
/// without limit); `burn_window_days` is the trailing window `pool_runway_days`
/// extrapolates from (7, per the brief's "trailing 7-day burn rate").
///
/// # Errors
/// Returns `Err` if any underlying store query fails.
pub async fn compute(
    store: &MemoryStore,
    pool: &PoolState,
    cache_saving_window_days: i64,
    burn_window_days: i64,
) -> Result<UnitEconomics> {
    let total_spend = store.total_spend_all_time().await?;
    let success_count = store.success_task_count().await?;
    let gate_pass_count = store.first_attempt_gate_pass_count().await?;
    let retry_spend = store.retry_spend().await?;
    let cache_attributed_saving = cache_saving(store, cache_saving_window_days).await?;

    let trend = store.daily_cost_trend(burn_window_days).await?;
    let daily_burn = average_daily_burn(&trend, burn_window_days);
    let pool_runway_days = pool.runway_days(Money::from_usd(daily_burn)).await;

    Ok(UnitEconomics {
        cost_per_merged_pr: ratio(total_spend, success_count),
        cost_per_gate_pass: ratio(total_spend, gate_pass_count),
        cost_per_retry: Money::from_usd(retry_spend),
        cache_attributed_saving,
        pool_runway_days,
    })
}

fn ratio(total_usd: f64, count: i64) -> Option<Money> {
    (count > 0).then(|| Money::from_usd(total_usd / count as f64))
}

#[allow(clippy::cast_precision_loss)]
fn average_daily_burn(trend: &[(String, f64)], window_days: i64) -> f64 {
    if window_days <= 0 {
        return 0.0;
    }
    let total: f64 = trend.iter().map(|(_, cost)| cost).sum();
    total / window_days as f64
}

/// Sum, over every turn sampled in the trailing `days`, of "what this turn
/// would have cost billed entirely at the input rate (no cache discount)"
/// minus what was actually billed. Floors at zero per-turn — a turn can
/// never have a negative saving (a turn priced *below* its no-cache
/// equivalent by construction).
async fn cache_saving(store: &MemoryStore, days: i64) -> Result<Money> {
    let samples = store.cache_pricing_samples(days).await?;
    let mut saving_usd = 0.0;
    for s in &samples {
        let rates = lopi_agent::pricing::rates_for(&s.model);
        let no_cache_input_tokens =
            (s.input_tokens + s.cache_read_tokens + s.cache_write_tokens) as f64;
        let cost_without_cache = no_cache_input_tokens / 1_000_000.0 * rates.input
            + s.output_tokens as f64 / 1_000_000.0 * rates.output;
        saving_usd += (cost_without_cache - s.actual_cost_usd).max(0.0);
    }
    Ok(Money::from_usd(saving_usd))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::cast_precision_loss)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, Utc};
    use lopi_core::{Pool, Task, TaskId, TurnMetrics, Verdict};
    use uuid::Uuid;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    fn sdk_pool(usd: f64) -> PoolState {
        PoolState::new(Pool::AgentSdkCredits {
            monthly_allotment: Money::from_usd(usd),
            resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        })
    }

    fn turn(task_id: TaskId, model: &str, attempt: u8, cost: f64) -> TurnMetrics {
        TurnMetrics {
            turn_id: Uuid::new_v4(),
            task_id,
            session_id: Uuid::new_v4(),
            model: model.into(),
            attempt_number: attempt,
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_read_input_tokens: 500_000,
            cache_write_input_tokens: 0,
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
            stage: "implement".into(),
            effort: None,
        }
    }

    #[tokio::test]
    async fn cost_per_merged_pr_is_none_with_no_successes() {
        let s = store().await;
        let pool = sdk_pool(100.0);
        let econ = compute(&s, &pool, 7, 7).await.unwrap();
        assert_eq!(econ.cost_per_merged_pr, None);
        assert_eq!(econ.cost_per_gate_pass, None);
        assert_eq!(econ.cost_per_retry, Money::ZERO);
    }

    #[tokio::test]
    async fn cost_per_merged_pr_divides_total_spend_by_success_count() {
        let s = store().await;
        let t1 = Task::new("succeeds 1");
        s.save_task(&t1, "queued").await.unwrap();
        s.mark_completed(&t1.id, "success").await.unwrap();
        s.save_turn_metrics(&turn(t1.id, "claude-sonnet-5", 1, 4.0))
            .await
            .unwrap();
        let t2 = Task::new("still running");
        s.save_task(&t2, "queued").await.unwrap();
        s.save_turn_metrics(&turn(t2.id, "claude-sonnet-5", 1, 2.0))
            .await
            .unwrap();

        let pool = sdk_pool(100.0);
        let econ = compute(&s, &pool, 7, 7).await.unwrap();
        // total spend 6.0, one success -> 6.0 per "merged PR" proxy.
        assert_eq!(econ.cost_per_merged_pr, Some(Money::from_usd(6.0)));
    }

    #[tokio::test]
    async fn cost_per_gate_pass_only_counts_first_attempt_passes() {
        let s = store().await;
        let t = Task::new("gate fixture");
        s.save_task(&t, "queued").await.unwrap();
        let tid = t.id.0.to_string();
        s.save_eval_outcome(
            &tid,
            1,
            &lopi_core::EvalOutcome {
                verdict: Verdict::Pass,
                score: 1.0,
                per_check: vec![],
                critique: vec![],
            },
        )
        .await
        .unwrap();
        s.save_turn_metrics(&turn(t.id, "claude-sonnet-5", 1, 10.0))
            .await
            .unwrap();

        let pool = sdk_pool(100.0);
        let econ = compute(&s, &pool, 7, 7).await.unwrap();
        assert_eq!(econ.cost_per_gate_pass, Some(Money::from_usd(10.0)));
    }

    #[tokio::test]
    async fn cost_per_retry_sums_only_attempts_two_and_later() {
        let s = store().await;
        let t = Task::new("retry fixture");
        s.save_task(&t, "queued").await.unwrap();
        s.save_turn_metrics(&turn(t.id, "claude-sonnet-5", 1, 1.0))
            .await
            .unwrap();
        s.save_turn_metrics(&turn(t.id, "claude-sonnet-5", 2, 5.0))
            .await
            .unwrap();

        let pool = sdk_pool(100.0);
        let econ = compute(&s, &pool, 7, 7).await.unwrap();
        assert_eq!(econ.cost_per_retry, Money::from_usd(5.0));
    }

    #[tokio::test]
    async fn cache_attributed_saving_is_positive_when_cache_was_used() {
        let s = store().await;
        let t = Task::new("cache fixture");
        s.save_task(&t, "queued").await.unwrap();
        // sonnet rates: input 3.00/M, output 15.0/M, cache_read 0.30/M.
        // 1M input @ full price would be $3.00, but 500K of the "input" here
        // is actually cache_read (billed far cheaper) — actual_cost_usd
        // reflects that discount, so the recomputed no-cache price must be
        // higher than what was billed.
        s.save_turn_metrics(&turn(t.id, "claude-sonnet-5", 1, 1.0))
            .await
            .unwrap();

        let pool = sdk_pool(100.0);
        let econ = compute(&s, &pool, 7, 7).await.unwrap();
        assert!(
            econ.cache_attributed_saving > Money::ZERO,
            "a turn with real cache_read_tokens must show a positive saving"
        );
    }

    #[tokio::test]
    async fn pool_runway_uses_trailing_burn_rate() {
        let s = store().await;
        let t = Task::new("burn fixture");
        s.save_task(&t, "queued").await.unwrap();
        s.save_turn_metrics(&turn(t.id, "claude-sonnet-5", 1, 7.0))
            .await
            .unwrap();

        // Pool with $70 headroom, ~$1/day average over a 7-day window
        // (7.0 total / 7 days) -> ~70 days of runway.
        let pool = sdk_pool(70.0);
        let econ = compute(&s, &pool, 7, 7).await.unwrap();
        assert!(
            econ.pool_runway_days > 60.0 && econ.pool_runway_days < 80.0,
            "expected roughly 70 days of runway, got {}",
            econ.pool_runway_days
        );
    }
}
