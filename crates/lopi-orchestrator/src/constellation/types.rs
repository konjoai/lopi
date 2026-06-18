//! Public data types for constellation routing.
//!
//! These are the wire-facing structs (serialised by the REST layer) plus the
//! routing strategy enum and the dispatch error type. The router itself and
//! its private live-state live in the parent module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum routing decisions kept in memory per constellation. The `stats`
/// endpoint returns decisions from the trailing hour, but we cap the in-RAM
/// buffer regardless so a runaway dispatch loop can't OOM the process.
pub(crate) const MAX_DECISION_LOG: usize = 4_096;

/// One member of a constellation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstellationMember {
    /// Opaque agent identifier. Constellation does not interpret this — it
    /// is whatever the caller chose for their agent pool.
    pub agent_id: String,
    /// Soft preference weight for `WeightedRandom`. Ignored by other
    /// strategies. Members with `weight <= 0.0` are skipped entirely.
    #[serde(default = "default_weight")]
    pub weight: f32,
    /// Capability tags — used by `TagMatch` to filter candidates.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Hard cap on simultaneous in-flight tasks. `0` means "no cap".
    #[serde(default)]
    pub max_concurrent: u8,
}

pub(crate) const fn default_weight() -> f32 {
    1.0
}

/// Pluggable selection rule. Defaults to `RoundRobin`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RoutingStrategy {
    /// Visit members in declaration order, wrapping at the end.
    #[default]
    RoundRobin,
    /// Sample a member proportional to its `weight`.
    WeightedRandom,
    /// Pick the member with the fewest in-flight tasks. Ties broken by
    /// declaration order.
    LeastLoaded,
    /// Restrict candidates to members whose `tags` contain every name in
    /// `required_tags`, then break ties by least-loaded.
    TagMatch {
        /// Tags that every eligible member must have.
        required_tags: Vec<String>,
    },
    /// Sprint T — pick the member with the highest learned Q-value for this
    /// constellation, exploring epsilon-greedily. Rewards are fed back via
    /// [`crate::ConstellationRouter::record_outcome`].
    QLearned,
}

/// A named group with a routing rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constellation {
    /// Unique name identifying this constellation.
    pub name: String,
    /// Ordered list of member agents.
    pub agents: Vec<ConstellationMember>,
    #[serde(default)]
    /// Routing strategy applied when dispatching to this constellation.
    pub routing_strategy: RoutingStrategy,
    #[serde(default = "Utc::now")]
    /// When this constellation was created.
    pub created_at: DateTime<Utc>,
}

/// Outcome of a single dispatch call.
#[derive(Debug, Clone, Serialize)]
pub struct DispatchDecision {
    /// ID of the agent selected.
    pub agent_id: String,
    /// Human-readable name of the strategy that made this decision.
    pub strategy: &'static str,
    /// When the dispatch occurred.
    pub at: DateTime<Utc>,
    /// `required_tags` from a per-dispatch override (or the strategy's
    /// own list when `TagMatch`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_tags: Vec<String>,
}

/// Per-member load snapshot returned by `/stats`.
#[derive(Debug, Clone, Serialize)]
pub struct MemberLoad {
    /// Stable agent identifier.
    pub agent_id: String,
    /// Requests currently in flight for this member.
    pub in_flight: u64,
    /// Cumulative dispatches to this member since boot.
    pub dispatched_total: u64,
    /// Configured concurrency cap (0 = unlimited).
    pub max_concurrent: u8,
}

/// `/constellation/:name/stats` payload.
#[derive(Debug, Clone, Serialize)]
pub struct ConstellationStats {
    /// Name of the constellation this snapshot covers.
    pub name: String,
    /// Load snapshot for each member.
    pub members: Vec<MemberLoad>,
    /// Decisions made in the last hour, newest first.
    pub recent_decisions: Vec<DispatchDecision>,
}

/// What can go wrong on dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoutingError {
    /// No constellation with the given name is registered.
    #[error("unknown constellation: {0}")]
    UnknownConstellation(String),
    /// All members are saturated or filtered out by tag/weight constraints.
    #[error("no constellation members eligible to handle this task")]
    NoEligibleMember,
    /// The constellation exists but has no members configured.
    #[error("constellation `{0}` is empty")]
    Empty(String),
}
