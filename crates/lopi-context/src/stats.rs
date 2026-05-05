use crate::types::{Phase, TurnId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStats {
    pub total_turns: usize,
    pub active_turns: usize,
    pub evicted_turns: usize,
    pub total_tokens: usize,
    pub active_tokens: usize,
    pub token_pressure: f32,
    pub tokens_by_phase: HashMap<Phase, usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EvictionReason {
    PhaseTransition(Phase),
    BudgetLIFO,
    ExplicitTag,
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EvictionStats {
    pub turns_evicted: usize,
    pub tokens_freed: usize,
    pub reason: EvictionReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EvictionRecord {
    pub turn_id: TurnId,
    pub phase: Phase,
    pub tokens: usize,
    pub reason: EvictionReason,
    pub evicted_at_unix: u64,
}
