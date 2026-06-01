//! Per-agent rate limiting — bolted on to `AgentPool`.
//!
//! Each registered agent gets a token bucket sized by `max_per_minute`
//! plus an atomic in-flight counter capped by `max_concurrent`. Callers
//! gate dispatch (non-blocking) and release when the task finishes.
//!
//! Agents not in the registry are *unlimited* — registration is opt-in.
//! REST handlers translate "registry hit, acquire fails" into HTTP 429.

use lopi_ratelimit::TokenBucket;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

/// Operator-supplied limits — both fields are required.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AgentRateLimit {
    /// Maximum task dispatches per rolling minute. Refills at
    /// `max_per_minute / 60` tokens per second.
    pub max_per_minute: u32,
    /// Maximum concurrent in-flight tasks for this agent. 0 → no
    /// concurrency cap (only the per-minute budget applies).
    pub max_concurrent: u32,
}

impl AgentRateLimit {
    /// Sanity-check the inputs. Empty per-minute is the obvious mistake
    /// to catch — silently allowing zero-throughput agents is a footgun.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.max_per_minute > 0
    }
}

/// Internal entry — one per registered agent.
#[derive(Clone)]
pub(crate) struct AgentRateState {
    pub limit: AgentRateLimit,
    pub bucket: TokenBucket,
    pub in_flight: Arc<AtomicU32>,
}

impl AgentRateState {
    pub(crate) fn new(limit: AgentRateLimit) -> Self {
        let capacity = f64::from(limit.max_per_minute);
        let refill = capacity / 60.0;
        Self {
            limit,
            bucket: TokenBucket::new(capacity, refill),
            in_flight: Arc::new(AtomicU32::new(0)),
        }
    }
}

/// Public snapshot returned by `GET /api/agents/:id/rate-limit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRateLimitSnapshot {
    /// Stable identifier for the agent this snapshot describes.
    pub agent_id: String,
    /// Maximum token requests allowed per minute.
    pub max_per_minute: u32,
    /// Maximum simultaneous requests allowed.
    pub max_concurrent: u32,
    /// Requests currently in flight.
    pub in_flight: u32,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_validates_max_per_minute() {
        assert!(AgentRateLimit { max_per_minute: 60, max_concurrent: 4 }.is_valid());
        assert!(!AgentRateLimit { max_per_minute: 0, max_concurrent: 4 }.is_valid());
    }

    #[tokio::test]
    async fn bucket_capacity_matches_max_per_minute() {
        let state = AgentRateState::new(AgentRateLimit {
            max_per_minute: 3,
            max_concurrent: 0,
        });
        assert!(state.bucket.try_acquire(1.0).await);
        assert!(state.bucket.try_acquire(1.0).await);
        assert!(state.bucket.try_acquire(1.0).await);
        // 4th immediate acquire fails — refill rate is 3/60 = 0.05/sec,
        // so the bucket is empty until at least 20s pass.
        assert!(!state.bucket.try_acquire(1.0).await);
    }
}
