//! Candidate filtering and strategy-based selection.
//!
//! All functions are crate-private helpers driven by
//! [`crate::ConstellationRouter::dispatch`]. They borrow the parent module's
//! live [`super::ConstellationState`] for load counters and the round-robin
//! cursor.

use super::types::{ConstellationMember, RoutingError, RoutingStrategy};
use super::ConstellationState;
use crate::q_router::QRouter;
use std::sync::atomic::Ordering;

/// True when `m` may receive a dispatch: positive weight, every required tag
/// (strategy + caller) present, and not at its concurrency cap.
pub(super) fn is_eligible(
    m: &ConstellationMember,
    strategy_required: &[String],
    extra_required: &[String],
    state: &ConstellationState,
) -> bool {
    if m.weight <= 0.0 {
        return false;
    }
    if !tags_match(m, strategy_required) || !tags_match(m, extra_required) {
        return false;
    }
    if let Some(counter) = state.load.get(&m.agent_id) {
        if m.max_concurrent > 0
            && counter.in_flight.load(Ordering::Relaxed) >= u64::from(m.max_concurrent)
        {
            return false;
        }
    }
    true
}

fn tags_match(m: &ConstellationMember, required: &[String]) -> bool {
    if required.is_empty() {
        return true;
    }
    required.iter().all(|r| m.tags.iter().any(|t| t == r))
}

/// Stable wire label for a strategy, surfaced in `DispatchDecision`.
pub(super) fn strategy_label(s: &RoutingStrategy) -> &'static str {
    match s {
        RoutingStrategy::RoundRobin => "round_robin",
        RoutingStrategy::WeightedRandom => "weighted_random",
        RoutingStrategy::LeastLoaded => "least_loaded",
        RoutingStrategy::TagMatch { .. } => "tag_match",
        RoutingStrategy::QLearned => "q_learned",
    }
}

/// Apply `strategy` to pick one member from `candidates`. `q` and `state_key`
/// back the `QLearned` strategy; they are ignored by the others.
pub(super) fn select_with_strategy<'a>(
    strategy: &RoutingStrategy,
    candidates: &[&'a ConstellationMember],
    state: &ConstellationState,
    q: &QRouter,
    state_key: &str,
) -> Result<&'a ConstellationMember, RoutingError> {
    if candidates.is_empty() {
        return Err(RoutingError::NoEligibleMember);
    }
    let chosen = match strategy {
        RoutingStrategy::RoundRobin => {
            let idx = state.rr_cursor.fetch_add(1, Ordering::Relaxed) % candidates.len();
            candidates[idx]
        }
        RoutingStrategy::WeightedRandom => weighted_pick(candidates),
        RoutingStrategy::LeastLoaded | RoutingStrategy::TagMatch { .. } => {
            least_loaded_pick(candidates, state)
        }
        RoutingStrategy::QLearned => qlearned_pick(candidates, state, q, state_key),
    };
    Ok(chosen)
}

fn weighted_pick<'a>(candidates: &[&'a ConstellationMember]) -> &'a ConstellationMember {
    let total: f32 = candidates.iter().map(|m| m.weight).sum();
    if total <= 0.0 {
        return candidates[0];
    }
    // Cheap, deterministic-enough pseudo-random: use the wall clock nanos.
    // We don't need cryptographic randomness — just a non-degenerate sampler.
    let now_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    let pick = (f32::from(u16::try_from(now_nanos % u32::from(u16::MAX)).unwrap_or(0))
        / f32::from(u16::MAX))
        * total;
    let mut acc = 0.0_f32;
    for m in candidates {
        acc += m.weight;
        if pick <= acc {
            return m;
        }
    }
    candidates[candidates.len() - 1]
}

fn least_loaded_pick<'a>(
    candidates: &[&'a ConstellationMember],
    state: &ConstellationState,
) -> &'a ConstellationMember {
    let mut best_idx = 0;
    let mut best_load = u64::MAX;
    for (i, m) in candidates.iter().enumerate() {
        let load = state
            .load
            .get(&m.agent_id)
            .map(|c| c.in_flight.load(Ordering::Relaxed))
            .unwrap_or(0);
        if load < best_load {
            best_load = load;
            best_idx = i;
        }
    }
    candidates[best_idx]
}

/// Pick the candidate with the best learned Q-value for `state_key`, exploring
/// epsilon-greedily. Falls back to least-loaded if the router returns nothing
/// (only possible when `candidates` is empty, already handled upstream).
fn qlearned_pick<'a>(
    candidates: &[&'a ConstellationMember],
    state: &ConstellationState,
    q: &QRouter,
    state_key: &str,
) -> &'a ConstellationMember {
    let ids: Vec<String> = candidates.iter().map(|m| m.agent_id.clone()).collect();
    match q.select(state_key, &ids) {
        Some(chosen) => candidates
            .iter()
            .find(|m| &m.agent_id == chosen)
            .copied()
            .unwrap_or_else(|| least_loaded_pick(candidates, state)),
        None => least_loaded_pick(candidates, state),
    }
}
