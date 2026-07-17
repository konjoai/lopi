//! Goal intent: does a task's goal expect file changes, or is it review-only?
//!
//! This is the classifier behind *intent-aware success*. A run that produces
//! zero file changes used to be scored as a 100% pass and reported `goal_met`
//! — so a "write findings to research.md" goal that never wrote the file
//! "succeeded" with no commit, no PR, and nothing for a dependent task to use.
//! [`Deliverable`] lets `finalize` distinguish "produced nothing but the goal
//! demanded output" (a failure to retry) from "review/analysis goal that
//! legitimately changes nothing" (a real success).
//!
//! The default is inferred from the goal text ([`Deliverable::infer_from_goal`])
//! and fail-closed: anything not clearly review-only is treated as expecting
//! file changes, because for an autonomous coding agent "did nothing" is far
//! more often a failure than an intended no-op. A task can override the guess
//! explicitly (`Task::deliverable`).

use serde::{Deserialize, Serialize};

/// Whether a task's goal is expected to modify files. Decides what a
/// zero-diff attempt means at finalize time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Deliverable {
    /// The goal is expected to create or edit files (implement, fix, add,
    /// write a doc). A zero-diff attempt is a failure, not a success.
    FileChanges,
    /// The goal is review/analysis/answer-only and legitimately produces no
    /// file changes. Zero diff is still a valid success.
    ReviewOnly,
}

/// Verbs whose presence means the goal is expected to modify files. Checked
/// before the review verbs so a goal that both researches *and* writes (e.g.
/// "research X then write findings to research.md") resolves to
/// [`Deliverable::FileChanges`].
const CHANGE_VERBS: &[&str] = &[
    "write",
    "create",
    "add",
    "implement",
    "fix",
    "build",
    "edit",
    "update",
    "refactor",
    "generate",
    "make",
    "rename",
    "delete",
    "remove",
    "convert",
    "migrate",
    "patch",
    "append",
    "insert",
    "replace",
    "scaffold",
    "wire",
    "port",
    "bump",
    "install",
    "configure",
    "rewrite",
    "apply",
];

/// Verbs whose presence — absent any [`CHANGE_VERBS`] — means the goal is
/// review/analysis-only and legitimately produces no file changes.
const REVIEW_VERBS: &[&str] = &[
    "review",
    "analyze",
    "analyse",
    "investigate",
    "audit",
    "assess",
    "evaluate",
    "explain",
    "describe",
    "summarize",
    "summarise",
    "compare",
    "critique",
    "inspect",
    "examine",
    "answer",
    "comment",
    "diagnose",
];

impl Deliverable {
    /// Guess a task's deliverable from its goal text. Fail-closed: only a
    /// clearly review-only goal (a review verb and no change verb) yields
    /// [`Deliverable::ReviewOnly`]; everything else is
    /// [`Deliverable::FileChanges`].
    #[must_use]
    pub fn infer_from_goal(goal: &str) -> Self {
        let lower = goal.to_ascii_lowercase();
        let has = |verbs: &[&str]| {
            lower
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| verbs.contains(&word))
        };
        if has(CHANGE_VERBS) {
            Self::FileChanges
        } else if has(REVIEW_VERBS) {
            Self::ReviewOnly
        } else {
            Self::FileChanges
        }
    }

    /// Whether a zero-diff attempt should still count as a success for this
    /// deliverable. Only review-only goals are allowed to conclude with no
    /// file changes.
    #[must_use]
    pub fn allows_zero_diff_success(self) -> bool {
        matches!(self, Self::ReviewOnly)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::Deliverable;

    #[test]
    fn write_or_create_goals_expect_file_changes() {
        for goal in [
            "write findings to research.md",
            "research novel LLM memory architectures then write the findings to research.md",
            "create a new module for auth",
            "implement the retry loop",
            "fix the panic in scorer.rs",
            "add tests for the parser",
            "refactor run_loop into smaller files",
        ] {
            assert_eq!(
                Deliverable::infer_from_goal(goal),
                Deliverable::FileChanges,
                "goal={goal:?}"
            );
        }
    }

    #[test]
    fn review_and_analysis_goals_are_review_only() {
        for goal in [
            "review the auth module for security issues",
            "analyze the token usage of the agent loop",
            "investigate why the webhook handler stalls",
            "summarize the architecture",
            "explain how eviction works",
            "compare the two scoring strategies",
        ] {
            assert_eq!(
                Deliverable::infer_from_goal(goal),
                Deliverable::ReviewOnly,
                "goal={goal:?}"
            );
        }
    }

    #[test]
    fn a_change_verb_wins_over_a_review_verb() {
        // "review ... and fix" must expect changes — the fix is the point.
        assert_eq!(
            Deliverable::infer_from_goal("review the code and fix any bugs"),
            Deliverable::FileChanges
        );
    }

    #[test]
    fn unclassifiable_goals_default_to_expecting_changes() {
        // Fail-closed: no recognizable verb ⇒ assume output is expected, so a
        // zero-diff run is a failure rather than a phantom success.
        assert_eq!(
            Deliverable::infer_from_goal("the parser, but faster"),
            Deliverable::FileChanges
        );
    }

    #[test]
    fn only_review_only_allows_a_zero_diff_success() {
        assert!(Deliverable::ReviewOnly.allows_zero_diff_success());
        assert!(!Deliverable::FileChanges.allows_zero_diff_success());
    }

    #[test]
    fn deliverable_round_trips_as_snake_case_json() {
        let json = serde_json::to_string(&Deliverable::ReviewOnly).unwrap();
        assert_eq!(json, "\"review_only\"");
        let back: Deliverable = serde_json::from_str("\"file_changes\"").unwrap();
        assert_eq!(back, Deliverable::FileChanges);
    }
}
