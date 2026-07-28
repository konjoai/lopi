//! Sprint E, Part 2 — `CostEstimator`: median/p90 total cost per
//! `(repo, stage, model, effort)`, from the last N completed runs.
//!
//! "Cold start with conservative defaults from config; widen the interval
//! when the sample is small and say so." A bucket with zero history uses
//! the configured default outright (`cold_start = true`, `sample_size =
//! 0`); a bucket with *some* history but fewer samples than
//! `cold_start_sample_min` still computes real percentiles but widens the
//! p90 by a safety margin and still reports `cold_start = true` — small
//! samples are noisy, and admission should err toward declining rather
//! than trusting a p90 computed from two runs.

use lopi_core::Money;
use lopi_memory::MemoryStore;

/// How many historical runs to look back over when computing percentiles.
/// Bounds the query — old history stops informing today's estimate once a
/// bucket has this many more-recent samples.
const HISTORY_LOOKBACK: i64 = 50;

/// Safety multiplier applied to p90 when the sample is smaller than
/// `cold_start_sample_min` but non-empty — "widen the interval."
const SMALL_SAMPLE_P90_WIDEN: f64 = 1.5;

/// One estimate: median + p90 total cost for a `(repo, stage, model,
/// effort)` bucket, plus how much history backed it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostEstimate {
    /// Median total cost across the historical sample.
    pub median: Money,
    /// 90th-percentile total cost — what admission reserves against.
    pub p90: Money,
    /// How many historical runs this estimate is based on.
    pub sample_size: usize,
    /// `true` when this estimate leans on the configured cold-start
    /// default (zero history) or a widened interval (a small-but-nonzero
    /// sample) rather than a confident historical percentile.
    pub cold_start: bool,
}

/// Reads the token ledger (via `lopi-memory`) to estimate cost per pipeline
/// stage. Cheap to clone — holds only a `MemoryStore` handle and two
/// config scalars.
#[derive(Clone)]
pub struct CostEstimator {
    store: MemoryStore,
    cold_start_sample_min: usize,
    cold_start_default: Money,
}

impl CostEstimator {
    /// Build an estimator. `cold_start_default` is the conservative
    /// per-run cost assumed before any history exists for a bucket —
    /// should come from config, never a hardcoded literal buried in this
    /// module (per the brief's "rate tables live in config, never in
    /// code" spirit — this is the analogous rule for cost *estimates*).
    #[must_use]
    pub const fn new(
        store: MemoryStore,
        cold_start_sample_min: usize,
        cold_start_default: Money,
    ) -> Self {
        Self {
            store,
            cold_start_sample_min,
            cold_start_default,
        }
    }

    /// Estimate cost for one `(repo, stage, model, effort)` bucket.
    ///
    /// # Errors
    /// Returns `Err` if the underlying store query fails.
    pub async fn estimate(
        &self,
        repo: Option<&str>,
        stage: &str,
        model: &str,
        effort: Option<&str>,
    ) -> anyhow::Result<CostEstimate> {
        let mut samples = self
            .store
            .stage_cost_samples(repo, stage, model, effort, HISTORY_LOOKBACK)
            .await?;

        if samples.is_empty() {
            return Ok(CostEstimate {
                median: self.cold_start_default,
                p90: self.cold_start_default,
                sample_size: 0,
                cold_start: true,
            });
        }

        samples.sort_by(f64::total_cmp);
        let median = Money::from_usd(percentile(&samples, 0.5));
        let mut p90 = Money::from_usd(percentile(&samples, 0.9));

        let cold_start = samples.len() < self.cold_start_sample_min;
        if cold_start {
            p90 = Money::from_usd(p90.to_usd() * SMALL_SAMPLE_P90_WIDEN);
        }

        Ok(CostEstimate {
            median,
            p90,
            sample_size: samples.len(),
            cold_start,
        })
    }
}

/// Nearest-rank percentile over an already-sorted slice. `p` in `[0.0,
/// 1.0]`. Empty input returns `0.0` — callers handle the empty case before
/// reaching here (the cold-start branch above), so this is a defensive
/// fallback, not a real code path.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lopi_core::{Task, TaskId, TurnMetrics};
    use uuid::Uuid;

    fn turn(task_id: TaskId, cost: f64) -> TurnMetrics {
        TurnMetrics {
            turn_id: Uuid::new_v4(),
            task_id,
            session_id: Uuid::new_v4(),
            model: "claude-sonnet-5".into(),
            attempt_number: 1,
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: 0,
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
            timestamp: chrono::Utc::now(),
            stage: "implement".into(),
            effort: None,
        }
    }

    #[test]
    fn percentile_matches_hand_computed_values() {
        let sorted = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert!((percentile(&sorted, 0.5) - 5.0).abs() < 1e-9);
        assert!((percentile(&sorted, 0.9) - 9.0).abs() < 1e-9);
        assert!((percentile(&sorted, 1.0) - 10.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn zero_history_returns_cold_start_default() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let estimator = CostEstimator::new(store, 5, Money::from_usd(2.0));
        let est = estimator
            .estimate(None, "implement", "claude-sonnet-5", None)
            .await
            .unwrap();
        assert!(est.cold_start);
        assert_eq!(est.sample_size, 0);
        assert_eq!(est.median, Money::from_usd(2.0));
        assert_eq!(est.p90, Money::from_usd(2.0));
    }

    #[tokio::test]
    async fn small_sample_widens_p90_but_still_reports_real_median() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        for cost in [1.0, 2.0] {
            let t = Task::new(format!("fixture {cost}"));
            store.save_task(&t, "queued").await.unwrap();
            store.save_turn_metrics(&turn(t.id, cost)).await.unwrap();
        }
        // cold_start_sample_min = 5, only 2 samples exist.
        let estimator = CostEstimator::new(store, 5, Money::from_usd(2.0));
        let est = estimator
            .estimate(None, "implement", "claude-sonnet-5", None)
            .await
            .unwrap();
        assert!(est.cold_start);
        assert_eq!(est.sample_size, 2);
        // p90 of [1.0, 2.0] is 2.0, widened by 1.5x.
        assert!((est.p90.to_usd() - 3.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn confident_sample_reports_real_percentiles_unwidened() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        for cost in [1.0, 2.0, 3.0, 4.0, 5.0] {
            let t = Task::new(format!("fixture {cost}"));
            store.save_task(&t, "queued").await.unwrap();
            store.save_turn_metrics(&turn(t.id, cost)).await.unwrap();
        }
        let estimator = CostEstimator::new(store, 5, Money::from_usd(2.0));
        let est = estimator
            .estimate(None, "implement", "claude-sonnet-5", None)
            .await
            .unwrap();
        assert!(!est.cold_start);
        assert_eq!(est.sample_size, 5);
        assert!((est.median.to_usd() - 3.0).abs() < 1e-9);
        assert!((est.p90.to_usd() - 5.0).abs() < 1e-9);
    }
}
