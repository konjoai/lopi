#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Budget & Guardrail Controls (Parts 0-4) — `LoopConfig`'s legacy flat
//! budget/permission fields and the new `[budget]` section's `resolved_budget()`.
//!
//! Split out of `loop_config_tests.rs` purely to keep that file under the
//! 500-line CI file-size gate as these tests were added; no behavioral
//! difference from being inline.

use super::*;

/// The token-budget progress gate (`ProgressGate`/`effective_budget_tokens`)
/// is a second, independent line of defense catching ordinary retry-loop
/// accumulation — it was off (`0`) by default, so an unattended loop had no
/// protection here either. Same "safe by default" convention as the
/// `[budget]` USD cap.
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

/// Every parallel sub-agent fan-out primitive is denied by default so an
/// unattended loop must explicitly opt in (via `permission_allow`) before it
/// can fan out. `Workflow` alone once ran a session to $25.79; leaving
/// `Task`/`Agent` open blew a $3-capped session to $6.89 the same way.
#[test]
fn loop_config_default_denies_every_fan_out_primitive() {
    let c = LoopConfig::default();
    assert_eq!(c.permission_deny, vec!["Workflow", "Task", "Agent"]);
    assert!(
        c.permission_allow.is_empty(),
        "nothing is pre-approved by default"
    );
}

#[test]
fn loop_config_parses_toml_missing_permission_deny() {
    let c: LoopConfig = toml::from_str("autonomy_level = \"draft_pr\"\n").unwrap();
    assert_eq!(c.permission_deny, vec!["Workflow", "Task", "Agent"]);
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

/// The runtime default: a repo with no `[budget]` section resolves to the
/// conservative `standard` preset — a $1 cap, 1M tokens, and every sub-agent
/// fan-out primitive denied. This is the knob the pool actually wires in.
#[test]
fn loop_config_default_resolved_budget_is_conservative() {
    let r = LoopConfig::default().resolved_budget();
    assert_eq!(r.usd, 1.0);
    assert_eq!(r.tokens, 1_000_000);
    assert_eq!(r.deny, vec!["Workflow", "Task", "Agent"]);
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
        vec!["Workflow", "Task", "Agent"],
        "quick's own deny list is untouched by the usd/token overrides"
    );
}

/// `permission_allow` under `[budget]` re-opens a preset-denied tool without
/// also needing to clear a deny list by hand — and only the named tool, so a
/// repo can re-enable `Workflow` while `Task`/`Agent` stay denied.
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
    assert_eq!(r.deny, vec!["Task", "Agent"], "only Workflow is re-opened");
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
    assert_eq!(c.resolved_budget().usd, 1.0);
}

/// No `[budget].budget_tokens` override at all — nothing to disagree with.
#[test]
fn diverging_budget_tokens_none_when_section_unset() {
    let c = LoopConfig::default();
    assert!(c.diverging_budget_tokens().is_none());
}

/// `[budget].budget_tokens` set but matching the flat field — not a
/// divergence.
#[test]
fn diverging_budget_tokens_none_when_values_match() {
    let c = LoopConfig {
        budget_tokens: 500_000,
        budget: crate::budget_preset::BudgetSection {
            budget_tokens: Some(500_000),
            ..Default::default()
        },
        ..LoopConfig::default()
    };
    assert!(c.diverging_budget_tokens().is_none());
}

/// Regression test for the finding: the flat `budget_tokens` and
/// `[budget].budget_tokens` can silently disagree with nothing guarding it.
/// `resolved_budget()` always wins with the `[budget]` value; this asserts
/// the divergence is at least detected.
#[test]
fn diverging_budget_tokens_detects_mismatch() {
    let c = LoopConfig {
        budget_tokens: 1_000_000,
        budget: crate::budget_preset::BudgetSection {
            budget_tokens: Some(250_000),
            ..Default::default()
        },
        ..LoopConfig::default()
    };
    assert_eq!(c.diverging_budget_tokens(), Some((1_000_000, 250_000)));
    assert_eq!(c.resolved_budget().tokens, 250_000, "[budget] always wins");
}

/// End-to-end through `load_from_repo`: a real `.lopi/loop.toml` with both
/// fields set to disagreeing values must still load successfully (this is
/// a warning, not a hard error) and resolve using `[budget]`.
#[test]
fn load_from_repo_tolerates_diverging_budget_tokens() {
    let dir = std::env::temp_dir().join(format!("lopi_loop_cfg_diverge_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join(".lopi")).unwrap();
    std::fs::write(
        dir.join(".lopi/loop.toml"),
        "budget_tokens = 1000000\n[budget]\nbudget_tokens = 250000\n",
    )
    .unwrap();

    let c = LoopConfig::load_from_repo(&dir).unwrap();
    assert_eq!(c.diverging_budget_tokens(), Some((1_000_000, 250_000)));
    assert_eq!(c.resolved_budget().tokens, 250_000);
}
