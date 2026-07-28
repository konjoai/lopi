//! Loop-stack domain types — the Rust port of the stack/card model every
//! other lopi client (web `stores/stack.ts`, macOS/iOS `LopiStacksKit`)
//! already ships. Sprint T0 (TUI Client Foundation) exists specifically so
//! the TUI becomes the fourth client of *this* module instead of a
//! fourth independent reimplementation — see `LEDGER.md` for the sprint's
//! one-way-door decisions.
//!
//! Canonical port source is `StackTypes.swift` (`packages/LopiStacksKit/
//! Sources/LopiStacksKit/StackTypes.swift`), with `web/src/lib/stores/
//! stack.ts` as tie-breaker where the Swift snapshot is stale or silent —
//! confirmed stale on `Guardrails` (missing `budget_preset`/`budget_usd`/
//! `isolation`/`no_progress_limit`) and `CardConfig` (missing
//! `permission_mode`), both of which this port follows the TS shape for
//! since those fields are load-bearing for `cardToTaskPayload` parity.
//!
//! `OnFail` and `LimitWindow` are deliberately *not* redefined here —
//! [`crate::loop_config::OnFail`] and [`crate::config::LimitWindow`]
//! already exist and already match the Swift/TS wire tags exactly.
//!
//! This module owns pure data + pure conversion helpers only (evals →
//! acceptance, budget-preset → tokens, autonomy-string → [`AutonomyLevel`]).
//! The wire-payload builders that target the *real*
//! `lopi_ui::web::types::CreateTaskRequest` type live in `lopi-ui`
//! (`crates/lopi-ui/src/client/stack_payload.rs`), not here — `lopi-core`
//! cannot depend on `lopi-ui` (that dependency already runs the other way),
//! so a module that needs to *return* `CreateTaskRequest` cannot live in
//! this crate without introducing a cycle or a banned intermediate DTO.
//! See that module's doc comment and `LEDGER.md` for the full reasoning.

use crate::acceptance::{Acceptance, AcceptanceCheck, CheckSpec};
use crate::autonomy::AutonomyLevel;
use crate::budget_preset::BudgetPreset;
use crate::loop_config::{IsolationMode, OnFail};
use crate::task::Rubric;
use crate::EvalTier;
use serde::{Deserialize, Serialize};

#[path = "stack_catalog.rs"]
mod stack_catalog;
pub use stack_catalog::{
    baseline_eval, eval_catalog, eval_suites, legacy_aliases, preset_catalog, preset_descriptions,
    preset_keys, PresetDef, PresetKey,
};

#[path = "stack_schedule.rs"]
mod stack_schedule;
pub use stack_schedule::{default_cron, default_maxx, AmPm, CronConfig, CronFreq, Dow, MaxxConfig};

/// One eval attached to a card — a name plus the tier it runs at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalRef {
    /// Eval name, e.g. `"tests pass"` or `"code review"`.
    pub name: String,
    /// Tier this eval runs at.
    pub tier: EvalTier,
}

impl EvalRef {
    /// Build an `EvalRef` from a name and tier.
    #[must_use]
    pub fn new(name: impl Into<String>, tier: EvalTier) -> Self {
        Self {
            name: name.into(),
            tier,
        }
    }
}

/// Legacy per-run token-budget preset (`StackTypes.swift:67-71`). Distinct
/// from the newer [`BudgetPreset`]/[`BudgetPresetChoice`] system — `Budget`
/// only ever sets `budget_tokens` directly; the newer system also governs
/// USD cap and the sub-agent fan-out allow/deny list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    /// No hard cap — `budget_tokens` is omitted from the wire payload.
    #[default]
    Auto,
    /// A 200,000-token hard cap.
    #[serde(rename = "200k")]
    K200,
    /// No hard cap, explicitly chosen (same wire behavior as `Auto`).
    None,
}

/// Map a legacy [`Budget`] choice to the token cap it sets, `None` when the
/// choice sets no hard cap at all (`stack.ts::budgetToTokens`).
#[must_use]
pub fn budget_to_tokens(budget: Budget) -> Option<u64> {
    (budget == Budget::K200).then_some(200_000)
}

/// The real [`BudgetPreset`] vocabulary, plus the `inherit` sentinel meaning
/// "no preset chosen — omit `budget_override.preset`, the repo's
/// `.lopi/loop.toml` `[budget]` section governs."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BudgetPresetChoice {
    /// No override — the repo's configured preset governs.
    #[default]
    Inherit,
    /// [`BudgetPreset::Quick`].
    Quick,
    /// [`BudgetPreset::Standard`].
    Standard,
    /// [`BudgetPreset::Deep`].
    Deep,
    /// [`BudgetPreset::Unlimited`].
    Unlimited,
}

impl BudgetPresetChoice {
    /// The real [`BudgetPreset`] this choice selects, `None` for `Inherit`.
    #[must_use]
    pub fn to_budget_preset(self) -> Option<BudgetPreset> {
        match self {
            Self::Inherit => None,
            Self::Quick => Some(BudgetPreset::Quick),
            Self::Standard => Some(BudgetPreset::Standard),
            Self::Deep => Some(BudgetPreset::Deep),
            Self::Unlimited => Some(BudgetPreset::Unlimited),
        }
    }
}

/// Per-run isolation-mode override, plus the `inherit` sentinel meaning
/// "omit `isolation`, the repo's `.lopi/loop.toml` isolation mode governs."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IsolationChoice {
    /// No override — the repo's configured isolation mode governs.
    #[default]
    Inherit,
    /// [`IsolationMode::Branch`].
    Branch,
    /// [`IsolationMode::Worktree`].
    Worktree,
}

impl IsolationChoice {
    /// The real [`IsolationMode`] this choice selects, `None` for `Inherit`.
    #[must_use]
    pub fn to_isolation_mode(self) -> Option<IsolationMode> {
        match self {
            Self::Inherit => None,
            Self::Branch => Some(IsolationMode::Branch),
            Self::Worktree => Some(IsolationMode::Worktree),
        }
    }
}

/// A card's halting/budget/isolation guardrails. Follows the current
/// `web/src/lib/stores/stack.ts` `Guardrails` shape (the Swift snapshot is
/// stale here — see this module's doc comment).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Guardrails {
    /// Whether a gate command must pass before the loop proceeds.
    pub gate: bool,
    /// The gate command text.
    pub gate_cmd: String,
    /// Whether the loop retries until a command passes.
    pub until: bool,
    /// The until command text.
    pub until_cmd: String,
    /// Policy applied after a failed iteration.
    pub on_fail: OnFail,
    /// Legacy token-budget preset.
    pub budget: Budget,
    /// Real budget-preset override (USD/tokens/fan-out list).
    pub budget_preset: BudgetPresetChoice,
    /// USD cap override, independent of `budget_preset`.
    pub budget_usd: Option<f64>,
    /// Per-run isolation-mode override.
    pub isolation: IsolationChoice,
    /// Per-run no-progress-limit override.
    pub no_progress_limit: Option<u8>,
}

/// A fresh card's guardrails: nothing on, everything inherited.
#[must_use]
pub fn default_guardrails() -> Guardrails {
    Guardrails {
        gate: false,
        gate_cmd: String::new(),
        until: false,
        until_cmd: String::new(),
        on_fail: OnFail::Stop,
        budget: Budget::Auto,
        budget_preset: BudgetPresetChoice::Inherit,
        budget_usd: None,
        isolation: IsolationChoice::Inherit,
        no_progress_limit: None,
    }
}

/// Per-card overrides of the pane's defaults
/// (`StackTypes.swift:181-196`, plus `permission_mode` — present on the
/// current TS `CardConfig` but missing from the stale Swift snapshot).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardConfig {
    /// Model override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Effort override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Repo override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Branch override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Autonomy-level override, `"L1".."L4"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy: Option<String>,
    /// Permission-mode override (the CLI's `--permission-mode` literal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
}

/// A card's lifecycle state (`StackTypes.swift:206-208`). Distinct from
/// [`crate::task::TaskStatus`] — a dispatched card's live status is the
/// richer `TaskStatus`; `CardStatus` is only the smaller pre/post-dispatch
/// set the builder UI itself needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    /// Not yet given a goal.
    Draft,
    /// Has a goal, not yet queued.
    Idle,
    /// Submitted, waiting to run.
    Queued,
    /// Currently executing.
    Running,
    /// Finished successfully.
    Done,
    /// Blocked on an unmet precondition.
    Blocked,
}

/// Whether a card's `tpl` field names a single prompt or a nested stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TplKind {
    /// A single reusable prompt template.
    Prompt,
    /// A nested, reusable stack template.
    Stack,
}

/// A card's live iteration counter (`StackTypes.swift:228-236`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationProgress {
    /// The attempt currently running (or last run).
    pub current: u32,
    /// The configured ceiling.
    pub total: u32,
}

/// `0` — the backend's "no ceiling" sentinel, and a fresh card's default.
pub const DEFAULT_MAX_ITERATIONS: u32 = 0;

/// One loop in a stack — the full port of `StackTypes.swift`'s `StackCard`
/// (`StackTypes.swift:239-302`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackCard {
    /// Stable card id (also sent as `client_ref` on submission).
    pub id: String,
    /// The preset this card was built from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<PresetKey>,
    /// The goal text.
    pub goal: String,
    /// Display alias, e.g. `"lint-sweep"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// `true` when the goal was typed literally (no alias/preset resolved).
    pub literal: bool,
    /// Evals attached to this card.
    pub evals: Vec<EvalRef>,
    /// Lifecycle status.
    pub status: CardStatus,
    /// Hard iteration ceiling. `0` = infinite (the backend sentinel).
    pub max_iterations: u32,
    /// Live iteration progress, once running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iteration: Option<IterationProgress>,
    /// Whether this card runs on a cron schedule.
    pub scheduled: bool,
    /// The card's cron configuration.
    pub cron: CronConfig,
    /// Halting/budget/isolation guardrails.
    pub guardrails: Guardrails,
    /// Per-card config overrides.
    pub config: CardConfig,
    /// The dispatched task's id, once submitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// A named template this card was instantiated from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpl: Option<String>,
    /// Whether `tpl` names a prompt or a nested stack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpl_kind: Option<TplKind>,
    /// MAXX (autonomous continuation) settings.
    #[serde(default = "default_maxx")]
    pub maxx: MaxxConfig,
    /// The persisted MAXX entry id backing this card, once registered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxx_entry_id: Option<String>,
    /// Why this card is blocked, when `status == Blocked`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
}

/// Pane-level defaults a card's `config` overrides fall back to
/// (`web/src/lib/stores/stack.ts::PaneDefaults`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneDefaults {
    /// Default model (`"auto"` means no override).
    pub model: String,
    /// Default effort.
    pub effort: String,
    /// Default repo.
    pub repo: String,
    /// Default branch, if any.
    pub branch: Option<String>,
    /// Default autonomy (`"L1".."L4"`), if any.
    pub autonomy: Option<String>,
    /// Default permission mode, if any.
    pub permission_mode: Option<String>,
}

/// A bare-prompt submission from the always-focused input bar, before pane
/// defaults are merged in (`web/src/lib/stores/stack.ts::PaneLaunch`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaneLaunch {
    /// The goal text.
    pub goal: String,
    /// Target repo.
    pub repo: String,
    /// Priority (`"low"`/`"normal"`/`"high"`/`"critical"`); empty defers to
    /// `"normal"`.
    pub priority: Option<String>,
    /// Model override (`"auto"` means no override).
    pub model: Option<String>,
    /// Effort override.
    pub effort: Option<String>,
    /// Branch override.
    pub branch: Option<String>,
    /// Permission-mode override.
    pub permission_mode: Option<String>,
}

/// Compile a card's evals into a real [`Acceptance`] goal
/// (`stack.ts::evalsToAcceptance`). Returns `None` when the card carries no
/// checks at all (an empty evals list — never happens for a card built via
/// the composer, since every card gets at least the baseline eval, but a
/// hand-constructed `StackCard` could have one).
#[must_use]
pub fn evals_to_acceptance(evals: &[EvalRef]) -> Option<Acceptance> {
    let mut checks = Vec::new();

    let has_deterministic = evals
        .iter()
        .any(|e| matches!(e.tier, EvalTier::ExecutionOk | EvalTier::ShellTest));
    if has_deterministic {
        checks.push(AcceptanceCheck::new(CheckSpec::ExecutionOk));
    }

    let judge_names: Vec<String> = evals
        .iter()
        .filter(|e| e.tier == EvalTier::Judge)
        .map(|e| e.name.clone())
        .collect();
    if !judge_names.is_empty() {
        checks.push(AcceptanceCheck::new(CheckSpec::Judge {
            rubric: Rubric {
                name: "ui-evals".to_string(),
                criteria: judge_names,
            },
            metric: None,
        }));
    }

    for suite in evals.iter().filter(|e| e.tier == EvalTier::Suite) {
        checks.push(AcceptanceCheck::new(CheckSpec::Suite {
            name: suite.name.clone(),
        }));
    }

    (!checks.is_empty()).then(|| Acceptance::new(checks))
}

/// Map an `"L1".."L4"` autonomy choice to the real [`AutonomyLevel`] the
/// wire payload carries. Returns `None` for `None`/empty/unrecognized input
/// — every one of those cases means "omit the field, the repo's
/// `.lopi/loop.toml` governs" (`stack.ts::autonomyToWire`'s contract).
#[must_use]
pub fn autonomy_to_wire(level: Option<&str>) -> Option<AutonomyLevel> {
    level.and_then(AutonomyLevel::parse)
}

#[cfg(test)]
#[path = "stack_tests.rs"]
mod tests;
