//! Loop-engineering configuration — the "loop as code" artifact.
//!
//! A [`LoopConfig`] is the declarative, git-trackable description of how a
//! repo's autonomous loops behave: their trust level, intent anchor, enabled
//! skills/rules, permission policy, and halting conditions. It is the
//! source of truth that the Loop Engineering UI reads and (selectively) writes.
//!
//! See `docs/LOOP_ENGINEERING.md` for the design rationale.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
}

/// Declarative loop-engineering configuration for a repo.
///
/// Loaded from `<repo>/.lopi/loop.toml`. Every field has a safe default, so an
/// absent file yields a conservative loop (draft-PR autonomy, all skills/rules
/// enabled, a generous iteration cap).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopConfig {
    /// Default trust level for loops in this repo.
    #[serde(default)]
    pub autonomy_level: AutonomyLevel,
    /// Path to the intent-anchor doc (VISION.md / AGENTS.md), relative to repo root.
    #[serde(default)]
    pub vision_path: Option<PathBuf>,
    /// Skill names enabled for this repo's loops. Empty = all discovered skills.
    #[serde(default)]
    pub skills_enabled: Vec<String>,
    /// Rule files enabled. Empty = all files in `.claude/rules`.
    #[serde(default)]
    pub rules_enabled: Vec<String>,
    /// Tool-call patterns pre-approved without prompting (e.g. `"Bash(cargo test *)"`).
    #[serde(default)]
    pub permission_allow: Vec<String>,
    /// Tool-call patterns always denied (e.g. `"Bash(rm -rf *)"`).
    #[serde(default)]
    pub permission_deny: Vec<String>,
    /// Halt after this many consecutive no-progress iterations (`0` = disabled).
    #[serde(default = "default_no_progress_limit")]
    pub no_progress_limit: u8,
    /// Hard iteration ceiling regardless of any other condition.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u8,
    /// Per-run token budget ceiling (`0` = inherit the global budget).
    #[serde(default)]
    pub budget_tokens: u64,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            autonomy_level: AutonomyLevel::default(),
            vision_path: None,
            skills_enabled: Vec::new(),
            rules_enabled: Vec::new(),
            permission_allow: Vec::new(),
            permission_deny: Vec::new(),
            no_progress_limit: default_no_progress_limit(),
            max_iterations: default_max_iterations(),
            budget_tokens: 0,
        }
    }
}

fn default_no_progress_limit() -> u8 {
    3
}

fn default_max_iterations() -> u8 {
    25
}

impl LoopConfig {
    /// Conventional location of the loop config inside a repo.
    pub const REL_PATH: &'static str = ".lopi/loop.toml";

    /// Load `<repo>/.lopi/loop.toml`. Returns [`Default`] if the file is absent.
    ///
    /// # Errors
    /// Returns `Err` if the file exists but cannot be read or parsed as TOML —
    /// a malformed loop config is surfaced loudly rather than silently ignored.
    pub fn load_from_repo(repo_path: &Path) -> anyhow::Result<Self> {
        let p = repo_path.join(Self::REL_PATH);
        if !p.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&p)?;
        let cfg: Self = toml::from_str(&text)?;
        Ok(cfg)
    }

    /// Validate the config against a repo on disk, returning a list of
    /// human-readable issues. An empty vec means the config is valid.
    ///
    /// Checks: vision-anchor existence, iteration-cap sanity, and that a
    /// no-progress limit does not exceed the hard iteration cap.
    #[must_use]
    pub fn validate(&self, repo_path: &Path) -> Vec<String> {
        let mut issues = Vec::new();
        if let Some(v) = &self.vision_path {
            if !repo_path.join(v).exists() {
                issues.push(format!("vision_path does not exist: {}", v.display()));
            }
        }
        if self.max_iterations == 0 {
            issues.push("max_iterations is 0 — the loop could never run".into());
        }
        if self.no_progress_limit > self.max_iterations {
            issues.push(format!(
                "no_progress_limit ({}) exceeds max_iterations ({}) — it can never trigger",
                self.no_progress_limit, self.max_iterations
            ));
        }
        issues
    }
}

#[cfg(test)]
#[path = "loop_config_tests.rs"]
mod tests;
