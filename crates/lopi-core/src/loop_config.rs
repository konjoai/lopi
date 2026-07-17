//! Loop-engineering configuration — the "loop as code" artifact.
//!
//! A [`LoopConfig`] is the declarative, git-trackable description of how a
//! repo's autonomous loops behave: their trust level, intent anchor, enabled
//! skills/rules, permission policy, and halting conditions. It is the
//! source of truth that the Loop Engineering UI reads and (selectively) writes.
//!
//! See `docs/LOOP_ENGINEERING.md` for the design rationale.

use crate::budget_preset::{BudgetSection, ResolvedBudget};
use crate::self_prompt::SelfPromptStrategy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Re-exported so `loop_config::AutonomyLevel` stays valid — the type moved to
/// its own module (`autonomy.rs`) purely to keep this file under the 500-line
/// CI file-size gate; see that module's doc comment.
pub use crate::autonomy::AutonomyLevel;

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

/// Policy applied after a loop iteration fails.
///
/// `Stop` is the default and the only variant whose runtime effect must
/// reproduce today's behavior exactly — every config written before this
/// enum existed deserializes to `Stop` via `#[serde(default)]`, and the
/// pre-existing retry loop already runs its bounded `max_retries` course
/// with a backoff pause between attempts. See `LEDGER.md` for why `Stop`
/// and `Backoff` currently share that same wait rather than `Stop` cutting
/// the loop short after one failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OnFail {
    /// Preserve the existing bounded-retry envelope: back off, then retry,
    /// until `max_retries`/`max_iterations` is exhausted (unchanged default).
    #[default]
    Stop,
    /// Proceed to the next attempt immediately, skipping the backoff pause.
    Continue,
    /// Explicitly pace retries with the existing full-jitter backoff
    /// ([`backoff_secs`](crate) equivalent in `lopi-agent`) — the same wait
    /// `Stop` already applies, offered as a named, user-selectable choice.
    Backoff,
}

/// Run a shell command in `cwd` and report whether it exited `0`.
///
/// Shared by the `gate` and `until` guardrails — the only two places a
/// user-supplied shell string is executed. Invoked via `sh -c` (unlike the
/// codebase's other shell-outs, which always run a fixed known binary with
/// explicit args) since these are free-form command strings, not an argv
/// array. Only the exit status is inspected — stdout/stderr are discarded,
/// since the pass/fail decision this guards needs nothing else.
///
/// SECURITY: `cmd` is user-supplied config, run in the repo's own working
/// directory — the same trust model as the existing git/gh shell-outs
/// (a local dev tool operating on the user's own repo), not a
/// network-exposed execution surface.
///
/// # Errors
/// Returns `Err` only if the shell itself could not be spawned (e.g. `sh`
/// missing from `PATH`). A command that runs and exits non-zero is a normal
/// `Ok(false)`, not an error.
pub async fn run_guard_command(cmd: &str, cwd: &Path) -> std::io::Result<bool> {
    let status = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .status()
        .await?;
    Ok(status.success())
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
    /// Forwarded to `claude -p` as `--allowedTools`. Include a tool named in
    /// `permission_deny`'s default (currently just `"Workflow"`) here to
    /// re-enable it for a repo that intentionally wants multi-agent fan-out.
    #[serde(default)]
    pub permission_allow: Vec<String>,
    /// Tool-call patterns always denied (e.g. `"Bash(rm -rf *)"`). Forwarded
    /// to `claude -p` as `--disallowedTools` — genuinely enforced even
    /// alongside `--dangerously-skip-permissions` (verified live: the model
    /// reports the tool as simply absent, not merely unapproved). Defaults to
    /// denying `Workflow`, the multi-agent orchestration primitive: a
    /// deep-research-style goal fanning out through it is what actually ran
    /// one `claude -p` session to $25.79 with no budget cap in place at all.
    /// `max_budget_usd`/`budget_tokens` above cap *how much* such a session
    /// can spend before halting; this stops the fan-out itself from starting
    /// on loops that don't explicitly opt in via `permission_allow`.
    #[serde(default = "default_permission_deny")]
    pub permission_deny: Vec<String>,
    /// Halt after this many consecutive no-progress iterations (`0` = disabled).
    #[serde(default = "default_no_progress_limit")]
    pub no_progress_limit: u8,
    /// Hard iteration ceiling regardless of any other condition.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u8,
    /// Per-run token budget ceiling metered from the outer runner's own
    /// streamed usage across the whole retry loop (`0` = disabled). Second,
    /// independent line of defense alongside `max_budget_usd` below — this one
    /// catches ordinary retry-loop accumulation (many small attempts adding
    /// up), while `max_budget_usd` catches one session spending big in one
    /// shot. Defaults to a conservative non-zero value for the same reason
    /// `max_budget_usd` does: an unattended loop needs a hard stop by default,
    /// not an opt-in one. Raise or disable per-repo in `.lopi/loop.toml`.
    #[serde(default = "default_budget_tokens")]
    pub budget_tokens: u64,
    /// Per-`claude -p` session USD spend ceiling, forwarded as `--max-budget-usd`
    /// (the CLI halts cleanly once reached). `0.0` disables the cap. Defaults to
    /// a conservative non-zero value: `budget_tokens` above only meters the
    /// outer runner's own streamed usage, not a session that fans out into
    /// parallel sub-agents (e.g. a deep-research goal) — this flag is the only
    /// thing that actually caps that spend, since it's CLI-enforced regardless
    /// of what the outer session sees. Raise it per-repo in `.lopi/loop.toml`
    /// for loops that intentionally need expensive sessions.
    #[serde(default = "default_max_budget_usd")]
    pub max_budget_usd: f64,
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
    /// Guardrail precondition — a shell command that must exit `0` before a
    /// loop's very first iteration starts. `None` (the default) means no
    /// precondition, unchanged from before this field existed. A non-empty
    /// command that exits non-zero (or fails to spawn) blocks the loop
    /// entirely with a `GateBlocked` failure rather than burning a retry
    /// attempt on it.
    #[serde(default)]
    pub gate: Option<String>,
    /// Guardrail exit-condition — a shell command checked after each
    /// iteration; exiting `0` ends the loop early as a success, independent
    /// of that iteration's own test score. `None` (the default) relies on
    /// scoring and `max_iterations` alone, unchanged from before this field
    /// existed.
    #[serde(default)]
    pub until: Option<String>,
    /// Policy applied when a loop iteration fails. Defaults to
    /// [`OnFail::Stop`], which reproduces the pre-existing backoff-then-retry
    /// behavior exactly.
    #[serde(default)]
    pub on_fail: OnFail,
    /// A2 (reflection) — durable cross-run learning: capture a learning from
    /// every rejected attempt (rollback-safe) and inject relevance-filtered,
    /// bounded learnings into the next planning prompt. `false` (the default)
    /// keeps behavior identical to before A2. Off-by-default is deliberate — the
    /// §2 discipline flags cross-run reflection until a live three-arm run beats
    /// blind retry by the pre-registered margin.
    #[serde(default)]
    pub reflect_cross_run: bool,
    /// Budget & Guardrail Controls Part 2 — the `[budget]` section: a named
    /// preset plus optional explicit overrides. See
    /// [`resolved_budget`](Self::resolved_budget) for how this combines with
    /// the legacy flat `max_budget_usd`/`budget_tokens`/`permission_*` fields
    /// above. Defaults to the `standard` preset with no overrides.
    #[serde(default)]
    pub budget: BudgetSection,
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
            permission_deny: default_permission_deny(),
            no_progress_limit: default_no_progress_limit(),
            max_iterations: default_max_iterations(),
            budget_tokens: default_budget_tokens(),
            max_budget_usd: default_max_budget_usd(),
            isolation: IsolationMode::default(),
            promote_after: 0,
            trust_ceiling: AutonomyLevel::default(),
            gate: None,
            until: None,
            on_fail: OnFail::default(),
            reflect_cross_run: false,
            budget: BudgetSection::default(),
        }
    }
}

fn default_no_progress_limit() -> u8 {
    3
}

fn default_max_iterations() -> u8 {
    25
}

/// Conservative enough to stop a runaway fan-out (a deep-research-style goal
/// invoking several parallel sub-agents) well short of real damage, generous
/// enough for ordinary plan/implement sessions.
fn default_max_budget_usd() -> f64 {
    3.0
}

/// A few times the single-turn context budget (`AgentRunner::CONTEXT_BUDGET`,
/// 150K) — generous for legitimate multi-attempt work, still a hard ceiling
/// well short of the tens of millions of tokens a genuine runaway fan-out
/// burns through.
fn default_budget_tokens() -> u64 {
    1_000_000
}

/// See `LoopConfig::permission_deny`'s doc comment.
fn default_permission_deny() -> Vec<String> {
    vec!["Workflow".to_string()]
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
    /// Checks: vision-anchor existence, iteration-cap sanity, that a
    /// no-progress limit does not exceed the hard iteration cap, and that
    /// `gate`/`until` are not set to an empty (whitespace-only) command.
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
        if matches!(&self.gate, Some(c) if c.trim().is_empty()) {
            issues.push("gate is set but empty — remove it or give it a real command".into());
        }
        if matches!(&self.until, Some(c) if c.trim().is_empty()) {
            issues.push("until is set but empty — remove it or give it a real command".into());
        }
        issues
    }

    /// Resolve this repo's effective budget from the `[budget]` section:
    /// [`preset`](BudgetSection::preset) first, then any explicit
    /// `max_budget_usd`/`budget_tokens` under `[budget]`, then
    /// `permission_allow` — which always wins over the preset's own deny
    /// list, so re-opening a tool (e.g. `Workflow` for an intentional
    /// fan-out repo) needs only `permission_allow`, never also clearing a
    /// deny list by hand. `LoopConfig::default().resolved_budget()` is a
    /// no-op: it reproduces the pre-existing hardcoded defaults ($3, 1M
    /// tokens, deny `Workflow`) exactly, since `standard` — the default
    /// preset — already carries those same values.
    ///
    /// The legacy flat `max_budget_usd`/`budget_tokens`/`permission_allow`/
    /// `permission_deny` fields above predate `[budget]` and still parse for
    /// backward compatibility, but this method resolves purely from
    /// `[budget]` — a repo that customized only the flat fields should
    /// migrate that customization into `[budget]`.
    #[must_use]
    pub fn resolved_budget(&self) -> ResolvedBudget {
        let mut resolved = self.budget.preset.resolved();
        if let Some(usd) = self.budget.max_budget_usd {
            resolved.usd = usd;
        }
        if let Some(tokens) = self.budget.budget_tokens {
            resolved.tokens = tokens;
        }
        resolved.allow = self.budget.permission_allow.clone();
        resolved.deny.retain(|tool| !resolved.allow.contains(tool));
        resolved
    }
}

#[cfg(test)]
#[path = "loop_config_tests.rs"]
mod tests;
