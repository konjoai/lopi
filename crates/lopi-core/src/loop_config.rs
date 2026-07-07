//! Loop-engineering configuration — the "loop as code" artifact.
//!
//! A [`LoopConfig`] is the declarative, git-trackable description of how a
//! repo's autonomous loops behave: their trust level, intent anchor, enabled
//! skills/rules, permission policy, and halting conditions. It is the
//! source of truth that the Loop Engineering UI reads and (selectively) writes.
//!
//! See `docs/LOOP_ENGINEERING.md` for the design rationale.

use crate::self_prompt::SelfPromptStrategy;
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

/// How an agent's working copy is isolated from its peers.
///
/// The two points on the loop-engineering isolation ladder. `Branch` (the
/// legacy default) checks out a fresh branch in the *shared* working directory,
/// so concurrent runs must be serialized to avoid index corruption. `Worktree`
/// gives each run its own physical checkout via `git worktree`, so — in Osmani's
/// words — "one agent's edits literally can not touch the other one's."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IsolationMode {
    /// Branch-per-attempt in the shared working directory (default; serialized).
    #[default]
    Branch,
    /// A dedicated `git worktree` per run — true parallel isolation.
    Worktree,
}

impl IsolationMode {
    /// Whether this mode uses a dedicated `git worktree`.
    #[must_use]
    pub fn is_worktree(self) -> bool {
        matches!(self, Self::Worktree)
    }

    /// The canonical snake_case tag (`"branch"` / `"worktree"`), matching serde.
    /// Used for DB columns and JSON payloads.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Worktree => "worktree",
        }
    }

    /// Parse a mode from a case-insensitive tag, tolerating UI/CLI spellings.
    #[must_use]
    pub fn from_tag(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "branch" => Some(Self::Branch),
            "worktree" | "work_tree" => Some(Self::Worktree),
            _ => None,
        }
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
    /// Verifier as Explicit Gate — force the Konjo Verifier second-score pass
    /// for this loop, independent of `autonomy_level`. `false` (the default)
    /// leaves the only forcing mechanism as `autonomy_level >= VerifiedPr`
    /// ([`AutonomyLevel::requires_verifier`]), unchanged from before this
    /// field existed.
    #[serde(default)]
    pub verifier_required: bool,
    /// Model used for the verifier's grading pass (e.g. `"claude-opus-4-7"`).
    /// `None` (the default) resolves to a model that differs from the
    /// worker's, so the checker is never the same model as the maker
    /// ("never grade your own homework").
    #[serde(default)]
    pub verifier_model: Option<String>,
    /// Reasoning-effort hint folded into the verifier's system prompt (e.g.
    /// `"low"`, `"medium"`, `"high"`, `"max"`) — the same free-form presets
    /// used by worker-side launch controls. `None` (the default) omits the
    /// hint entirely.
    #[serde(default)]
    pub verifier_effort: Option<String>,
    /// How the loop re-prompts *itself* after a failed attempt. Defaults to
    /// [`Direct`](SelfPromptStrategy::Direct) — the legacy raw-failure injection.
    #[serde(default)]
    pub self_prompt: SelfPromptStrategy,
    /// When `true`, the self-prompt strategy **escalates** one rung up the S1→S4
    /// ladder on each failed attempt (starting from `self_prompt`), instead of
    /// staying pinned. See [`SelfPromptStrategy::escalated`]. Defaults to `false`.
    #[serde(default)]
    pub escalate_strategy: bool,
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
    /// How each run's working copy is isolated. Defaults to
    /// [`Branch`](IsolationMode::Branch) — the legacy shared-checkout behavior.
    #[serde(default)]
    pub isolation: IsolationMode,
    /// Phase 16.7 — earned-trust auto-promotion: promote the loop's autonomy one
    /// rung after this many **consecutive clean, verifier-passed** runs. `0` (the
    /// default) disables auto-promotion — trust stays pinned at `autonomy_level`.
    #[serde(default)]
    pub promote_after: u32,
    /// The highest autonomy level earned trust may auto-promote to. Caps the
    /// ladder so unattended auto-merge (L4) stays opt-in even on a long clean
    /// streak. Defaults to [`DraftPr`](AutonomyLevel::DraftPr) — i.e. no headroom
    /// above the conservative default until a human raises the ceiling.
    #[serde(default)]
    pub trust_ceiling: AutonomyLevel,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            autonomy_level: AutonomyLevel::default(),
            verifier_required: false,
            verifier_model: None,
            verifier_effort: None,
            self_prompt: SelfPromptStrategy::default(),
            escalate_strategy: false,
            vision_path: None,
            skills_enabled: Vec::new(),
            rules_enabled: Vec::new(),
            permission_allow: Vec::new(),
            permission_deny: Vec::new(),
            no_progress_limit: default_no_progress_limit(),
            max_iterations: default_max_iterations(),
            budget_tokens: 0,
            isolation: IsolationMode::default(),
            promote_after: 0,
            trust_ceiling: AutonomyLevel::default(),
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

    /// Serialize and write this config to `<repo>/.lopi/loop.toml`, creating the
    /// `.lopi/` directory if needed. This is the write side of loop-as-code: the
    /// UI edits a lever, the server persists the artifact, and it round-trips
    /// through [`load_from_repo`](Self::load_from_repo).
    ///
    /// # Errors
    /// Returns `Err` if the directory cannot be created, the config cannot be
    /// serialized to TOML, or the file cannot be written.
    pub fn save_to_repo(&self, repo_path: &Path) -> anyhow::Result<()> {
        let dir = repo_path.join(".lopi");
        std::fs::create_dir_all(&dir)?;
        let text = toml::to_string_pretty(self)?;
        std::fs::write(dir.join("loop.toml"), text)?;
        Ok(())
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
        if self.promote_after > 0 && self.trust_ceiling.rank() <= self.autonomy_level.rank() {
            issues.push(format!(
                "trust_ceiling ({}) is not above autonomy_level ({}) — earned-trust promotion can never fire",
                self.trust_ceiling.tag(),
                self.autonomy_level.tag(),
            ));
        }
        issues
    }
}

#[cfg(test)]
#[path = "loop_config_tests.rs"]
mod tests;
