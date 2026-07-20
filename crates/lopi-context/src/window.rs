use crate::error::ContextError;
use crate::eviction;
use crate::stats::{ContextStats, EvictionRecord, EvictionStats};
use crate::tokens::estimate_tokens;
use crate::types::{ContentBlock, Phase, PinPolicy, Role, TaggedMessage, ToolPairId, TurnId};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Returns the natural predecessor phase to evict on transition.
fn phase_predecessor(new_phase: Phase) -> Option<Phase> {
    match new_phase {
        Phase::Planning => Some(Phase::Discovery),
        Phase::Implementation => Some(Phase::Planning),
        _ => None,
    }
}

/// Sliding token-budget window that holds tagged conversation turns for an agent run.
pub struct ContextWindow {
    turns: Vec<TaggedMessage>,
    token_budget: usize,
    current_tokens: usize,
    /// Auto-evict `BudgetEvictable` turns when pressure exceeds this ratio.
    budget_threshold: f32,
    eviction_log: Vec<EvictionRecord>,
    total_evicted: usize,
    total_token_evictions: usize,
}

impl ContextWindow {
    /// Create a new window with the given token budget.
    #[must_use]
    pub fn new(budget: usize) -> Self {
        Self {
            turns: Vec::new(),
            token_budget: budget,
            current_tokens: 0,
            budget_threshold: 0.75,
            eviction_log: Vec::new(),
            total_evicted: 0,
            total_token_evictions: 0,
        }
    }

    /// Insert a turn. Estimates tokens if `msg.tokens == 0`.
    ///
    /// Auto-evicts `BudgetEvictable` turns when pressure exceeds the threshold.
    ///
    /// # Errors
    /// Returns `Err(ContextError::Full)` if the turn cannot fit even after eviction.
    pub fn push(&mut self, mut msg: TaggedMessage) -> Result<TurnId, ContextError> {
        if msg.tokens == 0 {
            msg.tokens = estimate_tokens(&msg.content);
        }
        let msg_tokens = msg.tokens;
        let msg_id = msg.id;

        // Include this message's own weight in the pressure check — matches
        // push_tool_pair()'s calculation. Using token_pressure() (current
        // state only) let a message that would itself push pressure over
        // the threshold slip in without triggering auto-eviction first.
        // usize→f32 precision loss is acceptable: token counts are rough budget estimates.
        #[allow(clippy::cast_precision_loss)]
        let pressure = (self.current_tokens + msg_tokens) as f32 / self.token_budget as f32;
        if self.token_budget > 0 && pressure > self.budget_threshold {
            self.evict_toward_threshold();
        }

        if self.token_budget > 0 && self.current_tokens + msg_tokens > self.token_budget {
            return Err(ContextError::Full {
                budget: self.token_budget,
                needed: self.current_tokens + msg_tokens,
                after_eviction: self.current_tokens,
            });
        }

        self.current_tokens += msg_tokens;
        self.turns.push(msg);

        let expired =
            eviction::check_expired_tags(&mut self.turns, msg_id, &mut self.current_tokens);
        if expired.turns_evicted > 0 {
            self.record(expired);
        }

        Ok(msg_id)
    }

    /// Insert a `tool_use`/`tool_result` pair atomically.
    ///
    /// Returns `(call_id, result_id)` on success.
    ///
    /// # Errors
    /// Returns `Err(ContextError::Full)` if the pair cannot fit even after eviction.
    pub fn push_tool_pair(
        &mut self,
        mut call: TaggedMessage,
        mut result: TaggedMessage,
    ) -> Result<(TurnId, TurnId), ContextError> {
        let pair_id: ToolPairId = Uuid::new_v4();
        call.tool_pair_id = Some(pair_id);
        result.tool_pair_id = Some(pair_id);

        if call.tokens == 0 {
            call.tokens = estimate_tokens(&call.content);
        }
        if result.tokens == 0 {
            result.tokens = estimate_tokens(&result.content);
        }

        let call_id = call.id;
        let result_id = result.id;
        let combined = call.tokens + result.tokens;

        // Auto-evict for combined budget check.
        // usize→f32 precision loss is acceptable: token counts are rough budget estimates.
        #[allow(clippy::cast_precision_loss)]
        let pressure = (self.current_tokens + combined) as f32 / self.token_budget as f32;
        if self.token_budget > 0 && pressure > self.budget_threshold {
            self.evict_toward_threshold();
        }

        if self.token_budget > 0 && self.current_tokens + combined > self.token_budget {
            return Err(ContextError::Full {
                budget: self.token_budget,
                needed: self.current_tokens + combined,
                after_eviction: self.current_tokens,
            });
        }

        self.current_tokens += combined;
        self.turns.push(call);
        self.turns.push(result);

        let exp1 = eviction::check_expired_tags(&mut self.turns, call_id, &mut self.current_tokens);
        if exp1.turns_evicted > 0 {
            self.record(exp1);
        }
        let exp2 =
            eviction::check_expired_tags(&mut self.turns, result_id, &mut self.current_tokens);
        if exp2.turns_evicted > 0 {
            self.record(exp2);
        }

        Ok((call_id, result_id))
    }

    /// Transition to a new phase.
    ///
    /// Evicts `UntilPhase(new_phase)` turns and the natural predecessor phase's non-pinned turns.
    pub fn transition_phase(&mut self, new_phase: Phase) {
        tracing::info!(
            phase = ?new_phase,
            pressure = self.token_pressure(),
            "phase transition"
        );

        let until_ids: Vec<TurnId> = self
            .turns
            .iter()
            .filter(|t| {
                matches!(t.pin, PinPolicy::UntilPhase(p) if p == new_phase) && !t.is_conclusion
            })
            .map(|t| t.id)
            .collect();

        for id in until_ids {
            if let Ok(stats) =
                eviction::evict_turn(&mut self.turns, id, true, &mut self.current_tokens)
            {
                self.record(stats);
            }
        }

        if let Some(pred) = phase_predecessor(new_phase) {
            if let Ok(stats) =
                eviction::evict_phase(&mut self.turns, pred, &mut self.current_tokens)
            {
                if stats.turns_evicted > 0 {
                    self.record(stats);
                }
            }
        }
    }

    /// Insert a conclusion turn pinned `Always` with `is_conclusion=true`. Never auto-evicted.
    pub fn pin_conclusion(&mut self, summary: String, phase: Phase) -> TurnId {
        let id = Uuid::new_v4();
        let content = vec![ContentBlock::Text(summary)];
        let tokens = estimate_tokens(&content);
        self.current_tokens += tokens;
        self.turns.push(TaggedMessage {
            id,
            role: Role::Assistant,
            content,
            tokens,
            pin: PinPolicy::Always,
            phase,
            evict_after: None,
            tool_pair_id: None,
            is_conclusion: true,
        });
        id
    }

    /// Evict all turns in the given phase.
    ///
    /// # Errors
    /// Returns `Err` if the eviction logic encounters an inconsistency.
    pub fn evict_phase(&mut self, phase: Phase) -> Result<EvictionStats, ContextError> {
        let stats = eviction::evict_phase(&mut self.turns, phase, &mut self.current_tokens)?;
        self.record(stats.clone());
        Ok(stats)
    }

    /// Evict turns until the token count falls below `target`.
    ///
    /// # Errors
    /// Returns `Err` if the eviction logic encounters an inconsistency.
    pub fn evict_to_budget(&mut self, target: usize) -> Result<EvictionStats, ContextError> {
        let stats = eviction::evict_to_budget(&mut self.turns, target, &mut self.current_tokens)?;
        self.record(stats.clone());
        Ok(stats)
    }

    /// Evict a specific turn by ID. If `force` is true, ignores pin policies.
    ///
    /// # Errors
    /// Returns `Err` if the turn cannot be evicted (e.g., pinned and `force` is false).
    pub fn evict_turn(&mut self, id: TurnId, force: bool) -> Result<EvictionStats, ContextError> {
        let stats = eviction::evict_turn(&mut self.turns, id, force, &mut self.current_tokens)?;
        self.record(stats.clone());
        Ok(stats)
    }

    /// Returns turns in insertion order, excluding evicted turns.
    #[must_use]
    pub fn to_api_messages(&self) -> Vec<ApiMessage> {
        self.turns
            .iter()
            .map(|t| ApiMessage {
                role: t.role,
                content: t.content.clone(),
            })
            .collect()
    }

    /// Current token pressure as a ratio in [0.0, 1.0+].
    #[must_use]
    pub fn token_pressure(&self) -> f32 {
        if self.token_budget == 0 {
            return 0.0;
        }
        // usize→f32 precision loss is acceptable: pressure is a rough ratio.
        #[allow(clippy::cast_precision_loss)]
        let pressure = self.current_tokens as f32 / self.token_budget as f32;
        pressure
    }

    /// Return aggregate statistics for this window.
    #[must_use]
    pub fn stats(&self) -> ContextStats {
        let tokens_by_phase = self.turns.iter().fold(HashMap::new(), |mut map, t| {
            *map.entry(t.phase).or_insert(0) += t.tokens;
            map
        });
        ContextStats {
            total_turns: self.turns.len() + self.total_evicted,
            active_turns: self.turns.len(),
            evicted_turns: self.total_evicted,
            total_tokens: self.current_tokens + self.total_token_evictions,
            active_tokens: self.current_tokens,
            token_pressure: self.token_pressure(),
            tokens_by_phase,
        }
    }

    /// Return all active turns in insertion order.
    #[must_use]
    pub fn turns(&self) -> &[TaggedMessage] {
        &self.turns
    }

    /// Return the eviction log for this window.
    #[must_use]
    pub fn eviction_log(&self) -> &[EvictionRecord] {
        &self.eviction_log
    }

    /// Evict turns until usage drops back to `budget_threshold - 0.1` — the
    /// shared auto-evict-on-pressure step for both `push` and
    /// `push_tool_pair`.
    fn evict_toward_threshold(&mut self) {
        // usize→f32 precision loss is acceptable: token counts are rough budget estimates.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let target = (self.token_budget as f32 * (self.budget_threshold - 0.1)) as usize;
        if let Ok(stats) =
            eviction::evict_to_budget(&mut self.turns, target, &mut self.current_tokens)
        {
            self.record(stats);
        }
    }

    fn record(&mut self, stats: EvictionStats) {
        self.total_evicted += stats.turns_evicted;
        self.total_token_evictions += stats.tokens_freed;
        tracing::debug!(
            turns_evicted = stats.turns_evicted,
            tokens_freed = stats.tokens_freed,
            reason = ?stats.reason,
            "eviction"
        );
        let evicted_at_unix = now_unix();
        let reason = stats.reason;
        for (turn_id, phase, tokens) in stats.evicted {
            self.eviction_log.push(EvictionRecord {
                turn_id,
                phase,
                tokens,
                reason,
                evicted_at_unix,
            });
        }
    }
}

/// Anthropic wire-format message produced by `to_api_messages()`.
#[derive(Debug, Clone)]
pub struct ApiMessage {
    /// The role of the message sender.
    pub role: Role,
    /// The content blocks making up this message.
    pub content: Vec<ContentBlock>,
}
