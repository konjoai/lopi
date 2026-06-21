//! Self-prompting loop strategies — how an agent re-prompts *itself* between
//! iterations of a retry loop.
//!
//! "Loop engineering" is the discipline of directing an autonomous agent's
//! inner loop. The single highest-leverage lever is the **self-prompt**: the
//! text the agent feeds back into its *own* next planning step after an attempt
//! fails. A raw error dump (the legacy behaviour) is one strategy among many;
//! the research literature shows that *reframing* the failure into a structured
//! self-reflection lifts retry success substantially on coding tasks.
//!
//! Each [`SelfPromptStrategy`] is a pure transform from a failure summary into
//! the next self-prompt. The runner's adaptive-retry path calls [`frame`] to
//! produce the string it injects into the next attempt's planning prompt, so a
//! strategy change is observable end-to-end without touching the loop control
//! flow.
//!
//! [`frame`]: SelfPromptStrategy::frame
//!
//! Strategy provenance:
//! - **Direct** — legacy: inject the raw failure, no reframing.
//! - **Reflexion** — verbal self-reflection (Shinn et al., *Reflexion*, 2023).
//! - **Self-Refine** — self-critique then revise (Madaan et al., *Self-Refine*, 2023).
//! - **Plan-Then-Act** — decompose before editing (Wang et al., *Plan-and-Solve*, 2023).

use serde::{Deserialize, Serialize};

/// How an agent reframes a failed attempt into its next self-prompt.
///
/// Ordered S1→S4 by how much cognitive scaffolding each adds before the agent
/// re-plans. [`Direct`](Self::Direct) is the conservative default and exactly
/// reproduces the legacy raw-error injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SelfPromptStrategy {
    /// S1 — inject the raw failure context verbatim; no reframing (default).
    #[default]
    Direct,
    /// S2 — Reflexion: write a short verbal self-reflection on the root cause,
    /// then re-plan a *different* approach.
    Reflexion,
    /// S3 — Self-Refine: critique the prior attempt against an explicit rubric,
    /// then revise to address each critique.
    SelfRefine,
    /// S4 — Plan-Then-Act: decompose the remaining work into an explicit,
    /// numbered plan before editing a single line.
    PlanThenAct,
}

impl SelfPromptStrategy {
    /// The strategy's rank on the scaffolding ladder, `1..=4`.
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Self::Direct => 1,
            Self::Reflexion => 2,
            Self::SelfRefine => 3,
            Self::PlanThenAct => 4,
        }
    }

    /// Canonical snake_case serde tag (`"direct"` … `"plan_then_act"`).
    #[must_use]
    pub fn tag_snake(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Reflexion => "reflexion",
            Self::SelfRefine => "self_refine",
            Self::PlanThenAct => "plan_then_act",
        }
    }

    /// Short `"S1".."S4"` tag for compact UI display.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Direct => "S1",
            Self::Reflexion => "S2",
            Self::SelfRefine => "S3",
            Self::PlanThenAct => "S4",
        }
    }

    /// Human-readable label for UI surfaces.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Direct => "Direct",
            Self::Reflexion => "Reflexion",
            Self::SelfRefine => "Self-Refine",
            Self::PlanThenAct => "Plan-Then-Act",
        }
    }

    /// One-line description of how the strategy reframes the next prompt.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Direct => "Inject the raw failure context; let the model re-plan unaided.",
            Self::Reflexion => {
                "Write a verbal self-reflection on the root cause, then try a different approach."
            }
            Self::SelfRefine => "Critique the attempt against a rubric, then revise each weakness.",
            Self::PlanThenAct => {
                "Decompose the remaining work into a numbered plan before editing."
            }
        }
    }

    /// Parse a serialized tag (`"reflexion"`, `"S3"`, `"self_refine"`, …).
    /// Case-insensitive; accepts the snake_case name or the `S1..S4` tag.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "direct" | "s1" => Some(Self::Direct),
            "reflexion" | "s2" => Some(Self::Reflexion),
            "self_refine" | "selfrefine" | "s3" => Some(Self::SelfRefine),
            "plan_then_act" | "planthenact" | "s4" => Some(Self::PlanThenAct),
            _ => None,
        }
    }

    /// All strategies in ascending order, for UI pickers.
    #[must_use]
    pub fn all() -> [Self; 4] {
        [
            Self::Direct,
            Self::Reflexion,
            Self::SelfRefine,
            Self::PlanThenAct,
        ]
    }

    /// Reframe a failure summary into the next attempt's self-prompt.
    ///
    /// `base_failure` is the raw failure block (test pass-rate, lint, diff,
    /// captured errors). `attempt` is the 1-based number of the attempt that
    /// just failed. [`Direct`](Self::Direct) returns `base_failure` unchanged,
    /// preserving the legacy behaviour byte-for-byte; the other strategies wrap
    /// it in a self-prompting preamble and a concrete instruction.
    #[must_use]
    pub fn frame(self, base_failure: &str, attempt: u8) -> String {
        match self {
            Self::Direct => base_failure.to_string(),
            Self::Reflexion => format!(
                "Reflexion — before re-planning attempt {next}, reflect on attempt {attempt}.\n\n\
                 {base_failure}\n\n\
                 Self-reflection (write this first): in 1–2 sentences, name the single root \
                 cause of the failure above. Then plan a *different* approach — do not repeat \
                 the same edits that just failed.",
                next = attempt + 1,
            ),
            Self::SelfRefine => format!(
                "Self-Refine — critique attempt {attempt}, then revise.\n\n\
                 {base_failure}\n\n\
                 Critique (write this first): grade the prior attempt against \
                 correctness, test coverage, and minimality. List each weakness as a bullet. \
                 Then revise so that every bullet is resolved in the next attempt.",
            ),
            Self::PlanThenAct => format!(
                "Plan-Then-Act — decompose before editing (attempt {next}).\n\n\
                 {base_failure}\n\n\
                 Plan (write this first): produce a numbered, dependency-ordered list of the \
                 concrete steps that will make the tests pass. Only after the plan is complete, \
                 implement step by step — do not edit any file before the plan exists.",
                next = attempt + 1,
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_is_direct() {
        assert_eq!(SelfPromptStrategy::default(), SelfPromptStrategy::Direct);
    }

    #[test]
    fn ranks_are_monotonic() {
        assert_eq!(SelfPromptStrategy::Direct.rank(), 1);
        assert_eq!(SelfPromptStrategy::Reflexion.rank(), 2);
        assert_eq!(SelfPromptStrategy::SelfRefine.rank(), 3);
        assert_eq!(SelfPromptStrategy::PlanThenAct.rank(), 4);
    }

    #[test]
    fn parse_accepts_names_and_tags() {
        assert_eq!(
            SelfPromptStrategy::parse("reflexion"),
            Some(SelfPromptStrategy::Reflexion)
        );
        assert_eq!(
            SelfPromptStrategy::parse("S3"),
            Some(SelfPromptStrategy::SelfRefine)
        );
        assert_eq!(
            SelfPromptStrategy::parse("  plan_then_act "),
            Some(SelfPromptStrategy::PlanThenAct)
        );
        assert_eq!(SelfPromptStrategy::parse("nonsense"), None);
    }

    #[test]
    fn serde_is_snake_case() {
        let json = serde_json::to_string(&SelfPromptStrategy::SelfRefine).unwrap();
        assert_eq!(json, "\"self_refine\"");
        let back: SelfPromptStrategy = serde_json::from_str("\"plan_then_act\"").unwrap();
        assert_eq!(back, SelfPromptStrategy::PlanThenAct);
    }

    #[test]
    fn tag_label_description_and_all() {
        // Exhaustive per-variant assertions on every `&'static str` accessor —
        // pins the exact output so a mutant that swaps an arm's string (e.g. the
        // cargo-mutants `"xyzzy"` replacement) is killed, not just the empty one.
        let table = [
            (
                SelfPromptStrategy::Direct,
                "S1",
                "direct",
                "Direct",
                "raw failure",
            ),
            (
                SelfPromptStrategy::Reflexion,
                "S2",
                "reflexion",
                "Reflexion",
                "self-reflection",
            ),
            (
                SelfPromptStrategy::SelfRefine,
                "S3",
                "self_refine",
                "Self-Refine",
                "Critique",
            ),
            (
                SelfPromptStrategy::PlanThenAct,
                "S4",
                "plan_then_act",
                "Plan-Then-Act",
                "numbered plan",
            ),
        ];
        for (s, tag, snake, label, desc_needle) in table {
            assert_eq!(s.tag(), tag, "tag for {label}");
            assert_eq!(s.tag_snake(), snake, "tag_snake for {label}");
            assert_eq!(s.label(), label, "label for {label}");
            assert!(
                s.description().contains(desc_needle),
                "description for {label} must mention {desc_needle:?}, got {:?}",
                s.description()
            );
            assert_eq!(SelfPromptStrategy::parse(s.tag_snake()), Some(s));
        }
        assert_eq!(SelfPromptStrategy::all().len(), 4);
    }

    #[test]
    fn s_tag_aliases_parse_to_each_variant() {
        // Pins the `s1`..`s4` alias arms in `parse` so per-arm mutants are killed.
        assert_eq!(
            SelfPromptStrategy::parse("s1"),
            Some(SelfPromptStrategy::Direct)
        );
        assert_eq!(
            SelfPromptStrategy::parse("s2"),
            Some(SelfPromptStrategy::Reflexion)
        );
        assert_eq!(
            SelfPromptStrategy::parse("s4"),
            Some(SelfPromptStrategy::PlanThenAct)
        );
    }

    #[test]
    fn plan_then_act_advances_the_attempt_counter() {
        // Independently pins PlanThenAct's `next = attempt + 1` expression.
        let framed = SelfPromptStrategy::PlanThenAct.frame("x", 1);
        assert!(framed.contains("attempt 2"), "next attempt is N+1");
    }

    #[test]
    fn direct_frame_is_byte_identical_to_input() {
        let base = "Attempt 1 failed:\n  test_pass_rate: 40%";
        assert_eq!(SelfPromptStrategy::Direct.frame(base, 1), base);
    }

    #[test]
    fn reflective_strategies_embed_the_base_failure_and_an_instruction() {
        let base = "Attempt 2 failed:\n  lint_errors: 3";
        for s in [
            SelfPromptStrategy::Reflexion,
            SelfPromptStrategy::SelfRefine,
            SelfPromptStrategy::PlanThenAct,
        ] {
            let framed = s.frame(base, 2);
            assert!(framed.contains(base), "{} must embed the failure", s.tag());
            assert!(
                framed.len() > base.len(),
                "{} must add a self-prompt",
                s.tag()
            );
            assert!(
                framed.to_lowercase().contains("write this first"),
                "{} must instruct the agent",
                s.tag()
            );
        }
    }

    #[test]
    fn reflexion_advances_the_attempt_counter() {
        let framed = SelfPromptStrategy::Reflexion.frame("x", 3);
        assert!(framed.contains("attempt 4"), "next attempt is N+1");
    }
}
