use crate::error::ContextError;
use crate::stats::{EvictionReason, EvictionStats};
use crate::types::{Phase, PinPolicy, TaggedMessage, TurnId};

fn can_evict(turn: &TaggedMessage) -> bool {
    !matches!(turn.pin, PinPolicy::Always) && !turn.is_conclusion
}

fn is_budget_evictable(turn: &TaggedMessage) -> bool {
    matches!(turn.pin, PinPolicy::BudgetEvictable | PinPolicy::Never) && !turn.is_conclusion
}

fn partner_index(turns: &[TaggedMessage], turn_id: TurnId, pair_id: uuid::Uuid) -> Option<usize> {
    turns
        .iter()
        .position(|t| t.id != turn_id && t.tool_pair_id == Some(pair_id))
}

/// Evict all non-pinned, non-conclusion turns in `phase`. Tool pairs spanning phases are skipped.
///
/// # Errors
/// Returns `Err` if an internal consistency check fails.
pub fn evict_phase(
    turns: &mut Vec<TaggedMessage>,
    phase: Phase,
    current_tokens: &mut usize,
) -> Result<EvictionStats, ContextError> {
    let mut to_remove: Vec<usize> = Vec::new();
    let mut tokens_freed = 0usize;

    for (i, turn) in turns.iter().enumerate() {
        if turn.phase != phase || !can_evict(turn) {
            continue;
        }
        if to_remove.contains(&i) {
            continue;
        }

        if let Some(pair_id) = turn.tool_pair_id {
            if let Some(pidx) = partner_index(turns, turn.id, pair_id) {
                let partner = &turns[pidx];
                // Skip if partner is in a different phase — evicting would orphan it.
                if partner.phase != phase || !can_evict(partner) {
                    continue;
                }
                if !to_remove.contains(&pidx) {
                    to_remove.push(pidx);
                    tokens_freed += partner.tokens;
                }
            }
        }

        to_remove.push(i);
        tokens_freed += turn.tokens;
    }

    to_remove.sort_unstable();
    to_remove.dedup();
    let turns_evicted = to_remove.len();

    for idx in to_remove.iter().rev() {
        tracing::debug!(
            turn_id = %turns[*idx].id,
            phase = ?turns[*idx].phase,
            tokens = turns[*idx].tokens,
            "phase-evicting turn"
        );
        turns.remove(*idx);
    }

    *current_tokens = current_tokens.saturating_sub(tokens_freed);

    Ok(EvictionStats {
        turns_evicted,
        tokens_freed,
        reason: EvictionReason::PhaseTransition(phase),
    })
}

/// Evict oldest `BudgetEvictable`/`Never` turns until `current_tokens <= target_tokens`.
/// Respects `PinPolicy::Always` and `is_conclusion` — those are never touched.
///
/// # Errors
/// Returns `Err` if an internal consistency check fails.
pub fn evict_to_budget(
    turns: &mut Vec<TaggedMessage>,
    target_tokens: usize,
    current_tokens: &mut usize,
) -> Result<EvictionStats, ContextError> {
    let mut tokens_freed = 0usize;
    let mut turns_evicted = 0usize;

    let mut i = 0;
    while i < turns.len() && *current_tokens > target_tokens {
        if !is_budget_evictable(&turns[i]) {
            i += 1;
            continue;
        }

        if let Some(pair_id) = turns[i].tool_pair_id {
            let pidx = partner_index(turns, turns[i].id, pair_id);
            if let Some(pidx) = pidx {
                if !is_budget_evictable(&turns[pidx]) {
                    // Partner is not evictable — skip entire pair.
                    i += 1;
                    continue;
                }
                let t_tokens = turns[i].tokens;
                let p_tokens = turns[pidx].tokens;
                let (lo, hi) = if i < pidx { (i, pidx) } else { (pidx, i) };
                tracing::debug!(lo, hi, "budget-evicting tool pair");
                turns.remove(hi);
                turns.remove(lo);
                tokens_freed += t_tokens + p_tokens;
                turns_evicted += 2;
                *current_tokens = current_tokens.saturating_sub(t_tokens + p_tokens);
                // i removed — stay at same index.
                continue;
            }
        }

        let t_tokens = turns[i].tokens;
        tracing::debug!(turn_id = %turns[i].id, tokens = t_tokens, "budget-evicting turn");
        turns.remove(i);
        tokens_freed += t_tokens;
        turns_evicted += 1;
        *current_tokens = current_tokens.saturating_sub(t_tokens);
    }

    Ok(EvictionStats {
        turns_evicted,
        tokens_freed,
        reason: EvictionReason::BudgetLIFO,
    })
}

/// Evict a specific turn by ID.
///
/// If the turn is part of a tool pair, `force=false` returns `OrphanedToolPair`.
/// `force=true` evicts both turns in the pair unconditionally.
/// Non-evictable pins (`Always` / `is_conclusion`) require `force=true` or return `ForcedPinViolation`.
///
/// # Errors
/// Returns `Err(ContextError::TurnNotFound)` if the ID does not exist,
/// `Err(ContextError::ForcedPinViolation)` if the turn is pinned and `force=false`,
/// or `Err(ContextError::OrphanedToolPair)` if the turn has a tool-pair partner and `force=false`.
pub fn evict_turn(
    turns: &mut Vec<TaggedMessage>,
    id: TurnId,
    force: bool,
    current_tokens: &mut usize,
) -> Result<EvictionStats, ContextError> {
    let idx = turns
        .iter()
        .position(|t| t.id == id)
        .ok_or(ContextError::TurnNotFound { id })?;

    if !force && !can_evict(&turns[idx]) {
        return Err(ContextError::ForcedPinViolation { id });
    }

    let pair_id = turns[idx].tool_pair_id;
    let pidx = pair_id.and_then(|pid| partner_index(turns, id, pid));

    if let Some(pidx) = pidx {
        if !force {
            return Err(ContextError::OrphanedToolPair {
                id,
                partner_id: turns[pidx].id,
            });
        }
        let t_tokens = turns[idx].tokens;
        let p_tokens = turns[pidx].tokens;
        let (lo, hi) = if idx < pidx { (idx, pidx) } else { (pidx, idx) };
        turns.remove(hi);
        turns.remove(lo);
        *current_tokens = current_tokens.saturating_sub(t_tokens + p_tokens);
        return Ok(EvictionStats {
            turns_evicted: 2,
            tokens_freed: t_tokens + p_tokens,
            reason: EvictionReason::Manual,
        });
    }

    let tokens = turns[idx].tokens;
    turns.remove(idx);
    *current_tokens = current_tokens.saturating_sub(tokens);

    Ok(EvictionStats {
        turns_evicted: 1,
        tokens_freed: tokens,
        reason: EvictionReason::Manual,
    })
}

/// Evict any turn whose `evict_after` matches `just_completed`, skipping conclusions.
pub fn check_expired_tags(
    turns: &mut Vec<TaggedMessage>,
    just_completed: TurnId,
    current_tokens: &mut usize,
) -> EvictionStats {
    let to_remove: Vec<usize> = turns
        .iter()
        .enumerate()
        .filter(|(_, t)| t.evict_after == Some(just_completed) && !t.is_conclusion)
        .map(|(i, _)| i)
        .collect();

    let turns_evicted = to_remove.len();
    let mut tokens_freed = 0usize;

    for idx in to_remove.iter().rev() {
        tokens_freed += turns[*idx].tokens;
        turns.remove(*idx);
    }

    *current_tokens = current_tokens.saturating_sub(tokens_freed);

    EvictionStats {
        turns_evicted,
        tokens_freed,
        reason: EvictionReason::ExplicitTag,
    }
}
