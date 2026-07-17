//! Autonomy ladder — how much a loop is trusted to act without a human.
//!
//! Split out of `loop_config.rs` (Budget & Guardrail Controls, Part 2) purely
//! to keep that file under the 500-line CI file-size gate as the new
//! `[budget]` section was added — no behavioral change, [`AutonomyLevel`] is
//! re-exported from `loop_config` unchanged so every existing
//! `loop_config::AutonomyLevel` path stays valid.

use serde::{Deserialize, Serialize};

/// How much a loop is trusted to act without a human in the loop.
///
/// The phased-rollout ladder from loop-engineering practice (Cobus Greyling):
/// trust is *earned incrementally*, never assumed. Each level strictly
/// supersets the autonomy of the one below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    /// L1 — produce a report / diff artifact only; never open a PR.
    ReportOnly,
    /// L2 — open a *draft* PR; a human must approve before merge (default).
    #[default]
    DraftPr,
    /// L3 — run the verifier (maker/checker) before opening a PR.
    VerifiedPr,
    /// L4 — auto-merge when the verifier passes and the score clears the gate.
    AutoMerge,
}

impl AutonomyLevel {
    /// The level's rank on the ladder, `1..=4`.
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Self::ReportOnly => 1,
            Self::DraftPr => 2,
            Self::VerifiedPr => 3,
            Self::AutoMerge => 4,
        }
    }

    /// The canonical snake_case serialization tag (`"report_only"` … `"auto_merge"`),
    /// matching the serde representation. Used for DB columns and JSON payloads.
    #[must_use]
    pub fn tag_snake(self) -> &'static str {
        match self {
            Self::ReportOnly => "report_only",
            Self::DraftPr => "draft_pr",
            Self::VerifiedPr => "verified_pr",
            Self::AutoMerge => "auto_merge",
        }
    }

    /// Short `"L1".."L4"` tag for compact UI display.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::ReportOnly => "L1",
            Self::DraftPr => "L2",
            Self::VerifiedPr => "L3",
            Self::AutoMerge => "L4",
        }
    }

    /// Human-readable label for UI surfaces.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ReportOnly => "Report only",
            Self::DraftPr => "Draft PR",
            Self::VerifiedPr => "Verified PR",
            Self::AutoMerge => "Auto-merge",
        }
    }

    /// Whether this level opens a pull request at all (L2+).
    #[must_use]
    pub fn opens_pr(self) -> bool {
        self.rank() >= Self::DraftPr.rank()
    }

    /// Whether a human must approve before the change can merge (L1–L3).
    #[must_use]
    pub fn requires_human_approval(self) -> bool {
        self != Self::AutoMerge
    }

    /// Whether the verifier (maker/checker) must pass before the PR (L3+).
    #[must_use]
    pub fn requires_verifier(self) -> bool {
        self.rank() >= Self::VerifiedPr.rank()
    }

    /// Whether the loop may merge without human sign-off (L4 only).
    #[must_use]
    pub fn allows_auto_merge(self) -> bool {
        self == Self::AutoMerge
    }

    /// Parse a serialized tag (`"report_only"`, `"l3"`, `"verified_pr"`, …).
    /// Case-insensitive; accepts both the snake_case name and the `L1..L4` tag.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "report_only" | "reportonly" | "l1" => Some(Self::ReportOnly),
            "draft_pr" | "draftpr" | "l2" => Some(Self::DraftPr),
            "verified_pr" | "verifiedpr" | "l3" => Some(Self::VerifiedPr),
            "auto_merge" | "automerge" | "l4" => Some(Self::AutoMerge),
            _ => None,
        }
    }

    /// All levels in ascending order, for UI pickers.
    #[must_use]
    pub fn all() -> [Self; 4] {
        [
            Self::ReportOnly,
            Self::DraftPr,
            Self::VerifiedPr,
            Self::AutoMerge,
        ]
    }

    /// The level at ladder `rank`, clamped to the valid `1..=4` band.
    /// Inverse of [`rank`](Self::rank): `0/1 → L1`, `2 → L2`, `3 → L3`, `≥4 → L4`.
    #[must_use]
    pub fn from_rank(rank: u8) -> Self {
        match rank {
            0 | 1 => Self::ReportOnly,
            2 => Self::DraftPr,
            3 => Self::VerifiedPr,
            _ => Self::AutoMerge,
        }
    }

    /// One rung **up** the trust ladder, saturating at [`AutoMerge`](Self::AutoMerge).
    /// The earned-trust promotion step (Phase 16.7).
    #[must_use]
    pub fn promoted(self) -> Self {
        Self::from_rank(self.rank().saturating_add(1))
    }

    /// One rung **down** the trust ladder, saturating at [`ReportOnly`](Self::ReportOnly).
    /// The earned-trust demotion step applied on a regression.
    #[must_use]
    pub fn demoted(self) -> Self {
        Self::from_rank(self.rank().saturating_sub(1))
    }
}
