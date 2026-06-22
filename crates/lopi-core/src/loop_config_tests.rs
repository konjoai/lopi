#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn autonomy_default_is_draft_pr() {
    assert_eq!(AutonomyLevel::default(), AutonomyLevel::DraftPr);
}

#[test]
fn autonomy_ranks_are_monotonic() {
    assert_eq!(AutonomyLevel::ReportOnly.rank(), 1);
    assert_eq!(AutonomyLevel::DraftPr.rank(), 2);
    assert_eq!(AutonomyLevel::VerifiedPr.rank(), 3);
    assert_eq!(AutonomyLevel::AutoMerge.rank(), 4);
}

#[test]
fn autonomy_capability_gates() {
    assert!(!AutonomyLevel::ReportOnly.opens_pr());
    assert!(AutonomyLevel::DraftPr.opens_pr());
    assert!(AutonomyLevel::VerifiedPr.requires_verifier());
    assert!(!AutonomyLevel::DraftPr.requires_verifier());
    assert!(AutonomyLevel::AutoMerge.allows_auto_merge());
    assert!(!AutonomyLevel::AutoMerge.requires_human_approval());
    assert!(AutonomyLevel::VerifiedPr.requires_human_approval());
}

#[test]
fn autonomy_from_rank_clamps_to_band() {
    assert_eq!(AutonomyLevel::from_rank(0), AutonomyLevel::ReportOnly);
    assert_eq!(AutonomyLevel::from_rank(1), AutonomyLevel::ReportOnly);
    assert_eq!(AutonomyLevel::from_rank(2), AutonomyLevel::DraftPr);
    assert_eq!(AutonomyLevel::from_rank(3), AutonomyLevel::VerifiedPr);
    assert_eq!(AutonomyLevel::from_rank(4), AutonomyLevel::AutoMerge);
    assert_eq!(AutonomyLevel::from_rank(99), AutonomyLevel::AutoMerge);
}

#[test]
fn autonomy_promoted_steps_up_and_saturates() {
    assert_eq!(AutonomyLevel::ReportOnly.promoted(), AutonomyLevel::DraftPr);
    assert_eq!(AutonomyLevel::DraftPr.promoted(), AutonomyLevel::VerifiedPr);
    assert_eq!(
        AutonomyLevel::VerifiedPr.promoted(),
        AutonomyLevel::AutoMerge
    );
    assert_eq!(
        AutonomyLevel::AutoMerge.promoted(),
        AutonomyLevel::AutoMerge
    );
}

#[test]
fn autonomy_demoted_steps_down_and_saturates() {
    assert_eq!(
        AutonomyLevel::AutoMerge.demoted(),
        AutonomyLevel::VerifiedPr
    );
    assert_eq!(AutonomyLevel::VerifiedPr.demoted(), AutonomyLevel::DraftPr);
    assert_eq!(AutonomyLevel::DraftPr.demoted(), AutonomyLevel::ReportOnly);
    assert_eq!(
        AutonomyLevel::ReportOnly.demoted(),
        AutonomyLevel::ReportOnly
    );
}

#[test]
fn loop_config_trust_levers_default_off() {
    let cfg = LoopConfig::default();
    assert_eq!(cfg.promote_after, 0, "auto-promotion disabled by default");
    assert_eq!(cfg.trust_ceiling, AutonomyLevel::DraftPr);
}

#[test]
fn validate_flags_unreachable_trust_ceiling() {
    let dir = std::env::temp_dir();
    // promote_after set, but ceiling not above the current level → can never fire.
    let cfg = LoopConfig {
        promote_after: 3,
        autonomy_level: AutonomyLevel::DraftPr,
        trust_ceiling: AutonomyLevel::DraftPr,
        ..LoopConfig::default()
    };
    let issues = cfg.validate(&dir);
    assert!(
        issues.iter().any(|i| i.contains("trust_ceiling")),
        "expected a trust_ceiling issue, got: {issues:?}"
    );
}

#[test]
fn validate_passes_with_headroom() {
    let dir = std::env::temp_dir();
    let cfg = LoopConfig {
        promote_after: 3,
        autonomy_level: AutonomyLevel::DraftPr,
        trust_ceiling: AutonomyLevel::VerifiedPr,
        ..LoopConfig::default()
    };
    assert!(cfg
        .validate(&dir)
        .iter()
        .all(|i| !i.contains("trust_ceiling")));
}

#[test]
fn loop_config_trust_levers_round_trip_through_toml() {
    let cfg = LoopConfig {
        promote_after: 5,
        trust_ceiling: AutonomyLevel::VerifiedPr,
        ..LoopConfig::default()
    };
    let toml = toml::to_string_pretty(&cfg).unwrap();
    let back: LoopConfig = toml::from_str(&toml).unwrap();
    assert_eq!(back.promote_after, 5);
    assert_eq!(back.trust_ceiling, AutonomyLevel::VerifiedPr);
}

#[test]
fn autonomy_parse_accepts_names_and_tags() {
    assert_eq!(
        AutonomyLevel::parse("report_only"),
        Some(AutonomyLevel::ReportOnly)
    );
    assert_eq!(AutonomyLevel::parse("L2"), Some(AutonomyLevel::DraftPr));
    assert_eq!(
        AutonomyLevel::parse("  verified_pr "),
        Some(AutonomyLevel::VerifiedPr)
    );
    assert_eq!(
        AutonomyLevel::parse("AutoMerge"),
        Some(AutonomyLevel::AutoMerge)
    );
    assert_eq!(AutonomyLevel::parse("nonsense"), None);
}

#[test]
fn autonomy_tag_and_label() {
    assert_eq!(AutonomyLevel::ReportOnly.tag(), "L1");
    assert_eq!(AutonomyLevel::AutoMerge.label(), "Auto-merge");
    assert_eq!(AutonomyLevel::all().len(), 4);
}

#[test]
fn autonomy_serde_is_snake_case() {
    let json = serde_json::to_string(&AutonomyLevel::VerifiedPr).unwrap();
    assert_eq!(json, "\"verified_pr\"");
    let back: AutonomyLevel = serde_json::from_str("\"auto_merge\"").unwrap();
    assert_eq!(back, AutonomyLevel::AutoMerge);
}

#[test]
fn loop_config_default_is_conservative() {
    let c = LoopConfig::default();
    assert_eq!(c.autonomy_level, AutonomyLevel::DraftPr);
    assert_eq!(c.no_progress_limit, 3);
    assert_eq!(c.max_iterations, 25);
    assert!(c.skills_enabled.is_empty());
}

#[test]
fn loop_config_load_missing_file_yields_default() {
    let dir = std::env::temp_dir().join("lopi_loop_cfg_missing");
    let _ = std::fs::create_dir_all(&dir);
    let c = LoopConfig::load_from_repo(&dir).unwrap();
    assert_eq!(c, LoopConfig::default());
}

#[test]
fn loop_config_round_trips_through_toml() {
    let c = LoopConfig {
        autonomy_level: AutonomyLevel::VerifiedPr,
        vision_path: Some(PathBuf::from("VISION.md")),
        skills_enabled: vec!["konjo-quality".into()],
        no_progress_limit: 2,
        max_iterations: 10,
        budget_tokens: 50_000,
        ..LoopConfig::default()
    };
    let toml_str = toml::to_string(&c).unwrap();
    let back: LoopConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(c, back);
}

#[test]
fn loop_config_default_self_prompt_is_direct() {
    assert_eq!(
        LoopConfig::default().self_prompt,
        crate::SelfPromptStrategy::Direct
    );
}

#[test]
fn save_to_repo_round_trips_defaults_including_self_prompt() {
    let dir = std::env::temp_dir().join("lopi_loop_cfg_save_default");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = LoopConfig::default();
    cfg.save_to_repo(&dir).unwrap();
    assert!(dir.join(LoopConfig::REL_PATH).exists());
    let back = LoopConfig::load_from_repo(&dir).unwrap();
    assert_eq!(cfg, back);
}

#[test]
fn loop_config_default_does_not_escalate() {
    assert!(!LoopConfig::default().escalate_strategy);
}

#[test]
fn save_to_repo_persists_escalation_flag() {
    let dir = std::env::temp_dir().join("lopi_loop_cfg_save_escalate");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = LoopConfig {
        self_prompt: crate::SelfPromptStrategy::Reflexion,
        escalate_strategy: true,
        ..LoopConfig::default()
    };
    cfg.save_to_repo(&dir).unwrap();
    let back = LoopConfig::load_from_repo(&dir).unwrap();
    assert!(back.escalate_strategy);
    assert_eq!(back.self_prompt, crate::SelfPromptStrategy::Reflexion);
}

#[test]
fn save_to_repo_persists_a_changed_strategy() {
    let dir = std::env::temp_dir().join("lopi_loop_cfg_save_strategy");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = LoopConfig {
        self_prompt: crate::SelfPromptStrategy::Reflexion,
        ..LoopConfig::default()
    };
    cfg.save_to_repo(&dir).unwrap();
    let back = LoopConfig::load_from_repo(&dir).unwrap();
    assert_eq!(back.self_prompt, crate::SelfPromptStrategy::Reflexion);
}

#[test]
fn validate_flags_missing_vision() {
    let dir = std::env::temp_dir().join("lopi_loop_cfg_validate");
    let _ = std::fs::create_dir_all(&dir);
    let c = LoopConfig {
        vision_path: Some(PathBuf::from("nope/VISION.md")),
        ..LoopConfig::default()
    };
    let issues = c.validate(&dir);
    assert!(issues.iter().any(|i| i.contains("vision_path")));
}

#[test]
fn validate_flags_zero_iterations_and_bad_no_progress() {
    let dir = std::env::temp_dir();
    let c = LoopConfig {
        max_iterations: 0,
        no_progress_limit: 5,
        ..LoopConfig::default()
    };
    let issues = c.validate(&dir);
    assert!(issues.iter().any(|i| i.contains("max_iterations is 0")));
    assert!(issues.iter().any(|i| i.contains("no_progress_limit")));
}

#[test]
fn validate_clean_config_has_no_issues() {
    let dir = std::env::temp_dir();
    let c = LoopConfig::default();
    assert!(c.validate(&dir).is_empty());
}
