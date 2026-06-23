//! Lesson → skill promotion detection (Pentad M2.3).
//!
//! The Ratchet: when the same lesson keeps being re-learned, it should stop
//! being a transient lesson and become a named skill. This module is the
//! *detector* — it clusters recurring lessons by a bag-of-words fingerprint and
//! surfaces the clusters frequent enough to promote. Drafting the `SKILL.md` and
//! the human-approval gate live in the runner; keeping detection pure makes the
//! threshold logic fully unit-testable and decoupled from the lessons store.

use std::collections::HashMap;

/// Minimum word length kept in a fingerprint. Shorter tokens (articles, "is",
/// "to", "run") carry little clustering signal and only fragment groups.
const MIN_WORD_LEN: usize = 4;

/// A cluster of recurring lessons that qualifies for promotion to a skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionCandidate {
    /// Bag-of-words fingerprint shared by the cluster.
    pub fingerprint: String,
    /// How many lessons fell into this cluster.
    pub occurrences: usize,
    /// The most common category among the cluster's lessons.
    pub category: String,
    /// Distinct raw lesson contents in the cluster, in first-seen order.
    pub examples: Vec<String>,
}

/// Identify recurring lesson clusters worth promoting, most-frequent first.
///
/// `lessons` is a slice of `(category, content)` pairs (as loaded from the
/// lessons store). Lessons are grouped by [`fingerprint`] so trivial wording
/// differences still cluster; any cluster with at least `min_occurrences`
/// lessons is returned. A `min_occurrences` of 0 is treated as 1, and lessons
/// whose fingerprint is empty (no significant words) are ignored.
#[must_use]
pub fn promotion_candidates(
    lessons: &[(String, String)],
    min_occurrences: usize,
) -> Vec<PromotionCandidate> {
    let threshold = min_occurrences.max(1);
    let mut clusters: HashMap<String, Cluster> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (category, content) in lessons {
        let fp = fingerprint(content);
        if fp.is_empty() {
            continue;
        }
        let cluster = clusters.entry(fp.clone()).or_insert_with(|| {
            order.push(fp.clone());
            Cluster::default()
        });
        cluster.add(category, content);
    }
    let mut out: Vec<PromotionCandidate> = order
        .into_iter()
        .filter_map(|fp| {
            let c = clusters.remove(&fp)?;
            (c.occurrences >= threshold).then(|| c.into_candidate(fp))
        })
        .collect();
    // Most-recurring first; fingerprint as a stable tiebreak for determinism.
    out.sort_by(|a, b| {
        b.occurrences
            .cmp(&a.occurrences)
            .then_with(|| a.fingerprint.cmp(&b.fingerprint))
    });
    out
}

/// Accumulates one fingerprint group while scanning lessons.
#[derive(Default)]
struct Cluster {
    occurrences: usize,
    categories: HashMap<String, usize>,
    examples: Vec<String>,
}

impl Cluster {
    fn add(&mut self, category: &str, content: &str) {
        self.occurrences += 1;
        *self.categories.entry(category.to_string()).or_default() += 1;
        if !self.examples.iter().any(|e| e == content) {
            self.examples.push(content.to_string());
        }
    }

    /// The category with the most lessons; ties broken alphabetically for
    /// determinism. Empty string when the cluster somehow has no category.
    fn dominant_category(&self) -> String {
        self.categories
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(k, _)| k.clone())
            .unwrap_or_default()
    }

    fn into_candidate(self, fingerprint: String) -> PromotionCandidate {
        PromotionCandidate {
            fingerprint,
            occurrences: self.occurrences,
            category: self.dominant_category(),
            examples: self.examples,
        }
    }
}

/// A bag-of-words fingerprint: lowercased significant words, deduped and sorted.
///
/// Dropping short words and word order means lessons that say the same thing
/// differently still land in one cluster. Isolated here so it can be swapped for
/// an embedding bucket to make clustering semantic without changing callers.
#[must_use]
pub fn fingerprint(content: &str) -> String {
    let mut words: Vec<String> = content
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= MIN_WORD_LEN)
        .map(str::to_lowercase)
        .collect();
    words.sort();
    words.dedup();
    words.join(" ")
}

/// Render a `SKILL.md` draft for a promotion `candidate` — the artifact a human
/// reviews (and edits) before it is committed as a real skill. The output parses
/// cleanly via [`Skill::parse`](crate::Skill::parse): a draft is always a valid
/// skill, never half-formed. Pure, so it is fully unit-testable.
#[must_use]
pub fn draft_skill_md(candidate: &PromotionCandidate) -> String {
    let name = draft_skill_name(&candidate.fingerprint);
    let triggers = candidate.fingerprint.replace(' ', ", ");
    let examples = candidate
        .examples
        .iter()
        .map(|e| format!("- {e}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "---\n\
         name: {name}\n\
         description: Recurring {category} lesson, auto-promoted after {n} occurrences. Apply it proactively.\n\
         user-invocable: false\n\
         version: 0.1.0\n\
         triggers: {triggers}\n\
         ---\n\
         # {name}\n\
         \n\
         This pattern recurred **{n}** times in the lessons ledger and was\n\
         promoted to a draft skill for review.\n\
         \n\
         ## What was repeatedly learned\n\
         {examples}\n",
        category = candidate.category,
        n = candidate.occurrences,
    )
}

/// A skill name slug for a candidate: `learned-<first three fingerprint words>`.
pub(crate) fn draft_skill_name(fingerprint: &str) -> String {
    let slug = fingerprint
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "learned-pattern".to_string()
    } else {
        format!("learned-{slug}")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::{fingerprint, promotion_candidates};

    fn lesson(cat: &str, content: &str) -> (String, String) {
        (cat.to_string(), content.to_string())
    }

    #[test]
    fn fingerprint_is_order_and_case_insensitive() {
        assert_eq!(
            fingerprint("Always run the TESTS after refactor"),
            fingerprint("after REFACTOR, run tests always")
        );
        // Short words (`run`, `the`) are dropped; significant ones remain.
        assert_eq!(fingerprint("run the tests"), "tests");
    }

    #[test]
    fn empty_fingerprint_for_only_short_words() {
        assert_eq!(fingerprint("a is to of in"), "");
    }

    #[test]
    fn clusters_recurring_lessons_over_threshold() {
        // The detector targets recurring lessons that differ only in case, word
        // order, punctuation, and short filler — exactly what `fingerprint`
        // normalizes — not loose paraphrases.
        let lessons = vec![
            lesson("recovery", "Run the tests after refactor"),
            lesson("recovery", "after refactor run tests"),
            lesson("strategy", "TESTS, after refactor, run!"),
            lesson("optimization", "Cache build artifacts between runs"),
        ];
        let candidates = promotion_candidates(&lessons, 3);
        assert_eq!(
            candidates.len(),
            1,
            "only the refactor/tests cluster qualifies"
        );
        let c = &candidates[0];
        assert_eq!(c.occurrences, 3);
        assert_eq!(c.fingerprint, "after refactor tests");
        assert_eq!(c.category, "recovery", "dominant category wins");
        assert_eq!(c.examples.len(), 3, "three distinct phrasings kept");
    }

    #[test]
    fn below_threshold_yields_nothing() {
        let lessons = vec![
            lesson("strategy", "Pin the toolchain version explicitly"),
            lesson("strategy", "Explicitly pin the toolchain version"),
        ];
        assert!(promotion_candidates(&lessons, 3).is_empty());
        // ...but a threshold of 2 surfaces it.
        assert_eq!(promotion_candidates(&lessons, 2).len(), 1);
    }

    #[test]
    fn sorted_by_frequency_then_fingerprint() {
        let mut lessons = vec![lesson("a", "alpha beta gamma"); 2];
        lessons.extend(vec![lesson("a", "delta epsilon zeta"); 4]);
        let candidates = promotion_candidates(&lessons, 2);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].occurrences, 4, "most frequent first");
        assert_eq!(candidates[1].occurrences, 2);
    }

    #[test]
    fn draft_round_trips_through_the_skill_parser() {
        use crate::Skill;
        use std::path::Path;
        let lessons = vec![
            lesson("recovery", "Run the tests after refactor"),
            lesson("recovery", "after refactor run tests"),
            lesson("strategy", "TESTS, after refactor, run!"),
        ];
        let candidate = &promotion_candidates(&lessons, 3)[0];
        let md = super::draft_skill_md(candidate);

        // A draft must always be a valid skill the registry can load.
        let skill = Skill::parse(&md, Path::new("/.lopi/skills/x/SKILL.md")).unwrap();
        assert_eq!(skill.name, "learned-after-refactor-tests");
        assert!(!skill.user_invocable, "drafts are not user-invocable");
        assert_eq!(skill.version, "0.1.0");
        assert_eq!(skill.triggers, vec!["after", "refactor", "tests"]);
        assert!(skill.description.contains("after 3 occurrences"));
        assert!(skill.body.contains("Run the tests after refactor"));
    }

    #[test]
    fn draft_name_handles_empty_fingerprint() {
        assert_eq!(super::draft_skill_name(""), "learned-pattern");
        assert_eq!(super::draft_skill_name("alpha beta"), "learned-alpha-beta");
    }

    #[test]
    fn zero_threshold_treated_as_one_and_skips_empty() {
        let lessons = vec![
            lesson("s", "a to of"),
            lesson("s", "meaningful lesson content"),
        ];
        let candidates = promotion_candidates(&lessons, 0);
        // The all-short-words lesson is ignored; the real one surfaces.
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].occurrences, 1);
    }
}
