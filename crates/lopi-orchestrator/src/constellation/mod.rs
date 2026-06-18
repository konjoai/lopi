//! P2 — Constellation routing.
//!
//! A *constellation* is a named group of agents with a pluggable routing
//! strategy. `POST /constellation/{name}/dispatch` picks the best agent for
//! a task and returns the chosen `agent_id`; the caller (the HTTP handler)
//! is responsible for actually submitting the task to that agent.
//!
//! Live load is tracked per-member via atomic counters so `LeastLoaded` is
//! lock-free on the read path. The routing decisions log is bounded so the
//! `/stats` endpoint never reads an unbounded history.
//!
//! Sprint T adds the `QLearned` strategy: selection delegates to a shared
//! [`crate::QRouter`] keyed on the constellation name, and
//! [`ConstellationRouter::record_outcome`] feeds the task's quality reward back
//! into the Q-table.

mod select;
mod types;

use crate::q_router::QRouter;
use chrono::Utc;
use select::{is_eligible, select_with_strategy, strategy_label};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use types::MAX_DECISION_LOG;

pub use types::{
    Constellation, ConstellationMember, ConstellationStats, DispatchDecision, MemberLoad,
    RoutingError, RoutingStrategy,
};

/// In-memory router with bounded decision history. Cheap to `clone()`.
#[derive(Debug, Clone, Default)]
pub struct ConstellationRouter {
    inner: Arc<RwLock<HashMap<String, ConstellationState>>>,
    /// Shared Q-table backing the `QLearned` strategy. Keyed on
    /// `(constellation_name, agent_id)`.
    q: Arc<QRouter>,
}

#[derive(Debug)]
pub(crate) struct ConstellationState {
    /// Declarative spec — never mutated after registration.
    spec: Constellation,
    /// Round-robin cursor.
    pub(crate) rr_cursor: AtomicUsize,
    /// agent_id → live counters.
    pub(crate) load: HashMap<String, MemberCounter>,
    /// Bounded ring buffer of recent decisions, oldest first.
    decisions: parking_lot_friendly::DecisionBuffer,
}

#[derive(Debug, Default)]
pub(crate) struct MemberCounter {
    pub(crate) in_flight: AtomicU64,
    dispatched_total: AtomicU64,
}

mod parking_lot_friendly {
    use super::DispatchDecision;
    use super::MAX_DECISION_LOG;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Mutex-guarded ring buffer — std::sync::Mutex is fine here because
    /// the held duration is microseconds (push + drop oldest).
    #[derive(Debug)]
    pub struct DecisionBuffer {
        inner: Mutex<VecDeque<DispatchDecision>>,
    }

    impl Default for DecisionBuffer {
        fn default() -> Self {
            Self {
                inner: Mutex::new(VecDeque::with_capacity(64)),
            }
        }
    }

    impl DecisionBuffer {
        pub fn push(&self, d: DispatchDecision) {
            // Recover from poisoning — a panicked writer shouldn't drop
            // future decisions.
            let mut q = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if q.len() == MAX_DECISION_LOG {
                q.pop_front();
            }
            q.push_back(d);
        }

        pub fn snapshot_last_hour(&self) -> Vec<DispatchDecision> {
            let q = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(1);
            q.iter().rev().filter(|d| d.at >= cutoff).cloned().collect()
        }
    }
}

impl ConstellationRouter {
    /// Build a fresh router with no constellations registered.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace a constellation. Returns `true` if this replaced
    /// an existing one with the same name.
    pub async fn register(&self, c: Constellation) -> bool {
        let load = c
            .agents
            .iter()
            .map(|m| (m.agent_id.clone(), MemberCounter::default()))
            .collect();
        let state = ConstellationState {
            spec: c.clone(),
            rr_cursor: AtomicUsize::new(0),
            load,
            decisions: parking_lot_friendly::DecisionBuffer::default(),
        };
        let mut map = self.inner.write().await;
        map.insert(c.name.clone(), state).is_some()
    }

    /// List every registered constellation as a `Vec<Constellation>` —
    /// drops the live load counters since they aren't part of the spec.
    pub async fn list(&self) -> Vec<Constellation> {
        let map = self.inner.read().await;
        map.values().map(|s| s.spec.clone()).collect()
    }

    /// Pick an agent for `name`. `extra_required_tags`, if non-empty, is
    /// intersected with the strategy's own filter (for `TagMatch` they
    /// combine; for other strategies they act as a hard filter).
    ///
    /// # Errors
    /// `UnknownConstellation` / `Empty` / `NoEligibleMember` depending on
    /// what went wrong.
    pub async fn dispatch(
        &self,
        name: &str,
        extra_required_tags: &[String],
    ) -> Result<DispatchDecision, RoutingError> {
        let map = self.inner.read().await;
        let state = map
            .get(name)
            .ok_or_else(|| RoutingError::UnknownConstellation(name.to_string()))?;

        if state.spec.agents.is_empty() {
            return Err(RoutingError::Empty(name.to_string()));
        }

        // Combine strategy-level required tags (for TagMatch) with caller-
        // supplied ones — a member must satisfy every tag in the union to
        // be eligible.
        let strategy_required: &[String] = match &state.spec.routing_strategy {
            RoutingStrategy::TagMatch { required_tags } => required_tags.as_slice(),
            _ => &[],
        };

        // Filter: weight > 0, tag match (strategy + extras), max_concurrent cap.
        let candidates: Vec<&ConstellationMember> = state
            .spec
            .agents
            .iter()
            .filter(|m| is_eligible(m, strategy_required, extra_required_tags, state))
            .collect();

        let chosen = select_with_strategy(
            &state.spec.routing_strategy,
            &candidates,
            state,
            &self.q,
            name,
        )?;
        // Bump counters.
        if let Some(counter) = state.load.get(&chosen.agent_id) {
            counter.in_flight.fetch_add(1, Ordering::Relaxed);
            counter.dispatched_total.fetch_add(1, Ordering::Relaxed);
        }
        let decision = DispatchDecision {
            agent_id: chosen.agent_id.clone(),
            strategy: strategy_label(&state.spec.routing_strategy),
            at: Utc::now(),
            required_tags: dispatch_required_tags(
                &state.spec.routing_strategy,
                extra_required_tags,
            ),
        };
        state.decisions.push(decision.clone());
        Ok(decision)
    }

    /// Decrement a member's in-flight counter — call this when a task
    /// completes (success OR failure). Idempotent on unknown ids.
    pub async fn release(&self, constellation: &str, agent_id: &str) {
        let map = self.inner.read().await;
        if let Some(state) = map.get(constellation) {
            if let Some(counter) = state.load.get(agent_id) {
                // Saturating: a runaway release should not wrap around to
                // u64::MAX.
                let prev = counter.in_flight.load(Ordering::Relaxed);
                if prev > 0 {
                    counter.in_flight.fetch_sub(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Sprint T — feed a task's quality `reward` (clamped to `[0, 1]`) back
    /// into the Q-table for the `QLearned` strategy. `constellation` is the
    /// state key and `agent_id` the action. Call this when a task that was
    /// dispatched here finishes scoring; no-op for non-`QLearned` groups
    /// beyond updating the shared table.
    pub fn record_outcome(&self, constellation: &str, agent_id: &str, reward: f64) {
        self.q.update(constellation, agent_id, reward);
    }

    /// Snapshot the `QLearned` value table, for persistence or inspection.
    #[must_use]
    pub fn q_snapshot(&self) -> Vec<crate::QValueEntry> {
        self.q.snapshot()
    }

    /// Per-member load snapshot + last-hour decisions.
    pub async fn stats(&self, name: &str) -> Option<ConstellationStats> {
        let map = self.inner.read().await;
        let state = map.get(name)?;
        let members = state
            .spec
            .agents
            .iter()
            .map(|m| {
                let counter = state.load.get(&m.agent_id);
                MemberLoad {
                    agent_id: m.agent_id.clone(),
                    in_flight: counter
                        .map(|c| c.in_flight.load(Ordering::Relaxed))
                        .unwrap_or(0),
                    dispatched_total: counter
                        .map(|c| c.dispatched_total.load(Ordering::Relaxed))
                        .unwrap_or(0),
                    max_concurrent: m.max_concurrent,
                }
            })
            .collect();
        Some(ConstellationStats {
            name: state.spec.name.clone(),
            members,
            recent_decisions: state.decisions.snapshot_last_hour(),
        })
    }
}

/// Compute the `required_tags` recorded on a decision: for `TagMatch` it is the
/// deduped union of strategy + caller tags; otherwise just the caller's.
fn dispatch_required_tags(strategy: &RoutingStrategy, extra: &[String]) -> Vec<String> {
    match strategy {
        RoutingStrategy::TagMatch { required_tags } => {
            let mut combined = required_tags.clone();
            combined.extend(extra.iter().cloned());
            combined.sort();
            combined.dedup();
            combined
        }
        _ => extra.to_vec(),
    }
}

#[cfg(test)]
mod tests;
