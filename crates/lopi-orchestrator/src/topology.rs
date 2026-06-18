//! Topology classifier (Sprint T).
//!
//! Maps a task goal to a [`TopologyHint`] using a fast keyword heuristic. When
//! the heuristic is not confident (`confidence < CONFIDENCE_THRESHOLD`), the
//! caller may fall back to a Haiku classification pass — that fallback is not
//! implemented here; this module only provides the deterministic first stage.
//!
//! Inspired by AdaptOrch (arXiv 2602.16873): selecting the right topology per
//! task beats any single static topology by 12–23% on identical models.

use lopi_core::topology::TopologyHint;

/// Confidence at or above which the heuristic verdict is trusted without a
/// Haiku fallback.
pub const CONFIDENCE_THRESHOLD: f64 = 0.6;

/// Outcome of a topology classification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TopologyClassification {
    /// The proposed topology.
    pub hint: TopologyHint,
    /// Heuristic confidence in `[0.0, 1.0]`.
    pub confidence: f64,
    /// True when `confidence < CONFIDENCE_THRESHOLD` — a Haiku fallback is
    /// advisable before acting on the hint.
    pub low_confidence: bool,
}

/// Keyword signals for each non-default topology. Order matches the categories
/// scored in [`classify`].
const SEQUENTIAL_KEYWORDS: &[&str] = &[
    "then",
    "after",
    "step by step",
    "first",
    "once",
    "followed by",
    "migrate",
    "upgrade",
    "in order",
];
const PARALLEL_KEYWORDS: &[&str] = &[
    "each",
    "every",
    "all ",
    "across",
    "in parallel",
    "independently",
    "fan out",
    "for all",
    "multiple files",
    "concurrently",
];
const HIERARCHICAL_KEYWORDS: &[&str] = &[
    "decompose",
    "break down",
    "subtask",
    "coordinate",
    "orchestrate",
    "epic",
    "multi-part",
    "plan and",
    "delegate",
];

/// Count how many keywords appear in `haystack`.
fn count_hits(haystack: &str, keywords: &[&str]) -> u32 {
    keywords.iter().filter(|kw| haystack.contains(*kw)).count() as u32
}

/// Classify a task goal into a topology using keyword heuristics.
///
/// Returns [`TopologyHint::Hybrid`] with low confidence when the goal shows no
/// clear signal or two categories tie.
#[must_use]
pub fn classify(goal: &str) -> TopologyClassification {
    let g = goal.to_ascii_lowercase();
    let scored = [
        (
            TopologyHint::Sequential,
            count_hits(&g, SEQUENTIAL_KEYWORDS),
        ),
        (TopologyHint::Parallel, count_hits(&g, PARALLEL_KEYWORDS)),
        (
            TopologyHint::Hierarchical,
            count_hits(&g, HIERARCHICAL_KEYWORDS),
        ),
    ];
    verdict_from_scores(&scored)
}

/// Turn per-category hit counts into a classification. The winner is the
/// highest-scoring category; confidence grows with its margin over the runner-up.
fn verdict_from_scores(scored: &[(TopologyHint, u32)]) -> TopologyClassification {
    let total: u32 = scored.iter().map(|(_, n)| n).sum();
    if total == 0 {
        return hybrid(0.30);
    }
    let mut sorted: Vec<u32> = scored.iter().map(|(_, n)| *n).collect();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let margin = sorted[0].saturating_sub(sorted.get(1).copied().unwrap_or(0));
    if margin == 0 {
        return hybrid(0.50);
    }
    let (hint, _) = scored
        .iter()
        .max_by_key(|(_, n)| *n)
        .copied()
        .unwrap_or((TopologyHint::Hybrid, 0));
    let confidence = (0.50 + 0.15 * f64::from(margin)).min(0.95);
    TopologyClassification {
        hint,
        confidence,
        low_confidence: confidence < CONFIDENCE_THRESHOLD,
    }
}

/// Build a `Hybrid` verdict at the given confidence (always low-confidence,
/// since `Hybrid` is the fallback rather than a positive signal).
fn hybrid(confidence: f64) -> TopologyClassification {
    TopologyClassification {
        hint: TopologyHint::Hybrid,
        confidence,
        low_confidence: confidence < CONFIDENCE_THRESHOLD,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn empty_signal_yields_low_confidence_hybrid() {
        let v = classify("do the thing");
        assert_eq!(v.hint, TopologyHint::Hybrid);
        assert!(v.low_confidence);
    }

    #[test]
    fn sequential_goal_classifies_sequential() {
        let v = classify("First add the column, then migrate the data in order");
        assert_eq!(v.hint, TopologyHint::Sequential);
        assert!(!v.low_confidence, "clear sequential signal");
    }

    #[test]
    fn parallel_goal_classifies_parallel() {
        let v = classify("Update every crate independently across the workspace in parallel");
        assert_eq!(v.hint, TopologyHint::Parallel);
        assert!(v.confidence > CONFIDENCE_THRESHOLD);
    }

    #[test]
    fn hierarchical_goal_classifies_hierarchical() {
        let v = classify("Decompose this epic into subtasks and delegate to child agents");
        assert_eq!(v.hint, TopologyHint::Hierarchical);
        assert!(!v.low_confidence);
    }

    #[test]
    fn tie_between_two_categories_falls_back_to_hybrid() {
        // one sequential ("then") + one parallel ("each") → margin 0.
        let v = classify("update each module then stop");
        assert_eq!(v.hint, TopologyHint::Hybrid);
        assert!(v.low_confidence);
    }

    #[test]
    fn confidence_is_bounded() {
        let v = classify("first then after once migrate upgrade in order followed by step by step");
        assert!(v.confidence <= 0.95);
        assert!(v.confidence >= 0.0);
    }

    #[test]
    fn case_insensitive_matching() {
        let v = classify("DECOMPOSE the EPIC and DELEGATE subtask work");
        assert_eq!(v.hint, TopologyHint::Hierarchical);
    }
}
