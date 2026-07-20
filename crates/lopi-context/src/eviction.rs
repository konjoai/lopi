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
    let mut evicted = Vec::with_capacity(turns_evicted);

    for idx in to_remove.iter().rev() {
        let turn = &turns[*idx];
        tracing::debug!(
            turn_id = %turn.id,
            phase = ?turn.phase,
            tokens = turn.tokens,
            "phase-evicting turn"
        );
        evicted.push((turn.id, turn.phase, turn.tokens));
        turns.remove(*idx);
    }

    *current_tokens = current_tokens.saturating_sub(tokens_freed);

    Ok(EvictionStats {
        turns_evicted,
        tokens_freed,
        reason: EvictionReason::PhaseTransition(phase),
        evicted,
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
    let mut evicted = Vec::new();

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
                evicted.push((turns[lo].id, turns[lo].phase, turns[lo].tokens));
                evicted.push((turns[hi].id, turns[hi].phase, turns[hi].tokens));
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
        evicted.push((turns[i].id, turns[i].phase, turns[i].tokens));
        turns.remove(i);
        tokens_freed += t_tokens;
        turns_evicted += 1;
        *current_tokens = current_tokens.saturating_sub(t_tokens);
    }

    Ok(EvictionStats {
        turns_evicted,
        tokens_freed,
        reason: EvictionReason::BudgetFifo,
        evicted,
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
        let evicted = vec![
            (turns[lo].id, turns[lo].phase, turns[lo].tokens),
            (turns[hi].id, turns[hi].phase, turns[hi].tokens),
        ];
        turns.remove(hi);
        turns.remove(lo);
        *current_tokens = current_tokens.saturating_sub(t_tokens + p_tokens);
        return Ok(EvictionStats {
            turns_evicted: 2,
            tokens_freed: t_tokens + p_tokens,
            reason: EvictionReason::Manual,
            evicted,
        });
    }

    let tokens = turns[idx].tokens;
    let evicted = vec![(turns[idx].id, turns[idx].phase, turns[idx].tokens)];
    turns.remove(idx);
    *current_tokens = current_tokens.saturating_sub(tokens);

    Ok(EvictionStats {
        turns_evicted: 1,
        tokens_freed: tokens,
        reason: EvictionReason::Manual,
        evicted,
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
    let mut evicted = Vec::with_capacity(turns_evicted);

    for idx in to_remove.iter().rev() {
        let turn = &turns[*idx];
        tokens_freed += turn.tokens;
        evicted.push((turn.id, turn.phase, turn.tokens));
        turns.remove(*idx);
    }

    *current_tokens = current_tokens.saturating_sub(tokens_freed);

    EvictionStats {
        turns_evicted,
        tokens_freed,
        reason: EvictionReason::ExplicitTag,
        evicted,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_turn(tokens: usize, pin: PinPolicy, phase: Phase) -> TaggedMessage {
        TaggedMessage {
            id: Uuid::new_v4(),
            role: crate::types::Role::User,
            content: vec![],
            tokens,
            pin,
            phase,
            evict_after: None,
            tool_pair_id: None,
            is_conclusion: false,
        }
    }

    fn make_conclusion(tokens: usize, phase: Phase) -> TaggedMessage {
        let mut t = make_turn(tokens, PinPolicy::Always, phase);
        t.is_conclusion = true;
        t
    }

    #[test]
    fn evict_phase_removes_evictable_turns() {
        let mut turns = vec![
            make_turn(100, PinPolicy::Never, Phase::Planning),
            make_turn(200, PinPolicy::Never, Phase::Planning),
        ];
        let mut tokens = 300;
        let stats = evict_phase(&mut turns, Phase::Planning, &mut tokens).unwrap();
        assert_eq!(turns.len(), 0);
        assert_eq!(stats.turns_evicted, 2);
        assert_eq!(stats.tokens_freed, 300);
        assert_eq!(tokens, 0);
    }

    #[test]
    fn evict_phase_skips_always_pinned() {
        let mut turns = vec![
            make_turn(100, PinPolicy::Always, Phase::Planning),
            make_turn(200, PinPolicy::Never, Phase::Planning),
        ];
        let mut tokens = 300;
        let stats = evict_phase(&mut turns, Phase::Planning, &mut tokens).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(stats.turns_evicted, 1);
        assert_eq!(stats.tokens_freed, 200);
    }

    #[test]
    fn evict_phase_skips_conclusions() {
        let mut turns = vec![
            make_conclusion(150, Phase::Planning),
            make_turn(200, PinPolicy::Never, Phase::Planning),
        ];
        let mut tokens = 350;
        let stats = evict_phase(&mut turns, Phase::Planning, &mut tokens).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(stats.turns_evicted, 1);
        assert_eq!(stats.tokens_freed, 200);
    }

    #[test]
    fn evict_phase_skips_different_phase() {
        let mut turns = vec![
            make_turn(100, PinPolicy::Never, Phase::Boot),
            make_turn(200, PinPolicy::Never, Phase::Planning),
        ];
        let mut tokens = 300;
        let stats = evict_phase(&mut turns, Phase::Planning, &mut tokens).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].phase, Phase::Boot);
        assert_eq!(stats.turns_evicted, 1);
        assert_eq!(stats.tokens_freed, 200);
    }

    #[test]
    fn evict_to_budget_removes_budget_evictable_turns() {
        let mut turns = vec![
            make_turn(100, PinPolicy::BudgetEvictable, Phase::Planning),
            make_turn(200, PinPolicy::BudgetEvictable, Phase::Planning),
            make_turn(50, PinPolicy::Always, Phase::Planning),
        ];
        let mut tokens = 350;
        // Target: 100 tokens — should evict first BudgetEvictable turns
        let stats = evict_to_budget(&mut turns, 100, &mut tokens).unwrap();
        assert!(tokens <= 350);
        assert!(stats.turns_evicted > 0);
    }

    #[test]
    fn evict_to_budget_respects_target() {
        let mut turns = vec![
            make_turn(100, PinPolicy::BudgetEvictable, Phase::Planning),
            make_turn(100, PinPolicy::BudgetEvictable, Phase::Planning),
        ];
        let mut tokens = 200;
        let _stats = evict_to_budget(&mut turns, 200, &mut tokens).unwrap();
        // Already at budget, nothing should be evicted
        assert_eq!(tokens, 200);
        assert_eq!(turns.len(), 2);
    }

    #[test]
    fn evict_to_budget_skips_pinned_always() {
        let mut turns = vec![
            make_turn(100, PinPolicy::Always, Phase::Planning),
            make_turn(100, PinPolicy::BudgetEvictable, Phase::Planning),
        ];
        let mut tokens = 200;
        let stats = evict_to_budget(&mut turns, 50, &mut tokens).unwrap();
        // Only the BudgetEvictable one can be removed
        assert_eq!(stats.turns_evicted, 1);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].pin, PinPolicy::Always);
    }

    #[test]
    fn evict_turn_removes_specific_turn() {
        let turn = make_turn(100, PinPolicy::Never, Phase::Planning);
        let turn_id = turn.id;
        let mut turns = vec![turn, make_turn(200, PinPolicy::Never, Phase::Planning)];
        let mut tokens = 300;
        let stats = evict_turn(&mut turns, turn_id, false, &mut tokens).unwrap();
        assert_eq!(stats.turns_evicted, 1);
        assert_eq!(stats.tokens_freed, 100);
        assert_eq!(tokens, 200);
        assert_eq!(turns.len(), 1);
    }

    #[test]
    fn evict_turn_returns_error_for_nonexistent_id() {
        let mut turns = vec![make_turn(100, PinPolicy::Never, Phase::Planning)];
        let mut tokens = 100;
        let fake_id = Uuid::new_v4();
        let result = evict_turn(&mut turns, fake_id, false, &mut tokens);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ContextError::TurnNotFound { .. }
        ));
    }

    #[test]
    fn evict_turn_returns_error_for_pinned_always_without_force() {
        let turn = make_turn(100, PinPolicy::Always, Phase::Planning);
        let turn_id = turn.id;
        let mut turns = vec![turn];
        let mut tokens = 100;
        let result = evict_turn(&mut turns, turn_id, false, &mut tokens);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ContextError::ForcedPinViolation { .. }
        ));
    }

    #[test]
    fn evict_turn_with_force_removes_pinned() {
        let turn = make_turn(100, PinPolicy::Always, Phase::Planning);
        let turn_id = turn.id;
        let mut turns = vec![turn];
        let mut tokens = 100;
        let stats = evict_turn(&mut turns, turn_id, true, &mut tokens).unwrap();
        assert_eq!(stats.turns_evicted, 1);
        assert!(turns.is_empty());
        assert_eq!(tokens, 0);
    }

    #[test]
    fn check_expired_tags_removes_matching_turns() {
        let trigger_id = Uuid::new_v4();
        let mut turn1 = make_turn(100, PinPolicy::Never, Phase::Planning);
        turn1.evict_after = Some(trigger_id);
        let turn2 = make_turn(200, PinPolicy::Never, Phase::Planning);
        let mut window = vec![turn1, turn2];
        let mut tokens = 300;

        let stats = check_expired_tags(&mut window, trigger_id, &mut tokens);
        assert_eq!(stats.turns_evicted, 1);
        assert_eq!(stats.tokens_freed, 100);
        assert_eq!(tokens, 200);
        assert_eq!(window.len(), 1);
    }

    #[test]
    fn check_expired_tags_skips_conclusions() {
        let trigger_id = Uuid::new_v4();
        let mut turn = make_turn(100, PinPolicy::Never, Phase::Planning);
        turn.evict_after = Some(trigger_id);
        turn.is_conclusion = true;
        let mut turns = vec![turn];
        let mut tokens = 100;

        let stats = check_expired_tags(&mut turns, trigger_id, &mut tokens);
        assert_eq!(stats.turns_evicted, 0);
        assert_eq!(turns.len(), 1);
    }

    #[test]
    fn check_expired_tags_no_match_does_nothing() {
        let trigger_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();
        let mut turn = make_turn(100, PinPolicy::Never, Phase::Planning);
        turn.evict_after = Some(other_id);
        let mut turns = vec![turn];
        let mut tokens = 100;

        let stats = check_expired_tags(&mut turns, trigger_id, &mut tokens);
        assert_eq!(stats.turns_evicted, 0);
        assert_eq!(turns.len(), 1);
        assert_eq!(tokens, 100);
    }

    #[test]
    fn evict_phase_empty_turns_returns_zero_stats() {
        let mut turns: Vec<TaggedMessage> = vec![];
        let mut tokens = 0;
        let stats = evict_phase(&mut turns, Phase::Planning, &mut tokens).unwrap();
        assert_eq!(stats.turns_evicted, 0);
        assert_eq!(stats.tokens_freed, 0);
    }
}
