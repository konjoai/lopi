//! `lopi loop` — inspect and validate a repo's loop-engineering config.
//!
//! The loop config (`<repo>/.lopi/loop.toml`) is the "loop as code" artifact:
//! autonomy level, intent anchor, enabled skills/rules, permission policy, and
//! halting conditions. These commands let an engineer treat it like any other
//! checked-in config — validate it in CI, inspect the effective values.

use anyhow::Result;
use lopi_core::LoopConfig;
use std::path::Path;

/// Validate `<repo>/.lopi/loop.toml`. Prints each issue and returns a non-zero
/// process exit when the config is invalid, so it can gate CI.
///
/// # Errors
/// Returns `Err` if the file exists but cannot be parsed as TOML.
pub fn validate(repo: &Path) -> Result<()> {
    let cfg = LoopConfig::load_from_repo(repo)?;
    let issues = cfg.validate(repo);
    if issues.is_empty() {
        println!("✓ loop config valid ({})", LoopConfig::REL_PATH);
        return Ok(());
    }
    eprintln!("✗ loop config has {} issue(s):", issues.len());
    for issue in &issues {
        eprintln!("  • {issue}");
    }
    std::process::exit(1);
}

/// Print the effective loop config for a repo. When no file is present the
/// conservative defaults are shown, clearly labelled.
///
/// # Errors
/// Returns `Err` if the file exists but cannot be parsed as TOML.
pub fn show(repo: &Path) -> Result<()> {
    let path = repo.join(LoopConfig::REL_PATH);
    let present = path.exists();
    let cfg = LoopConfig::load_from_repo(repo)?;
    println!("⟲ lopi loop — {}", repo.display());
    if present {
        println!("  config: {}", path.display());
    } else {
        println!("  config: (none — showing defaults)");
    }
    println!();
    println!(
        "  autonomy      {} ({})",
        cfg.autonomy_level.tag(),
        cfg.autonomy_level.label()
    );
    println!(
        "  vision        {}",
        cfg.vision_path
            .as_ref()
            .map_or_else(|| "—".to_string(), |p| p.display().to_string())
    );
    println!("  skills        {}", fmt_list(&cfg.skills_enabled));
    println!("  rules         {}", fmt_list(&cfg.rules_enabled));
    println!("  allow         {}", fmt_list(&cfg.permission_allow));
    println!("  deny          {}", fmt_list(&cfg.permission_deny));
    println!("  no-progress   {} iterations", cfg.no_progress_limit);
    println!("  max-iter      {}", cfg.max_iterations);
    println!(
        "  budget        {}",
        if cfg.budget_tokens == 0 {
            "inherit global".to_string()
        } else {
            format!("{} tokens", cfg.budget_tokens)
        }
    );
    Ok(())
}

/// Render a string list as `all` (empty), or a comma-joined summary.
fn fmt_list(items: &[String]) -> String {
    if items.is_empty() {
        "all".to_string()
    } else {
        items.join(", ")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn show_defaults_for_missing_config() {
        let dir = std::env::temp_dir().join("lopi_loop_cmd_show");
        let _ = std::fs::create_dir_all(&dir);
        // Should not error when no loop.toml exists.
        assert!(show(&dir).is_ok());
    }

    #[test]
    fn validate_ok_for_default_config() {
        let dir = std::env::temp_dir().join("lopi_loop_cmd_validate");
        let _ = std::fs::create_dir_all(&dir);
        assert!(validate(&dir).is_ok());
    }

    #[test]
    fn fmt_list_renders_all_or_joined() {
        assert_eq!(fmt_list(&[]), "all");
        assert_eq!(fmt_list(&["a".into(), "b".into()]), "a, b");
    }
}
