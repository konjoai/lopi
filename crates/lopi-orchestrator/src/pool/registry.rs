//! P2 — per-agent rate limiting.
//!
//! These methods all hang off [`AgentPool`]; they are split out of `mod.rs`
//! to keep each file within the size budget. They mutate the pool's
//! `agent_rate_limits` `DashMap`, a private field reachable here because
//! this is a child module of `pool`.

use super::AgentPool;
use std::sync::atomic::Ordering;

impl AgentPool {
    /// P2 — register (or replace) per-agent rate limits. Returns `false`
    /// when the supplied limit is invalid (`max_per_minute == 0`); the
    /// REST layer translates that into 422.
    pub fn register_agent_rate_limit(
        &self,
        agent_id: impl Into<String>,
        limit: crate::AgentRateLimit,
    ) -> bool {
        if !limit.is_valid() {
            return false;
        }
        let state = crate::agent_rate_limit::AgentRateState::new(limit);
        self.agent_rate_limits.insert(agent_id.into(), state);
        true
    }

    /// Remove an agent's rate-limit entry. Returns `true` when a row was
    /// removed.
    pub fn deregister_agent_rate_limit(&self, agent_id: &str) -> bool {
        self.agent_rate_limits.remove(agent_id).is_some()
    }

    /// Snapshot the registered limit for `agent_id`, or `None` if the
    /// agent was never registered.
    #[must_use]
    pub fn agent_rate_limit(&self, agent_id: &str) -> Option<crate::AgentRateLimitSnapshot> {
        let entry = self.agent_rate_limits.get(agent_id)?;
        Some(crate::AgentRateLimitSnapshot {
            agent_id: agent_id.to_string(),
            max_per_minute: entry.limit.max_per_minute,
            max_concurrent: entry.limit.max_concurrent,
            in_flight: entry.in_flight.load(Ordering::Relaxed),
        })
    }
}
