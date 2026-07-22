//! Budget-breakdown projections for the `/budget` dashboard — cost grouped by
//! model, and a daily spend trend. Both are read directly from `turn_metrics`,
//! the same durable per-turn cost ledger `daily_token_totals`/`task_costs`
//! already draw from. No new tables, no new write path.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{Duration, Utc};

use super::MemoryStore;

impl MemoryStore {
    /// Cost (USD) billed today (UTC), grouped by model, highest spend first.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn cost_by_model_today(&self) -> Result<Vec<(String, f64)>> {
        let rows: Vec<(String, f64)> = sqlx::query_as(
            "SELECT model, COALESCE(SUM(estimated_cost_usd), 0.0) \
             FROM turn_metrics WHERE timestamp >= ?1 \
             GROUP BY model ORDER BY 2 DESC",
        )
        .bind(start_of_day(0))
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Daily spend (USD) for the last `days` calendar days (UTC), oldest
    /// first, zero-filled for days with no recorded turns.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn daily_cost_trend(&self, days: i64) -> Result<Vec<(String, f64)>> {
        let rows: Vec<(String, f64)> = sqlx::query_as(
            "SELECT date(timestamp) AS day, COALESCE(SUM(estimated_cost_usd), 0.0) \
             FROM turn_metrics WHERE timestamp >= ?1 \
             GROUP BY day",
        )
        .bind(start_of_day(days - 1))
        .fetch_all(&self.read_pool)
        .await?;

        let by_day: HashMap<String, f64> = rows.into_iter().collect();
        let today = Utc::now().date_naive();
        Ok((0..days)
            .map(|i| {
                let key = (today - Duration::days(days - 1 - i))
                    .format("%Y-%m-%d")
                    .to_string();
                let cost = by_day.get(&key).copied().unwrap_or(0.0);
                (key, cost)
            })
            .collect())
    }
}

/// RFC-3339 timestamp for midnight UTC, `days_ago` calendar days before today.
/// Mirrors the day-boundary computation `daily_token_totals` already uses.
fn start_of_day(days_ago: i64) -> String {
    (Utc::now().date_naive() - Duration::days(days_ago))
        .and_hms_opt(0, 0, 0)
        .map(|dt| dt.and_utc().to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use lopi_core::{Task, TaskId, TurnMetrics};
    use uuid::Uuid;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    async fn seed_task(s: &MemoryStore) -> TaskId {
        let task = Task::new("budget breakdown fixture");
        s.save_task(&task, "queued").await.unwrap();
        task.id
    }

    fn turn(task_id: TaskId, model: &str, cost: f64, timestamp: DateTime<Utc>) -> TurnMetrics {
        TurnMetrics {
            turn_id: Uuid::new_v4(),
            task_id,
            session_id: Uuid::new_v4(),
            model: model.into(),
            attempt_number: 1,
            input_tokens: 10,
            output_tokens: 10,
            cache_read_input_tokens: 0,
            cache_write_input_tokens: 0,
            ttft_ms: 100,
            turn_latency_ms: 100,
            tool_execution_ms: 0,
            context_tokens: 100,
            context_pressure: 0.1,
            evictions_this_turn: 0,
            tool_calls: 0,
            tools_parallel: false,
            estimated_cost_usd: cost,
            timestamp,
        }
    }

    #[tokio::test]
    async fn cost_by_model_groups_and_orders_by_spend() {
        let s = store().await;
        let task = seed_task(&s).await;
        let now = Utc::now();
        s.save_turn_metrics(&turn(task, "opus", 1.0, now))
            .await
            .unwrap();
        s.save_turn_metrics(&turn(task, "sonnet", 0.4, now))
            .await
            .unwrap();
        s.save_turn_metrics(&turn(task, "sonnet", 0.3, now))
            .await
            .unwrap();

        let rows = s.cost_by_model_today().await.unwrap();
        assert_eq!(rows[0], ("opus".to_string(), 1.0));
        assert_eq!(rows[1], ("sonnet".to_string(), 0.7));
    }

    #[tokio::test]
    async fn cost_by_model_excludes_turns_before_today() {
        let s = store().await;
        let task = seed_task(&s).await;
        let yesterday = Utc::now() - Duration::days(1);
        s.save_turn_metrics(&turn(task, "opus", 5.0, yesterday))
            .await
            .unwrap();

        let rows = s.cost_by_model_today().await.unwrap();
        assert!(
            rows.is_empty(),
            "yesterday's spend must not count as today's, got {rows:?}"
        );
    }

    #[tokio::test]
    async fn daily_trend_zero_fills_and_buckets_by_day() {
        let s = store().await;
        let task = seed_task(&s).await;
        s.save_turn_metrics(&turn(task, "sonnet", 2.5, Utc::now()))
            .await
            .unwrap();

        let trend = s.daily_cost_trend(7).await.unwrap();
        assert_eq!(trend.len(), 7);
        for (_, cost) in &trend[..6] {
            assert_eq!(*cost, 0.0);
        }
        let (last_day, last_cost) = &trend[6];
        assert_eq!(
            *last_day,
            Utc::now().date_naive().format("%Y-%m-%d").to_string()
        );
        assert_eq!(*last_cost, 2.5);
    }
}
