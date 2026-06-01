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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Maximum routing decisions kept in memory per constellation. The `stats`
/// endpoint returns decisions from the trailing hour, but we cap the in-RAM
/// buffer regardless so a runaway dispatch loop can't OOM the process.
const MAX_DECISION_LOG: usize = 4_096;

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

const fn default_weight() -> f32 {
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

/// In-memory router with bounded decision history. Cheap to `clone()`.
#[derive(Debug, Clone, Default)]
pub struct ConstellationRouter {
    inner: Arc<RwLock<HashMap<String, ConstellationState>>>,
}

#[derive(Debug)]
struct ConstellationState {
    /// Declarative spec — never mutated after registration.
    spec: Constellation,
    /// Round-robin cursor.
    rr_cursor: AtomicUsize,
    /// agent_id → live counters.
    load: HashMap<String, MemberCounter>,
    /// Bounded ring buffer of recent decisions, oldest first.
    decisions: parking_lot_friendly::DecisionBuffer,
}

#[derive(Debug, Default)]
struct MemberCounter {
    in_flight: AtomicU64,
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

        let chosen = select_with_strategy(&state.spec.routing_strategy, &candidates, state)?;
        let strategy_label = strategy_label(&state.spec.routing_strategy);
        // Bump counters.
        if let Some(counter) = state.load.get(&chosen.agent_id) {
            counter.in_flight.fetch_add(1, Ordering::Relaxed);
            counter.dispatched_total.fetch_add(1, Ordering::Relaxed);
        }
        let required_tags = match &state.spec.routing_strategy {
            RoutingStrategy::TagMatch { required_tags } => {
                let mut combined = required_tags.clone();
                combined.extend(extra_required_tags.iter().cloned());
                combined.sort();
                combined.dedup();
                combined
            }
            _ => extra_required_tags.to_vec(),
        };
        let decision = DispatchDecision {
            agent_id: chosen.agent_id.clone(),
            strategy: strategy_label,
            at: Utc::now(),
            required_tags,
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

fn is_eligible(
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

fn strategy_label(s: &RoutingStrategy) -> &'static str {
    match s {
        RoutingStrategy::RoundRobin => "round_robin",
        RoutingStrategy::WeightedRandom => "weighted_random",
        RoutingStrategy::LeastLoaded => "least_loaded",
        RoutingStrategy::TagMatch { .. } => "tag_match",
    }
}

fn select_with_strategy<'a>(
    strategy: &RoutingStrategy,
    candidates: &[&'a ConstellationMember],
    state: &ConstellationState,
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn member(id: &str, weight: f32, tags: &[&str], max_concurrent: u8) -> ConstellationMember {
        ConstellationMember {
            agent_id: id.into(),
            weight,
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            max_concurrent,
        }
    }

    fn make(
        name: &str,
        strat: RoutingStrategy,
        members: Vec<ConstellationMember>,
    ) -> Constellation {
        Constellation {
            name: name.into(),
            agents: members,
            routing_strategy: strat,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn unknown_constellation_returns_error() {
        let r = ConstellationRouter::new();
        let err = r.dispatch("does-not-exist", &[]).await.unwrap_err();
        assert!(matches!(err, RoutingError::UnknownConstellation(_)));
    }

    #[tokio::test]
    async fn empty_constellation_returns_error() {
        let r = ConstellationRouter::new();
        r.register(make("empty", RoutingStrategy::RoundRobin, vec![]))
            .await;
        let err = r.dispatch("empty", &[]).await.unwrap_err();
        assert!(matches!(err, RoutingError::Empty(_)));
    }

    #[tokio::test]
    async fn round_robin_visits_each_member_in_order() {
        let r = ConstellationRouter::new();
        r.register(make(
            "rr",
            RoutingStrategy::RoundRobin,
            vec![
                member("a", 1.0, &[], 0),
                member("b", 1.0, &[], 0),
                member("c", 1.0, &[], 0),
            ],
        ))
        .await;
        let a = r.dispatch("rr", &[]).await.unwrap().agent_id;
        let b = r.dispatch("rr", &[]).await.unwrap().agent_id;
        let c = r.dispatch("rr", &[]).await.unwrap().agent_id;
        let d = r.dispatch("rr", &[]).await.unwrap().agent_id;
        assert_eq!(a, "a");
        assert_eq!(b, "b");
        assert_eq!(c, "c");
        assert_eq!(d, "a", "round-robin must wrap");
    }

    #[tokio::test]
    async fn least_loaded_picks_lowest_in_flight() {
        let r = ConstellationRouter::new();
        r.register(make(
            "ll",
            RoutingStrategy::LeastLoaded,
            vec![member("a", 1.0, &[], 0), member("b", 1.0, &[], 0)],
        ))
        .await;
        // First dispatch — both tied at 0, picks first.
        assert_eq!(r.dispatch("ll", &[]).await.unwrap().agent_id, "a");
        // Now a=1, b=0 → next pick should be b.
        assert_eq!(r.dispatch("ll", &[]).await.unwrap().agent_id, "b");
        // Now a=1, b=1 → tied, picks first (a).
        assert_eq!(r.dispatch("ll", &[]).await.unwrap().agent_id, "a");
    }

    #[tokio::test]
    async fn release_decrements_in_flight() {
        let r = ConstellationRouter::new();
        r.register(make(
            "rel",
            RoutingStrategy::LeastLoaded,
            vec![member("a", 1.0, &[], 0)],
        ))
        .await;
        r.dispatch("rel", &[]).await.unwrap();
        r.dispatch("rel", &[]).await.unwrap();
        let s = r.stats("rel").await.unwrap();
        assert_eq!(s.members[0].in_flight, 2);
        r.release("rel", "a").await;
        let s2 = r.stats("rel").await.unwrap();
        assert_eq!(s2.members[0].in_flight, 1);
        // Release more than dispatched should not underflow.
        r.release("rel", "a").await;
        r.release("rel", "a").await;
        let s3 = r.stats("rel").await.unwrap();
        assert_eq!(s3.members[0].in_flight, 0);
    }

    #[tokio::test]
    async fn tag_match_filters_to_required_tags() {
        let r = ConstellationRouter::new();
        r.register(make(
            "tm",
            RoutingStrategy::TagMatch {
                required_tags: vec!["rust".into()],
            },
            vec![
                member("a", 1.0, &["python"], 0),
                member("b", 1.0, &["rust", "fast"], 0),
                member("c", 1.0, &["rust"], 0),
            ],
        ))
        .await;
        // Only b and c have the "rust" tag → least-loaded between them
        // picks b first, then c.
        let first = r.dispatch("tm", &[]).await.unwrap().agent_id;
        assert!(first == "b" || first == "c");
        // Either way, picking again should round between b and c.
        let second = r.dispatch("tm", &[]).await.unwrap().agent_id;
        assert!(second == "b" || second == "c");
    }

    #[tokio::test]
    async fn extra_required_tags_intersect_with_eligibility() {
        let r = ConstellationRouter::new();
        r.register(make(
            "extra",
            RoutingStrategy::RoundRobin,
            vec![
                member("a", 1.0, &["fast"], 0),
                member("b", 1.0, &["fast", "secure"], 0),
                member("c", 1.0, &["secure"], 0),
            ],
        ))
        .await;
        let chosen = r
            .dispatch("extra", &["fast".into(), "secure".into()])
            .await
            .unwrap()
            .agent_id;
        assert_eq!(chosen, "b", "only b has both fast and secure tags");
    }

    #[tokio::test]
    async fn max_concurrent_excludes_saturated_member() {
        let r = ConstellationRouter::new();
        r.register(make(
            "cap",
            RoutingStrategy::RoundRobin,
            vec![member("a", 1.0, &[], 1), member("b", 1.0, &[], 1)],
        ))
        .await;
        // Two dispatches use up both members' single slot.
        let _ = r.dispatch("cap", &[]).await.unwrap();
        let _ = r.dispatch("cap", &[]).await.unwrap();
        // Third one should return NoEligibleMember.
        let err = r.dispatch("cap", &[]).await.unwrap_err();
        assert!(matches!(err, RoutingError::NoEligibleMember));
    }

    #[tokio::test]
    async fn stats_lists_every_member_with_dispatched_total() {
        let r = ConstellationRouter::new();
        r.register(make(
            "ss",
            RoutingStrategy::RoundRobin,
            vec![member("a", 1.0, &[], 0), member("b", 1.0, &[], 0)],
        ))
        .await;
        let _ = r.dispatch("ss", &[]).await.unwrap();
        let _ = r.dispatch("ss", &[]).await.unwrap();
        let _ = r.dispatch("ss", &[]).await.unwrap();
        let s = r.stats("ss").await.unwrap();
        assert_eq!(s.members.len(), 2);
        let total: u64 = s.members.iter().map(|m| m.dispatched_total).sum();
        assert_eq!(total, 3);
        assert_eq!(s.recent_decisions.len(), 3);
    }

    #[tokio::test]
    async fn list_returns_registered_specs() {
        let r = ConstellationRouter::new();
        r.register(make("first", RoutingStrategy::RoundRobin, vec![]))
            .await;
        r.register(make("second", RoutingStrategy::RoundRobin, vec![]))
            .await;
        let listed = r.list().await;
        let mut names: Vec<_> = listed.iter().map(|c| c.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["first", "second"]);
    }

    #[tokio::test]
    async fn weight_zero_member_is_skipped() {
        let r = ConstellationRouter::new();
        r.register(make(
            "wz",
            RoutingStrategy::RoundRobin,
            vec![member("a", 0.0, &[], 0), member("b", 1.0, &[], 0)],
        ))
        .await;
        // Only b is eligible.
        for _ in 0..3 {
            assert_eq!(r.dispatch("wz", &[]).await.unwrap().agent_id, "b");
        }
    }

    #[tokio::test]
    async fn re_register_replaces_in_place() {
        let r = ConstellationRouter::new();
        r.register(make(
            "dup",
            RoutingStrategy::RoundRobin,
            vec![member("a", 1.0, &[], 0)],
        ))
        .await;
        let replaced = r
            .register(make(
                "dup",
                RoutingStrategy::RoundRobin,
                vec![member("b", 1.0, &[], 0)],
            ))
            .await;
        assert!(
            replaced,
            "register should report it replaced an existing entry"
        );
        let listed = r.list().await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].agents[0].agent_id, "b");
    }
}
