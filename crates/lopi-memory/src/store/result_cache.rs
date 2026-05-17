//! P2 — Content-addressed result cache.
//!
//! When an agent produces a successful result for a `(agent_id, task)` pair,
//! the dispatch path stores the result keyed on `SHA-256(agent_id ‖ task_json)`.
//! Subsequent identical requests bypass agent invocation entirely and return
//! the cached value with `cache_hit: true`.
//!
//! Two tables (defined in `schema.sql`):
//! - `result_cache` — the entries themselves, with `created_at` (unix epoch
//!   seconds), `hit_count`, and `size_bytes`.
//! - `result_cache_events` — a rolling hit/miss log capped at the last hour
//!   so [`CacheStats::hit_rate_last_hour`] is a true windowed rate, not
//!   process-lifetime.
//!
//! TTL is enforced lazily on `get` — stale entries are deleted on read.
//! Callers that need eager eviction call [`MemoryStore::sweep_cache`]
//! periodically (e.g. once every 5 min from a `tokio::spawn` loop).

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row as _;

use super::MemoryStore;

/// Default TTL — 1 hour. Tunable per-call via [`MemoryStore::cache_get_with_ttl`].
pub const DEFAULT_TTL_SECS: i64 = 3_600;

/// One cached result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachedResult {
    /// The cache key — `SHA-256(agent_id ‖ task_json)` as lowercase hex.
    pub key: String,
    /// The serialized result payload. Stored as TEXT so callers can put any
    /// UTF-8; binary payloads should be base64-encoded first.
    pub value: String,
    /// Which agent produced this result. Used by
    /// [`MemoryStore::invalidate_cache_for_agent`] for targeted purge.
    pub agent_id: String,
    /// Unix epoch seconds at which the entry was inserted.
    pub created_at: i64,
    /// Number of times this entry has been served.
    pub hit_count: u64,
    /// Stored payload size in bytes.
    pub size_bytes: u64,
}

/// Aggregate over the cache. Driven by `GET /api/cache/stats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of live (non-expired) entries.
    pub total_entries: u64,
    /// Sum of `size_bytes` across live entries.
    pub total_size_bytes: u64,
    /// Hit rate over the last hour — `hits / (hits + misses)`. `0.0` when
    /// there is no traffic in the window.
    pub hit_rate_last_hour: f32,
    /// `created_at` of the oldest live entry, or `None` if the cache is empty.
    pub oldest_entry: Option<DateTime<Utc>>,
}

/// Hash an `(agent_id, task_json)` pair into the canonical cache key.
///
/// `task_json` should be the result of `serde_json::to_vec(&task)` — the
/// caller is responsible for picking a canonical serialisation if they need
/// cross-process stability (default `serde_json` preserves field order from
/// the struct definition, which is stable for `Task`).
#[must_use]
pub fn compute_key(agent_id: &str, task_json: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(agent_id.as_bytes());
    h.update([0x1F]); // unit separator — namespacing safety
    h.update(task_json);
    hex::encode(h.finalize())
}

impl MemoryStore {
    /// Look up a cached result. Returns `None` for misses *and* for entries
    /// older than `ttl_secs` (which are deleted as a side-effect). Increments
    /// `hit_count` on a real hit and logs a hit/miss into
    /// `result_cache_events` for the rolling-rate calculation.
    ///
    /// # Errors
    /// Returns `Err` on underlying SQLite errors.
    pub async fn cache_get(&self, key: &str) -> Result<Option<CachedResult>> {
        self.cache_get_with_ttl(key, DEFAULT_TTL_SECS).await
    }

    /// Variant of [`Self::cache_get`] with an explicit TTL.
    ///
    /// # Errors
    /// Returns `Err` on underlying SQLite errors.
    pub async fn cache_get_with_ttl(
        &self,
        key: &str,
        ttl_secs: i64,
    ) -> Result<Option<CachedResult>> {
        let now = Utc::now().timestamp();
        let cutoff = now - ttl_secs;
        let row_opt = sqlx::query(
            "SELECT key, value, agent_id, created_at, hit_count, size_bytes
             FROM result_cache
             WHERE key = ?",
        )
        .bind(key)
        .fetch_optional(&self.read_pool)
        .await
        .context("querying result_cache")?;

        let outcome;
        let result = if let Some(row) = row_opt {
            let created_at: i64 = row.get("created_at");
            if created_at < cutoff {
                // Stale — delete + count as miss.
                sqlx::query("DELETE FROM result_cache WHERE key = ?")
                    .bind(key)
                    .execute(&self.write_pool)
                    .await
                    .context("evicting stale cache row")?;
                outcome = "miss";
                None
            } else {
                let mut entry = row_to_cached(&row);
                sqlx::query("UPDATE result_cache SET hit_count = hit_count + 1 WHERE key = ?")
                    .bind(key)
                    .execute(&self.write_pool)
                    .await
                    .context("bumping cache hit_count")?;
                // Reflect the bump in the returned struct so callers see
                // the count *including* this call, not the pre-call value.
                entry.hit_count = entry.hit_count.saturating_add(1);
                outcome = "hit";
                Some(entry)
            }
        } else {
            outcome = "miss";
            None
        };

        self.record_cache_event(now, outcome).await?;
        Ok(result)
    }

    /// Store a result. Overwrites any existing entry with the same key.
    /// `size_bytes` is computed from `value.len()`.
    ///
    /// # Errors
    /// Returns `Err` on underlying SQLite errors.
    pub async fn cache_put(&self, key: &str, value: &str, agent_id: &str) -> Result<()> {
        let now = Utc::now().timestamp();
        let size = u32::try_from(value.len()).unwrap_or(u32::MAX);
        sqlx::query(
            "INSERT INTO result_cache (key, value, agent_id, created_at, hit_count, size_bytes)
             VALUES (?, ?, ?, ?, 0, ?)
             ON CONFLICT(key) DO UPDATE SET
                value      = excluded.value,
                agent_id   = excluded.agent_id,
                created_at = excluded.created_at,
                hit_count  = 0,
                size_bytes = excluded.size_bytes",
        )
        .bind(key)
        .bind(value)
        .bind(agent_id)
        .bind(now)
        .bind(i64::from(size))
        .execute(&self.write_pool)
        .await
        .context("inserting result_cache row")?;
        Ok(())
    }

    /// Delete every cache entry produced by `agent_id`. Returns the number
    /// of rows removed.
    ///
    /// # Errors
    /// Returns `Err` on underlying SQLite errors.
    pub async fn invalidate_cache_for_agent(&self, agent_id: &str) -> Result<u64> {
        let res = sqlx::query("DELETE FROM result_cache WHERE agent_id = ?")
            .bind(agent_id)
            .execute(&self.write_pool)
            .await
            .context("invalidating cache for agent")?;
        Ok(res.rows_affected())
    }

    /// Delete every cache entry, full stop. Returns the count removed.
    ///
    /// # Errors
    /// Returns `Err` on underlying SQLite errors.
    pub async fn clear_cache(&self) -> Result<u64> {
        let res = sqlx::query("DELETE FROM result_cache")
            .execute(&self.write_pool)
            .await
            .context("clearing result_cache")?;
        Ok(res.rows_affected())
    }

    /// Delete every entry older than `ttl_secs`. Idempotent — safe to run
    /// from a periodic sweep loop. Returns the count removed.
    ///
    /// # Errors
    /// Returns `Err` on underlying SQLite errors.
    pub async fn sweep_cache(&self, ttl_secs: i64) -> Result<u64> {
        let cutoff = Utc::now().timestamp() - ttl_secs;
        let res = sqlx::query("DELETE FROM result_cache WHERE created_at < ?")
            .bind(cutoff)
            .execute(&self.write_pool)
            .await
            .context("sweeping stale cache")?;
        // Also trim the events log to the last hour.
        let one_hour_ago = Utc::now().timestamp() - 3_600;
        let _ = sqlx::query("DELETE FROM result_cache_events WHERE ts < ?")
            .bind(one_hour_ago)
            .execute(&self.write_pool)
            .await;
        Ok(res.rows_affected())
    }

    /// Aggregate stats for `/api/cache/stats`.
    ///
    /// # Errors
    /// Returns `Err` on underlying SQLite errors.
    pub async fn cache_stats(&self) -> Result<CacheStats> {
        let totals = sqlx::query(
            "SELECT COUNT(*) AS n, COALESCE(SUM(size_bytes), 0) AS bytes,
                    MIN(created_at) AS oldest
             FROM result_cache",
        )
        .fetch_one(&self.read_pool)
        .await
        .context("reading cache totals")?;
        let n: i64 = totals.get("n");
        let bytes: i64 = totals.get("bytes");
        let oldest_ts: Option<i64> = totals.try_get("oldest").unwrap_or(None);
        let oldest_entry = oldest_ts.and_then(|t| Utc.timestamp_opt(t, 0).single());

        let one_hour_ago = Utc::now().timestamp() - 3_600;
        let events = sqlx::query(
            "SELECT
                SUM(CASE outcome WHEN 'hit'  THEN 1 ELSE 0 END) AS hits,
                SUM(CASE outcome WHEN 'miss' THEN 1 ELSE 0 END) AS misses
             FROM result_cache_events
             WHERE ts >= ?",
        )
        .bind(one_hour_ago)
        .fetch_one(&self.read_pool)
        .await
        .context("reading cache event totals")?;
        let hits: i64 = events.try_get("hits").unwrap_or(0);
        let misses: i64 = events.try_get("misses").unwrap_or(0);
        #[allow(clippy::cast_precision_loss)]
        let hit_rate = if hits + misses == 0 {
            0.0_f32
        } else {
            (hits as f32) / ((hits + misses) as f32)
        };

        Ok(CacheStats {
            total_entries: u64::try_from(n).unwrap_or(0),
            total_size_bytes: u64::try_from(bytes).unwrap_or(0),
            hit_rate_last_hour: hit_rate,
            oldest_entry,
        })
    }

    async fn record_cache_event(&self, ts: i64, outcome: &str) -> Result<()> {
        sqlx::query("INSERT INTO result_cache_events (ts, outcome) VALUES (?, ?)")
            .bind(ts)
            .bind(outcome)
            .execute(&self.write_pool)
            .await
            .context("recording cache event")?;
        Ok(())
    }
}

fn row_to_cached(row: &sqlx::sqlite::SqliteRow) -> CachedResult {
    CachedResult {
        key: row.get("key"),
        value: row.get("value"),
        agent_id: row.get("agent_id"),
        created_at: row.get("created_at"),
        hit_count: u64::try_from(row.get::<i64, _>("hit_count")).unwrap_or(0),
        size_bytes: u64::try_from(row.get::<i64, _>("size_bytes")).unwrap_or(0),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    async fn fresh_store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    #[test]
    fn compute_key_is_deterministic() {
        let a = compute_key("agent-1", br#"{"goal":"x"}"#);
        let b = compute_key("agent-1", br#"{"goal":"x"}"#);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "sha-256 hex is 64 chars");
    }

    #[test]
    fn compute_key_differs_per_agent() {
        let a = compute_key("agent-1", br#"{"goal":"x"}"#);
        let b = compute_key("agent-2", br#"{"goal":"x"}"#);
        assert_ne!(a, b);
    }

    #[test]
    fn compute_key_differs_per_task() {
        let a = compute_key("agent-1", br#"{"goal":"x"}"#);
        let b = compute_key("agent-1", br#"{"goal":"y"}"#);
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn put_then_get_returns_stored_value() {
        let store = fresh_store().await;
        let key = compute_key("agent-A", br"task-bytes");
        store
            .cache_put(&key, "the-result", "agent-A")
            .await
            .unwrap();
        let got = store.cache_get(&key).await.unwrap().unwrap();
        assert_eq!(got.value, "the-result");
        assert_eq!(got.agent_id, "agent-A");
        // hit_count incremented on the get
        assert_eq!(got.hit_count, 1);
    }

    #[tokio::test]
    async fn get_unknown_key_returns_none() {
        let store = fresh_store().await;
        let got = store.cache_get("deadbeef").await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn stale_entries_are_evicted_on_get() {
        let store = fresh_store().await;
        let key = "k1";
        // Insert with a synthetic old timestamp via cache_put then mutate.
        store.cache_put(key, "old", "agent-A").await.unwrap();
        // Force created_at to 2 hours ago.
        sqlx::query("UPDATE result_cache SET created_at = ? WHERE key = ?")
            .bind(Utc::now().timestamp() - 7_200)
            .bind(key)
            .execute(&store.write_pool)
            .await
            .unwrap();
        // TTL is 1 hour by default.
        let got = store.cache_get(key).await.unwrap();
        assert!(got.is_none(), "stale row should not be returned");
        // Row should be gone.
        let stats = store.cache_stats().await.unwrap();
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn put_overwrites_existing_key() {
        let store = fresh_store().await;
        let key = "k1";
        store.cache_put(key, "first", "agent-A").await.unwrap();
        store.cache_put(key, "second", "agent-A").await.unwrap();
        let got = store.cache_get(key).await.unwrap().unwrap();
        assert_eq!(got.value, "second");
        // Overwrite resets hit_count, then this get bumps it to 1.
        assert_eq!(got.hit_count, 1);
    }

    #[tokio::test]
    async fn invalidate_for_agent_removes_only_their_entries() {
        let store = fresh_store().await;
        store.cache_put("k-a", "v", "agent-A").await.unwrap();
        store.cache_put("k-b", "v", "agent-B").await.unwrap();
        store.cache_put("k-a2", "v", "agent-A").await.unwrap();
        let removed = store.invalidate_cache_for_agent("agent-A").await.unwrap();
        assert_eq!(removed, 2);
        assert!(store.cache_get("k-b").await.unwrap().is_some());
        assert!(store.cache_get("k-a").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn clear_drops_everything() {
        let store = fresh_store().await;
        store.cache_put("k1", "v", "agent-A").await.unwrap();
        store.cache_put("k2", "v", "agent-B").await.unwrap();
        let removed = store.clear_cache().await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(store.cache_stats().await.unwrap().total_entries, 0);
    }

    #[tokio::test]
    async fn sweep_removes_only_stale() {
        let store = fresh_store().await;
        store.cache_put("k-fresh", "v", "a").await.unwrap();
        store.cache_put("k-stale", "v", "a").await.unwrap();
        sqlx::query("UPDATE result_cache SET created_at = ? WHERE key = ?")
            .bind(Utc::now().timestamp() - 7_200)
            .bind("k-stale")
            .execute(&store.write_pool)
            .await
            .unwrap();
        let removed = store.sweep_cache(3_600).await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(store.cache_stats().await.unwrap().total_entries, 1);
    }

    #[tokio::test]
    async fn stats_track_hit_rate_in_last_hour() {
        let store = fresh_store().await;
        let key = "k-stat";
        store.cache_put(key, "v", "a").await.unwrap();
        // 1 hit + 2 misses → 33% hit rate
        let _ = store.cache_get(key).await.unwrap();
        let _ = store.cache_get("nope-1").await.unwrap();
        let _ = store.cache_get("nope-2").await.unwrap();
        let stats = store.cache_stats().await.unwrap();
        assert!(
            (stats.hit_rate_last_hour - 1.0 / 3.0).abs() < 0.01,
            "expected ~0.333 hit rate, got {}",
            stats.hit_rate_last_hour
        );
        assert_eq!(stats.total_entries, 1);
        assert!(stats.oldest_entry.is_some());
    }
}
