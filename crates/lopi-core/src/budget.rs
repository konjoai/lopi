//! Budget scope — the pure data half of the cost governor.
//!
//! Tier-1 (foundation) crates may not depend on tier-2 (data) crates per
//! `.konjo/arch.toml`. This module exposes only the `BudgetScope` enum, so
//! `lopi-core::AgentEvent::BudgetExceeded` can ride a structured scope label
//! without pulling `CircuitBreaker` upward.
//!
//! The actual governor — `BudgetGovernor`, `BudgetLimit`, `BudgetConfig`,
//! `BudgetError`, `BudgetStates` — lives in `lopi-ratelimit::budget`, beside
//! the `CircuitBreaker` it wraps.

use serde::{Deserialize, Serialize};

/// One scope in the budget hierarchy.
///
/// Stable on the wire — both `AgentEvent::BudgetExceeded` and the budget
/// governor in `lopi-ratelimit` use this enum, so the JSON shape that
/// reaches the Forge UI is the same identifier the governor emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetScope {
    /// Whole `lopi sail` process — caps the total burn across all agents.
    Fleet,
    /// One running [`AgentRun`](crate::AgentRun) — caps one agent's burn.
    Agent,
    /// One task across its attempts — caps the retry budget per goal.
    Task,
}

impl BudgetScope {
    /// Stable wire string used in events, logs, and metrics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fleet => "fleet",
            Self::Agent => "agent",
            Self::Task => "task",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_as_str_is_stable() {
        // Pin the wire strings — they land in events, logs, and metrics.
        assert_eq!(BudgetScope::Fleet.as_str(), "fleet");
        assert_eq!(BudgetScope::Agent.as_str(), "agent");
        assert_eq!(BudgetScope::Task.as_str(), "task");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn scope_round_trips_through_json() {
        // Scope must survive a JSON round-trip — it rides in AgentEvent::BudgetExceeded.
        let scope = BudgetScope::Agent;
        let s = serde_json::to_string(&scope).unwrap();
        assert_eq!(s, "\"agent\"");
        let back: BudgetScope = serde_json::from_str(&s).unwrap();
        assert_eq!(back, BudgetScope::Agent);
    }
}
