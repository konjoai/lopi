//! P2 — capability advertisement and per-agent rate limiting.
//!
//! These methods all hang off [`AgentPool`]; they are split out of `mod.rs`
//! to keep each file within the size budget. They mutate the pool's
//! `capabilities` and `agent_rate_limits` `DashMap`s, which are private fields
//! reachable here because this is a child module of `pool`.

use super::AgentPool;
use lopi_core::Task;
use std::sync::atomic::Ordering;

impl AgentPool {
    /// P2 — advertise the capabilities of an agent slot. Tasks whose
    /// `required_capabilities` are not satisfied by *any* registered agent
    /// are rejected by [`Self::can_satisfy`] (and by callers that opt into
    /// pre-submit validation).
    ///
    /// `agent_id` is a free-form stable label — the pool itself doesn't
    /// care about its shape; it's just a key for de-duplication.
    pub fn register_capabilities(&self, agent_id: impl Into<String>, caps: Vec<String>) {
        self.capabilities.insert(agent_id.into(), caps);
    }

    /// Remove an agent's capability advertisement.
    /// Returns `true` if a row was removed.
    pub fn deregister_capabilities(&self, agent_id: &str) -> bool {
        self.capabilities.remove(agent_id).is_some()
    }

    /// Snapshot every agent's capabilities — feeds `/metrics` and the Forge
    /// fleet panel.
    #[must_use]
    pub fn capabilities_snapshot(&self) -> Vec<(String, Vec<String>)> {
        self.capabilities
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

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
    /// removed. Active in-flight counters held by the removed entry are
    /// dropped — completing tasks have no slot to decrement and just
    /// log a warning via `release_agent`.
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

    /// Try to reserve a dispatch slot for `agent_id`. Returns `true` when
    /// both gates pass (token bucket + concurrency cap), `false` when the
    /// agent is at its rate or concurrency limit.
    ///
    /// Agents that were never registered are **unlimited** and always
    /// return `true` — registration is opt-in.
    ///
    /// On success the caller MUST pair with [`Self::release_agent`] when
    /// the task completes.
    pub async fn try_acquire_agent(&self, agent_id: &str) -> bool {
        let Some(entry) = self.agent_rate_limits.get(agent_id) else {
            return true;
        };
        // Concurrency cap is checked first because it's cheap (atomic load)
        // and the token bucket lookup acquires an async lock.
        if entry.limit.max_concurrent > 0
            && entry.in_flight.load(Ordering::Relaxed) >= entry.limit.max_concurrent
        {
            return false;
        }
        if !entry.bucket.try_acquire(1.0).await {
            return false;
        }
        entry.in_flight.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Release a previously-acquired slot. Safe to call when the agent has
    /// no registry entry (e.g. it was deregistered mid-flight) — that's a
    /// noop. Underflow is impossible because the counter saturates at 0.
    pub fn release_agent(&self, agent_id: &str) {
        if let Some(entry) = self.agent_rate_limits.get(agent_id) {
            // Saturating decrement — if a runaway release call lands after
            // the slot was already returned, we just stay at 0.
            let _ = entry
                .in_flight
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                    if v == 0 {
                        None
                    } else {
                        Some(v - 1)
                    }
                });
        }
    }

    /// True when at least one registered agent advertises every capability
    /// in `task.required_capabilities`. Empty requirements vacuously pass.
    ///
    /// When the registry is *empty* (no agent has advertised anything yet)
    /// a non-empty requirement is treated as **unsatisfiable** — this
    /// closes the trap-door where a task with `required_capabilities`
    /// would otherwise silently run on whatever generic worker picks it
    /// up next.
    #[must_use]
    pub fn can_satisfy(&self, task: &Task) -> bool {
        if task.required_capabilities.is_empty() {
            return true;
        }
        if self.capabilities.is_empty() {
            return false;
        }
        self.capabilities
            .iter()
            .any(|e| task.capabilities_satisfied_by(e.value()))
    }
}
