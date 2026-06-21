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
