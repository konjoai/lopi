//! Named budget presets (Budget & Guardrail Controls, Part 2).
//!
//! Replaces the single hardcoded default with a tiered, per-task- and
//! remote-overridable budget system. A `[budget]` section in
//! `.lopi/loop.toml` names a [`BudgetPreset`] (quick/standard/deep/unlimited)
//! that sets USD cap, token cap, and the sub-agent fan-out deny list
//! ([`FAN_OUT_DENY`]) together; explicit fields under `[budget]` win over the
//! preset.
//! [`LoopConfig::resolved_budget`](crate::loop_config::LoopConfig::resolved_budget)
//! folds all of that into one [`ResolvedBudget`] the pool wires in one shot.
//! [`BudgetOverride`] is the further per-task/CLI/Telegram layer (Part 3)
//! applied on top of a repo's resolved budget.
//!
//! See `BUDGET_CONTROLS_PLAN.md` Part 2 for the full design rationale and
//! preset table this module is the single source of truth for.

use serde::{Deserialize, Serialize};

/// Every parallel sub-agent fan-out primitive, denied together by the capped
/// presets (`quick`/`standard`). Denying only `Workflow` (the orchestration
/// script) once left `Task`/`Agent` — the direct sub-agent spawn tool, which
/// ships under both names across CLI versions — wide open: a session that
/// fanned out through `Task` burned a $3-capped run to $6.89 (and, earlier,
/// $25.79). The per-session `--max-budget-usd` cap is only a *between-turn*
/// checkpoint, so it cannot cap money the sub-agents of a single turn spend in
/// parallel; denying the fan-out keeps each `claude -p` session a single
/// bounded agent, the one shape that cap reliably governs. Listing a name the
/// running CLI doesn't expose is a harmless no-op, so covering both `Task` and
/// `Agent` is free insurance against a version rename. `deep`/`unlimited`
/// deliberately omit this — they are the intentional-fan-out tiers.
const FAN_OUT_DENY: &[&str] = &["Workflow", "Task", "Agent"];

/// A named budget preset — sets USD cap, token cap, and the fan-out deny list
/// ([`FAN_OUT_DENY`]) together, so a repo "intends" a cost class rather than
/// tuning three independent knobs. The default `Standard` is a conservative
/// $1 cap with a generous 1M-token budget; only `deep`/`unlimited` re-enable
/// parallel sub-agent fan-out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BudgetPreset {
    /// One bugfix / small refactor / single file. $1 cap, 200K tokens,
    /// fan-out denied.
    Quick,
    /// Multi-file feature, test-and-fix loop with a retry or two. $1 cap, 1M
    /// tokens, fan-out denied — the conservative default.
    #[default]
    Standard,
    /// Research + implement, migration, *intentional* fan-out. $10 cap, 5M
    /// tokens, and the only tier below `unlimited` that re-enables sub-agent
    /// fan-out (`Workflow`/`Task`/`Agent`) by default — opt-in by naming this
    /// preset, never by accident.
    Deep,
    /// No cap at all (`0.0`/`0` — the pre-existing "disabled" sentinel).
    /// Requires explicitly setting `preset = "unlimited"`; never the default.
    Unlimited,
}

impl BudgetPreset {
    /// Parse a preset name, case-insensitive. `None` for anything else —
    /// callers should treat an unrecognized preset as a user error, not
    /// silently fall back to a different tier.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "quick" => Some(Self::Quick),
            "standard" => Some(Self::Standard),
            "deep" => Some(Self::Deep),
            "unlimited" => Some(Self::Unlimited),
            _ => None,
        }
    }

    /// The canonical lowercase tag, matching the serde representation and
    /// every CLI/Telegram-facing spelling.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Deep => "deep",
            Self::Unlimited => "unlimited",
        }
    }

    /// `(usd, tokens, deny)` — the single source of truth for the preset
    /// table in `BUDGET_CONTROLS_PLAN.md` Part 2. Kept private: callers use
    /// [`resolved`](Self::resolved), which shapes this into a [`ResolvedBudget`].
    fn table(self) -> (f64, u64, &'static [&'static str]) {
        match self {
            Self::Quick => (1.0, 200_000, FAN_OUT_DENY),
            Self::Standard => (1.0, 1_000_000, FAN_OUT_DENY),
            Self::Deep => (10.0, 5_000_000, &[]),
            Self::Unlimited => (0.0, 0, &[]),
        }
    }

    /// This preset's budget with an empty allow-list — the starting point
    /// [`LoopConfig::resolved_budget`](crate::loop_config::LoopConfig::resolved_budget)
    /// and [`BudgetOverride::apply`] layer explicit overrides on top of.
    #[must_use]
    pub fn resolved(self) -> ResolvedBudget {
        let (usd, tokens, deny) = self.table();
        ResolvedBudget {
            usd,
            tokens,
            allow: Vec::new(),
            deny: deny.iter().map(|s| (*s).to_string()).collect(),
        }
    }
}

/// A fully-resolved budget for one loop/session: USD cap, token cap, and the
/// tool allow/deny lists — one struct instead of the four loose values the
/// pool used to wire separately. `0.0`/`0` are the pre-existing "disabled"
/// sentinels for `usd`/`tokens` respectively.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedBudget {
    /// Per-`claude -p` session USD spend ceiling. `0.0` disables the cap.
    pub usd: f64,
    /// Per-run token budget ceiling metered across the retry loop. `0`
    /// disables the cap.
    pub tokens: u64,
    /// Tool-call patterns pre-approved without prompting.
    pub allow: Vec<String>,
    /// Tool-call patterns always denied — always filtered against `allow`
    /// (allow wins), so re-opening a tool never requires touching both lists.
    pub deny: Vec<String>,
}

/// `[budget]` section of `.lopi/loop.toml` — a named preset plus optional
/// explicit overrides that win over it. Omit everything past `preset` to
/// inherit the preset's own values unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BudgetSection {
    /// Named preset — see [`BudgetPreset`]. Defaults to `standard`.
    #[serde(default)]
    pub preset: BudgetPreset,
    /// Explicit per-`claude -p` session USD cap, overriding the preset's own
    /// value when set. `None` (the default) inherits the preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_budget_usd: Option<f64>,
    /// Explicit per-run token budget, overriding the preset's own value when
    /// set. `None` (the default) inherits the preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u64>,
    /// Tools the preset denies that this repo wants back (e.g. `Workflow`
    /// for an intentional fan-out repo pinned to `quick`/`standard`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_allow: Vec<String>,
}

/// A per-task, per-CLI-invocation, or per-Telegram-chat override applied on
/// top of a repo's already-[`resolved`](crate::loop_config::LoopConfig::resolved_budget)
/// budget (`lopi run --budget`/`--budget-preset`/`--budget-tokens`, Telegram
/// `/budget`). Every field is optional — an empty override changes nothing.
///
/// `preset`, when set, replaces the base's usd/tokens/allow/deny wholesale
/// (a full preset switch); `usd`/`tokens` alone only ever adjust the
/// numbers, never the tool lists — the "fan-out stays opt-in" invariant: a
/// bare per-task USD bump can never re-enable `Workflow` on its own, only
/// naming `deep`/`unlimited` (or the repo's own `permission_allow`) can.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BudgetOverride {
    /// Replace the base budget's preset (and its allow/deny lists) entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<BudgetPreset>,
    /// Override just the USD cap, leaving tool lists untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usd: Option<f64>,
    /// Override just the token cap, leaving tool lists untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
}

impl BudgetOverride {
    /// Whether this override changes nothing — the common case, so callers
    /// can skip re-resolving when no override was supplied.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.preset.is_none() && self.usd.is_none() && self.tokens.is_none()
    }

    /// Apply this override on top of `base` (a repo's already-resolved
    /// budget), returning the final [`ResolvedBudget`] a runner should use.
    #[must_use]
    pub fn apply(&self, base: ResolvedBudget) -> ResolvedBudget {
        let mut resolved = match self.preset {
            Some(preset) => preset.resolved(),
            None => base,
        };
        if let Some(usd) = self.usd {
            resolved.usd = usd;
        }
        if let Some(tokens) = self.tokens {
            resolved.tokens = tokens;
        }
        resolved
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn preset_default_is_standard() {
        assert_eq!(BudgetPreset::default(), BudgetPreset::Standard);
    }

    #[test]
    fn preset_parse_is_case_insensitive_and_total() {
        assert_eq!(BudgetPreset::parse("Deep"), Some(BudgetPreset::Deep));
        assert_eq!(
            BudgetPreset::parse(" UNLIMITED "),
            Some(BudgetPreset::Unlimited)
        );
        assert_eq!(BudgetPreset::parse("nonsense"), None);
    }

    /// The $1 floor: `quick` produces a live, non-zero USD cap — never the
    /// `0.0` "disabled" sentinel — and denies every fan-out primitive.
    #[test]
    fn quick_preset_is_the_one_dollar_floor() {
        let r = BudgetPreset::Quick.resolved();
        assert_eq!(r.usd, 1.0);
        assert_eq!(r.tokens, 200_000);
        assert_eq!(r.deny, vec!["Workflow", "Task", "Agent"]);
    }

    /// The conservative default: `standard` caps a session at $1 with a
    /// generous 1M-token budget and denies every parallel sub-agent fan-out
    /// primitive — the shape that once blew a $3 cap to $6.89.
    #[test]
    fn standard_preset_is_the_conservative_default() {
        let r = BudgetPreset::Standard.resolved();
        assert_eq!(r.usd, 1.0);
        assert_eq!(r.tokens, 1_000_000);
        assert_eq!(r.deny, vec!["Workflow", "Task", "Agent"]);
        assert!(r.allow.is_empty());
    }

    /// `deep`/`unlimited` are the only presets that re-enable sub-agent
    /// fan-out — never by accident, only by naming the preset.
    #[test]
    fn deep_and_unlimited_deny_nothing() {
        assert!(BudgetPreset::Deep.resolved().deny.is_empty());
        assert!(BudgetPreset::Unlimited.resolved().deny.is_empty());
    }

    #[test]
    fn unlimited_preset_is_the_disabled_sentinel() {
        let r = BudgetPreset::Unlimited.resolved();
        assert_eq!(r.usd, 0.0);
        assert_eq!(r.tokens, 0);
    }

    #[test]
    fn budget_section_default_is_standard_preset_no_overrides() {
        let s = BudgetSection::default();
        assert_eq!(s.preset, BudgetPreset::Standard);
        assert!(s.max_budget_usd.is_none());
        assert!(s.budget_tokens.is_none());
        assert!(s.permission_allow.is_empty());
    }

    #[test]
    fn empty_override_is_empty() {
        assert!(BudgetOverride::default().is_empty());
        assert!(!BudgetOverride {
            usd: Some(5.0),
            ..Default::default()
        }
        .is_empty());
    }

    /// A bare USD override must not touch the tool lists — "fan-out stays
    /// opt-in" holds even when a task bumps the dollar cap.
    #[test]
    fn bare_usd_override_does_not_change_tool_lists() {
        let base = BudgetPreset::Standard.resolved();
        let ov = BudgetOverride {
            usd: Some(9.0),
            ..Default::default()
        };
        let r = ov.apply(base);
        assert_eq!(r.usd, 9.0);
        assert_eq!(r.tokens, 1_000_000);
        assert_eq!(r.deny, vec!["Workflow", "Task", "Agent"]);
    }

    /// A preset override replaces the base wholesale, including its deny list.
    #[test]
    fn preset_override_replaces_deny_list() {
        let base = BudgetPreset::Standard.resolved();
        let ov = BudgetOverride {
            preset: Some(BudgetPreset::Deep),
            ..Default::default()
        };
        let r = ov.apply(base);
        assert_eq!(r.usd, 10.0);
        assert!(r.deny.is_empty());
    }

    /// `usd`/`tokens` still win over a preset override supplied in the same
    /// call — e.g. `--budget-preset deep --budget 25` for a one-off session
    /// bigger than even `deep`'s own default.
    #[test]
    fn preset_and_explicit_fields_combine() {
        let base = BudgetPreset::Standard.resolved();
        let ov = BudgetOverride {
            preset: Some(BudgetPreset::Deep),
            usd: Some(25.0),
            tokens: None,
        };
        let r = ov.apply(base);
        assert_eq!(r.usd, 25.0);
        assert_eq!(r.tokens, 5_000_000);
    }

    #[test]
    fn resolved_budget_round_trips_through_json() {
        let r = BudgetPreset::Deep.resolved();
        let json = serde_json::to_string(&r).unwrap();
        let back: ResolvedBudget = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }
}
