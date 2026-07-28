#![allow(clippy::unwrap_used, clippy::panic)]

use super::*;
use crate::config::LimitWindow;

#[test]
fn preset_catalog_has_all_eight_keys() {
    let catalog = preset_catalog();
    for key in PresetKey::all() {
        assert!(catalog.contains_key(&key), "missing preset {key:?}");
    }
    assert_eq!(catalog.len(), 8);
}

#[test]
fn every_preset_evals_include_baseline() {
    for def in preset_catalog().values() {
        assert!(
            def.evals.contains(&baseline_eval()),
            "{:?} preset dropped the baseline eval",
            def.key
        );
    }
}

#[test]
fn preset_descriptions_cover_every_key() {
    let descriptions = preset_descriptions();
    for key in PresetKey::all() {
        assert!(descriptions.contains_key(&key));
    }
}

#[test]
fn legacy_ratchet_alias_resolves_to_gain() {
    assert_eq!(legacy_aliases().get("ratchet"), Some(&PresetKey::Gain));
}

#[test]
fn eval_catalog_starts_with_baseline() {
    assert_eq!(eval_catalog()[0], baseline_eval());
    assert_eq!(eval_catalog().len(), 10);
}

#[test]
fn eval_suites_kcqf_matches_shipped_bundle() {
    let suites = eval_suites();
    assert_eq!(
        suites.get("kcqf"),
        Some(&vec![
            "tests pass",
            "code review",
            "vuln scan",
            "adversarial"
        ])
    );
}

#[test]
fn budget_to_tokens_only_200k_sets_a_cap() {
    assert_eq!(budget_to_tokens(Budget::Auto), None);
    assert_eq!(budget_to_tokens(Budget::None), None);
    assert_eq!(budget_to_tokens(Budget::K200), Some(200_000));
}

#[test]
fn budget_preset_choice_inherit_omits_preset() {
    assert_eq!(BudgetPresetChoice::Inherit.to_budget_preset(), None);
    assert_eq!(
        BudgetPresetChoice::Deep.to_budget_preset(),
        Some(BudgetPreset::Deep)
    );
}

#[test]
fn isolation_choice_inherit_omits_mode() {
    assert_eq!(IsolationChoice::Inherit.to_isolation_mode(), None);
    assert_eq!(
        IsolationChoice::Worktree.to_isolation_mode(),
        Some(IsolationMode::Worktree)
    );
}

#[test]
fn autonomy_to_wire_parses_l_tags_and_omits_junk() {
    assert_eq!(
        autonomy_to_wire(Some("L1")),
        Some(AutonomyLevel::ReportOnly)
    );
    assert_eq!(autonomy_to_wire(Some("L4")), Some(AutonomyLevel::AutoMerge));
    assert_eq!(autonomy_to_wire(None), None);
    assert_eq!(autonomy_to_wire(Some("")), None);
    assert_eq!(autonomy_to_wire(Some("nonsense")), None);
}

#[test]
fn evals_to_acceptance_empty_evals_yields_none() {
    assert_eq!(evals_to_acceptance(&[]), None);
}

#[test]
fn evals_to_acceptance_deterministic_only() {
    let acc = evals_to_acceptance(&[baseline_eval()]).unwrap();
    assert_eq!(acc.checks.len(), 1);
    assert_eq!(acc.checks[0].spec, CheckSpec::ExecutionOk);
}

#[test]
fn evals_to_acceptance_full_set_produces_one_check_per_category() {
    let evals = vec![
        baseline_eval(),
        EvalRef::new("tests pass", EvalTier::ShellTest),
        EvalRef::new("code review", EvalTier::Judge),
        EvalRef::new("beats-best", EvalTier::Judge),
        EvalRef::new("vuln scan", EvalTier::Suite),
        EvalRef::new("adversarial", EvalTier::Suite),
    ];
    let acc = evals_to_acceptance(&evals).unwrap();
    // One execution_ok check (base+test collapse to one), one judge check
    // (both judge names merged into one rubric), one suite check per suite.
    assert_eq!(acc.checks.len(), 4);
    assert_eq!(acc.checks[0].spec, CheckSpec::ExecutionOk);
    match &acc.checks[1].spec {
        CheckSpec::Judge { rubric, .. } => {
            assert_eq!(rubric.name, "ui-evals");
            assert_eq!(rubric.criteria, vec!["code review", "beats-best"]);
        }
        other => panic!("expected Judge check, got {other:?}"),
    }
    assert_eq!(
        acc.checks[2].spec,
        CheckSpec::Suite {
            name: "vuln scan".to_string()
        }
    );
    assert_eq!(
        acc.checks[3].spec,
        CheckSpec::Suite {
            name: "adversarial".to_string()
        }
    );
}

#[test]
fn default_guardrails_are_all_off_and_inherited() {
    let g = default_guardrails();
    assert!(!g.gate);
    assert!(!g.until);
    assert_eq!(g.on_fail, OnFail::Stop);
    assert_eq!(g.budget, Budget::Auto);
    assert_eq!(g.budget_preset, BudgetPresetChoice::Inherit);
    assert_eq!(g.isolation, IsolationChoice::Inherit);
    assert_eq!(g.no_progress_limit, None);
}

#[test]
fn default_cron_matches_daily_2am_raw_expression() {
    let cron = default_cron();
    assert_eq!(cron.freq, CronFreq::Daily);
    assert_eq!(cron.raw, "0 2 * * *");
}

#[test]
fn default_maxx_matches_shipped_defaults() {
    let maxx = default_maxx();
    assert!(!maxx.enabled);
    assert_eq!(maxx.quiet_hours, [23, 7]);
    assert!(maxx.headroom_gate);
    assert_eq!(
        maxx.windows,
        vec![LimitWindow::FiveHour, LimitWindow::SevenDay]
    );
}

#[test]
fn stack_card_round_trips_through_json() {
    let card = StackCard {
        id: "c1".to_string(),
        preset: Some(PresetKey::Implement),
        goal: "add a feature".to_string(),
        alias: Some("implement".to_string()),
        literal: false,
        evals: vec![baseline_eval()],
        status: CardStatus::Idle,
        max_iterations: DEFAULT_MAX_ITERATIONS,
        iteration: None,
        scheduled: false,
        cron: default_cron(),
        guardrails: default_guardrails(),
        config: CardConfig::default(),
        task_id: None,
        tpl: None,
        tpl_kind: None,
        maxx: default_maxx(),
        maxx_entry_id: None,
        block_reason: None,
    };
    let json = serde_json::to_string(&card).unwrap();
    let back: StackCard = serde_json::from_str(&json).unwrap();
    assert_eq!(back, card);
}
