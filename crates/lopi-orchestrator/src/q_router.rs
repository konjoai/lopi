//! Q-learning router (Sprint T).
//!
//! A contextual-bandit router that learns which agent configuration performs
//! best for each task type. The *state* is a task type, the *action* is an
//! agent-config identifier, and the reward is the Konjo Verifier composite
//! score (or any signal normalised to `[0, 1]`). Selection is epsilon-greedy
//! (ε = 0.1 by default); each `(state, action)` estimate is moved toward the
//! observed reward with learning rate α via `Q ← Q + α·(reward − Q)`.
//!
//! Grounded in AdaptOrch (arXiv 2602.16873) and RL-via-orchestration-traces
//! (arXiv 2605.02801): because the reward targets end-to-end task quality, the
//! router converges on the config that ships the best diffs per task type.

use dashmap::DashMap;

/// Default exploration rate — the fraction of selections made at random.
pub const DEFAULT_EPSILON: f64 = 0.1;
/// Default learning rate folded into each Q-value update.
pub const DEFAULT_ALPHA: f64 = 0.5;

/// Running statistics for one `(state, action)` pair.
#[derive(Debug, Clone, Copy)]
struct QStat {
    /// Current value estimate in `[0, 1]`.
    q: f64,
    /// Number of rewards folded into `q`.
    n: u64,
}

/// One exported Q-table cell — used for persistence and the REST endpoint.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QValueEntry {
    /// Task type this estimate is keyed on.
    pub state: String,
    /// Agent-config identifier this estimate is keyed on.
    pub action: String,
    /// Value estimate in `[0, 1]`.
    pub q: f64,
    /// Number of rewards folded into `q`.
    pub updates: u64,
}

/// Epsilon-greedy Q-learning router over a `(task_type → agent_config)` table.
#[derive(Debug)]
pub struct QRouter {
    table: DashMap<(String, String), QStat>,
    epsilon: f64,
    alpha: f64,
}

impl Default for QRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl QRouter {
    /// Create a router with the default exploration and learning rates.
    #[must_use]
    pub fn new() -> Self {
        Self::with_params(DEFAULT_EPSILON, DEFAULT_ALPHA)
    }

    /// Create a router with explicit `epsilon` (explore rate) and `alpha`
    /// (learning rate). Both are clamped to `[0, 1]`.
    #[must_use]
    pub fn with_params(epsilon: f64, alpha: f64) -> Self {
        Self {
            table: DashMap::new(),
            epsilon: epsilon.clamp(0.0, 1.0),
            alpha: alpha.clamp(0.0, 1.0),
        }
    }

    /// Current Q-value for a `(state, action)` pair; `0.0` when unseen.
    #[must_use]
    pub fn q_value(&self, state: &str, action: &str) -> f64 {
        self.table
            .get(&(state.to_string(), action.to_string()))
            .map_or(0.0, |s| s.q)
    }

    /// Select an action for `state` from `actions` using epsilon-greedy.
    ///
    /// Returns `None` only when `actions` is empty. With probability `epsilon`
    /// it explores (uniform random); otherwise it exploits the highest-valued
    /// action, defaulting unseen pairs to `0.0`.
    pub fn select<'a>(&self, state: &str, actions: &'a [String]) -> Option<&'a String> {
        if actions.is_empty() {
            return None;
        }
        if self.roll() < self.epsilon {
            let idx = self.roll_nanos() as usize % actions.len();
            return actions.get(idx);
        }
        actions.iter().max_by(|a, b| {
            self.q_value(state, a)
                .partial_cmp(&self.q_value(state, b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Fold an observed `reward` (clamped to `[0, 1]`) into `Q(state, action)`.
    pub fn update(&self, state: &str, action: &str, reward: f64) {
        let r = reward.clamp(0.0, 1.0);
        let mut entry = self
            .table
            .entry((state.to_string(), action.to_string()))
            .or_insert(QStat { q: 0.0, n: 0 });
        entry.q += self.alpha * (r - entry.q);
        entry.n += 1;
    }

    /// Export every Q-table cell, for persistence or inspection.
    #[must_use]
    pub fn snapshot(&self) -> Vec<QValueEntry> {
        self.table
            .iter()
            .map(|kv| {
                let (state, action) = kv.key();
                QValueEntry {
                    state: state.clone(),
                    action: action.clone(),
                    q: kv.value().q,
                    updates: kv.value().n,
                }
            })
            .collect()
    }

    /// Replace the in-memory table with persisted entries (e.g. on boot).
    pub fn hydrate(&self, entries: impl IntoIterator<Item = QValueEntry>) {
        for e in entries {
            self.table.insert(
                (e.state, e.action),
                QStat {
                    q: e.q,
                    n: e.updates,
                },
            );
        }
    }

    /// Wall-clock sub-second nanos — a cheap, non-degenerate pseudo-random
    /// source. Selection quality does not require cryptographic randomness.
    fn roll_nanos(&self) -> u32 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    }

    /// Pseudo-random value in `[0, 1)`.
    fn roll(&self) -> f64 {
        f64::from(self.roll_nanos()) / 1_000_000_000.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn actions() -> Vec<String> {
        vec![
            "fast".to_string(),
            "deep".to_string(),
            "balanced".to_string(),
        ]
    }

    #[test]
    fn unseen_pair_is_zero() {
        let r = QRouter::new();
        assert_eq!(r.q_value("s", "a"), 0.0);
    }

    #[test]
    fn update_moves_toward_reward() {
        let r = QRouter::with_params(0.0, 0.5);
        r.update("refactor", "deep", 1.0);
        assert!((r.q_value("refactor", "deep") - 0.5).abs() < 1e-9);
        r.update("refactor", "deep", 1.0);
        assert!((r.q_value("refactor", "deep") - 0.75).abs() < 1e-9);
    }

    #[test]
    fn reward_is_clamped() {
        let r = QRouter::with_params(0.0, 1.0);
        r.update("s", "a", 5.0);
        assert!((r.q_value("s", "a") - 1.0).abs() < 1e-9);
        r.update("s", "b", -3.0);
        assert_eq!(r.q_value("s", "b"), 0.0);
    }

    #[test]
    fn greedy_select_picks_highest_value() {
        let r = QRouter::with_params(0.0, 0.5);
        let acts = actions();
        r.update("feature", "deep", 1.0);
        r.update("feature", "fast", 0.2);
        assert_eq!(r.select("feature", &acts), Some(&"deep".to_string()));
    }

    #[test]
    fn select_empty_returns_none() {
        let r = QRouter::new();
        assert_eq!(r.select("s", &[]), None);
    }

    #[test]
    fn explore_always_returns_a_candidate() {
        let r = QRouter::with_params(1.0, 0.5);
        let acts = actions();
        for _ in 0..20 {
            assert!(r.select("s", &acts).is_some());
        }
    }

    #[test]
    fn snapshot_round_trips_through_hydrate() {
        let r = QRouter::with_params(0.0, 0.5);
        r.update("a", "x", 0.8);
        r.update("b", "y", 0.4);
        let snap = r.snapshot();
        assert_eq!(snap.len(), 2);

        let restored = QRouter::new();
        restored.hydrate(snap);
        assert!((restored.q_value("a", "x") - r.q_value("a", "x")).abs() < 1e-9);
        assert!((restored.q_value("b", "y") - r.q_value("b", "y")).abs() < 1e-9);
    }

    #[test]
    fn update_count_accumulates() {
        let r = QRouter::with_params(0.0, 0.5);
        r.update("s", "a", 1.0);
        r.update("s", "a", 0.5);
        let cell = r.snapshot().into_iter().find(|e| e.action == "a").unwrap();
        assert_eq!(cell.updates, 2);
    }

    #[test]
    fn params_are_clamped() {
        let r = QRouter::with_params(5.0, -1.0);
        // epsilon clamped to 1.0 → always explore but still returns a candidate.
        assert!(r.select("s", &actions()).is_some());
        // alpha clamped to 0.0 → updates never move the estimate.
        r.update("s", "a", 1.0);
        assert_eq!(r.q_value("s", "a"), 0.0);
    }
}
