#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Budget & Guardrail Controls (Parts 0-4) — `LoopConfig`'s legacy flat
//! budget/permission fields and the new `[budget]` section's `resolved_budget()`.
//!
//! Split out of `loop_config_tests.rs` purely to keep that file under the
//! 500-line CI file-size gate as these tests were added; no behavioral
//! difference from being inline.

use super::*;

/// A task that fans out into several parallel sub-agents (e.g. a
/// deep-research goal) ran fully uncapped and reached $25.79 for one
/// `claude -p` session before this field existed — the default must be
/// non-zero so an unattended loop is never uncapped by default.
#[test]
fn loop_config_default_max_budget_usd_is_a_conservative_nonzero_cap() {
    let c = LoopConfig::default();
    assert_eq!(c.max_budget_usd, 3.0);
}

/// A config predating this field must still parse, landing on the
/// conservative non-zero default — same convention as the verifier-gate
/// fields (`loop_config_parses_toml_missing_verifier_fields`).
#[test]
fn loop_config_parses_toml_missing_max_budget_usd() {
    let c: LoopConfig = toml::from_str("autonomy_level = \"draft_pr\"\n").unwrap();
    assert_eq!(c.max_budget_usd, 3.0);
}

#[test]
fn loop_config_max_budget_usd_round_trips_through_toml() {
    let c = LoopConfig {
        max_budget_usd: 20.0,
        ..LoopConfig::default()
    };
    let toml_str = toml::to_string(&c).unwrap();
    let back: LoopConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(back.max_budget_usd, 20.0);
}

/// The token-budget progress gate (`ProgressGate`/`effective_budget_tokens`)
/// is a second, independent line of defense catching ordinary retry-loop
/// accumulation — it was off (`0`) by default, so an unattended loop had no
/// protection here either. Same "safe by default" convention as
/// `max_budget_usd`.
#[test]
fn loop_config_default_budget_tokens_is_a_conservative_nonzero_cap() {
    let c = LoopConfig::default();
    assert_eq!(c.budget_tokens, 1_000_000);
}

#[test]
fn loop_config_parses_toml_missing_budget_tokens() {
    let c: LoopConfig = toml::from_str("autonomy_level = \"draft_pr\"\n").unwrap();
    assert_eq!(c.budget_tokens, 1_000_000);
}

/// The multi-agent orchestration primitive that turned a $0 baseline into
/// $25.79 for one session — denied by default so an unattended loop must
/// explicitly opt in (via `permission_allow`) before it can fan out.
#[test]
fn loop_config_default_denies_the_workflow_tool() {
    let c = LoopConfig::default();
    assert_eq!(c.permission_deny, vec!["Workflow".to_string()]);
    assert!(
        c.permission_allow.is_empty(),
        "nothing is pre-approved by default"
    );
}

#[test]
fn loop_config_parses_toml_missing_permission_deny() {
    let c: LoopConfig = toml::from_str("autonomy_level = \"draft_pr\"\n").unwrap();
    assert_eq!(c.permission_deny, vec!["Workflow".to_string()]);
}

/// A repo that wants deep-research-style runs re-enables `Workflow` via
/// `permission_allow` rather than needing to touch `permission_deny` at all.
#[test]
fn loop_config_permission_lists_round_trip_through_toml() {
    let c = LoopConfig {
        permission_allow: vec!["Workflow".to_string()],
        permission_deny: vec![],
        ..LoopConfig::default()
    };
    let toml_str = toml::to_string(&c).unwrap();
    let back: LoopConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(back.permission_allow, vec!["Workflow".to_string()]);
    assert!(back.permission_deny.is_empty());
}

/// Behavioral invariant (Budget & Guardrail Controls Part 2): `standard` ==
/// today's behavior. Existing repos with no `[budget]` section resolve to
/// $3 / 1M tokens / deny `Workflow` — a no-op migration.
#[test]
fn loop_config_default_resolved_budget_is_a_no_op_migration() {
    let r = LoopConfig::default().resolved_budget();
    assert_eq!(r.usd, 3.0);
    assert_eq!(r.tokens, 1_000_000);
    assert_eq!(r.deny, vec!["Workflow".to_string()]);
    assert!(r.allow.is_empty());
}

#[test]
fn resolved_budget_honors_preset_choice() {
    let c = LoopConfig {
        budget: crate::budget_preset::BudgetSection {
            preset: crate::budget_preset::BudgetPreset::Deep,
            ..Default::default()
        },
        ..LoopConfig::default()
    };
    let r = c.resolved_budget();
    assert_eq!(r.usd, 10.0);
    assert_eq!(r.tokens, 5_000_000);
    assert!(r.deny.is_empty(), "deep re-enables Workflow by default");
}

#[test]
fn resolved_budget_explicit_fields_win_over_preset() {
    let c = LoopConfig {
        budget: crate::budget_preset::BudgetSection {
            preset: crate::budget_preset::BudgetPreset::Quick,
            max_budget_usd: Some(2.5),
            budget_tokens: Some(500_000),
            ..Default::default()
        },
        ..LoopConfig::default()
    };
    let r = c.resolved_budget();
    assert_eq!(r.usd, 2.5);
    assert_eq!(r.tokens, 500_000);
    assert_eq!(
        r.deny,
        vec!["Workflow".to_string()],
        "quick's own deny list is untouched by the usd/token overrides"
    );
}

/// `permission_allow` under `[budget]` re-opens a preset-denied tool without
/// also needing to clear a deny list by hand.
#[test]
fn resolved_budget_permission_allow_reopens_preset_deny() {
    let c = LoopConfig {
        budget: crate::budget_preset::BudgetSection {
            preset: crate::budget_preset::BudgetPreset::Standard,
            permission_allow: vec!["Workflow".to_string()],
            ..Default::default()
        },
        ..LoopConfig::default()
    };
    let r = c.resolved_budget();
    assert!(r.deny.is_empty());
    assert_eq!(r.allow, vec!["Workflow".to_string()]);
}

#[test]
fn budget_section_round_trips_through_toml() {
    let c: LoopConfig = toml::from_str(
        "[budget]\npreset = \"deep\"\nmax_budget_usd = 7.5\npermission_allow = [\"Bash\"]\n",
    )
    .unwrap();
    assert_eq!(c.budget.preset, crate::budget_preset::BudgetPreset::Deep);
    assert_eq!(c.budget.max_budget_usd, Some(7.5));
    assert_eq!(c.budget.permission_allow, vec!["Bash".to_string()]);
}

/// A config predating `[budget]` (this sprint's own baseline) must still
/// parse and land on the standard-preset defaults.
#[test]
fn loop_config_parses_toml_missing_budget_section() {
    let c: LoopConfig = toml::from_str("autonomy_level = \"draft_pr\"\n").unwrap();
    assert_eq!(
        c.budget.preset,
        crate::budget_preset::BudgetPreset::Standard
    );
    assert_eq!(c.resolved_budget().usd, 3.0);
}
