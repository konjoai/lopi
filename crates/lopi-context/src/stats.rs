use crate::types::{Phase, TurnId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Snapshot of the current context window usage and eviction history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStats {
    /// Total number of turns ever inserted (including evicted).
    pub total_turns: usize,
    /// Turns currently in the window.
    pub active_turns: usize,
    /// Turns removed by any eviction policy.
    pub evicted_turns: usize,
    /// Cumulative token count of all inserted turns.
    pub total_tokens: usize,
    /// Token count of turns currently active.
    pub active_tokens: usize,
    /// Ratio of `active_tokens` to the configured budget (0.0–1.0+).
    pub token_pressure: f32,
    /// Token count grouped by agent phase.
    pub tokens_by_phase: HashMap<Phase, usize>,
}

/// Why a set of turns was evicted from the context window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EvictionReason {
    /// Evicted because the agent entered a new phase that supersedes the turns' phase.
    PhaseTransition(Phase),
    /// Evicted oldest-first (FIFO) to bring the window below the budget
    /// threshold — the standard "keep recent turns, drop stale ones"
    /// eviction order for a conversation window.
    BudgetFifo,
    /// Evicted because they carried a matching explicit eviction tag.
    ExplicitTag,
    /// Evicted by a direct caller request (force eviction).
    Manual,
}

/// Summary of a single eviction batch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EvictionStats {
    /// Number of turns removed in this batch.
    pub turns_evicted: usize,
    /// Tokens freed by removing those turns.
    pub tokens_freed: usize,
    /// Reason this batch was evicted.
    pub reason: EvictionReason,
}

/// Persistent record of a single evicted turn for audit and analytics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EvictionRecord {
    /// Identifier of the evicted turn.
    pub turn_id: TurnId,
    /// Phase the turn belonged to at eviction time.
    pub phase: Phase,
    /// Token count of the evicted turn.
    pub tokens: usize,
    /// Why the turn was evicted.
    pub reason: EvictionReason,
    /// Unix timestamp (seconds) when eviction occurred.
    pub evicted_at_unix: u64,
}
