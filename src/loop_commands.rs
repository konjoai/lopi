//! `lopi loop` — inspect and validate a repo's loop-engineering config.
//!
//! The loop config (`<repo>/.lopi/loop.toml`) is the "loop as code" artifact:
//! autonomy level, intent anchor, enabled skills/rules, permission policy, and
//! halting conditions. These commands let an engineer treat it like any other
//! checked-in config — validate it in CI, inspect the effective values.
//!
//! Both entry points are pure, return-value functions: [`check`] returns the
//! validation issues and [`render`] returns the printable block. The thin
//! `main.rs` dispatch does the printing and the non-zero exit, so the behaviour
//! here is fully unit-testable without capturing stdout or aborting the process.

use anyhow::Result;
use lopi_core::LoopConfig;
use std::path::Path;

/// Load and validate `<repo>/.lopi/loop.toml`, returning the list of issues
/// (empty when valid). Backs `lopi loop validate`.
///
/// # Errors
/// Returns `Err` if the file exists but cannot be parsed as TOML.
pub fn check(repo: &Path) -> Result<Vec<String>> {
    let cfg = LoopConfig::load_from_repo(repo)?;
    Ok(cfg.validate(repo))
}

/// Render the effective loop config for a repo as a printable block. When no
/// file is present the conservative defaults are shown, clearly labelled.
/// Backs `lopi loop show`.
///
/// # Errors
/// Returns `Err` if the file exists but cannot be parsed as TOML.
pub fn render(repo: &Path) -> Result<String> {
    let path = repo.join(LoopConfig::REL_PATH);
    let cfg = LoopConfig::load_from_repo(repo)?;
    let source = if path.exists() {
        path.display().to_string()
    } else {
        "(none — showing defaults)".to_string()
    };
    let vision = cfg
        .vision_path
        .as_ref()
        .map_or_else(|| "—".to_string(), |p| p.display().to_string());
    let budget = if cfg.budget_tokens == 0 {
        "inherit global".to_string()
    } else {
        format!("{} tokens", cfg.budget_tokens)
    };
    Ok(format!(
        "⟲ lopi loop — {repo}\n  config: {source}\n\n  \
         autonomy      {tag} ({label})\n  \
         vision        {vision}\n  \
         skills        {skills}\n  \
         rules         {rules}\n  \
         allow         {allow}\n  \
         deny          {deny}\n  \
         no-progress   {nop} iterations\n  \
         max-iter      {maxi}\n  \
         budget        {budget}\n",
        repo = repo.display(),
        tag = cfg.autonomy_level.tag(),
        label = cfg.autonomy_level.label(),
        skills = fmt_list(&cfg.skills_enabled),
        rules = fmt_list(&cfg.rules_enabled),
        allow = fmt_list(&cfg.permission_allow),
        deny = fmt_list(&cfg.permission_deny),
        nop = cfg.no_progress_limit,
        maxi = cfg.max_iterations,
    ))
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

    /// A unique, freshly-created temp dir per test name.
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lopi_loop_cmd_{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a `.lopi/loop.toml` with the given body into `dir`.
    fn write_config(dir: &Path, body: &str) {
        std::fs::create_dir_all(dir.join(".lopi")).unwrap();
        std::fs::write(dir.join(LoopConfig::REL_PATH), body).unwrap();
    }

    #[test]
    fn check_is_empty_for_default_config() {
        let dir = temp_dir("check_default");
        assert!(check(&dir).unwrap().is_empty(), "default config is valid");
    }

    #[test]
    fn check_reports_issues_for_bad_config() {
        let dir = temp_dir("check_bad");
        write_config(&dir, "max_iterations = 0\nno_progress_limit = 5\n");
        let issues = check(&dir).unwrap();
        assert!(!issues.is_empty(), "max_iterations=0 must be flagged");
        assert!(issues.iter().any(|i| i.contains("max_iterations")));
    }

    #[test]
    fn render_contains_all_fields_with_defaults() {
        let dir = temp_dir("render");
        let out = render(&dir).unwrap();
        assert!(out.contains("lopi loop"));
        assert!(
            out.contains("autonomy      L2 (Draft PR)"),
            "default autonomy"
        );
        // Pre-existing gap (predates Budget & Guardrail Controls): this
        // assertion expected "inherit global" (budget_tokens == 0), but
        // 0c5343e changed the default to a non-zero 1_000_000 without
        // updating this test — `render()`'s own `budget_tokens == 0` check
        // was correct, only this expectation was stale.
        assert!(out.contains("budget        1000000 tokens"));
        assert!(out.contains("max-iter      25"));
        assert!(out.contains("(none — showing defaults)"));
    }

    #[test]
    fn render_reflects_a_present_config() {
        let dir = temp_dir("render_present");
        write_config(
            &dir,
            "autonomy_level = \"verified_pr\"\nbudget_tokens = 50000\n",
        );
        let out = render(&dir).unwrap();
        assert!(out.contains("L3 (Verified PR)"));
        assert!(out.contains("50000 tokens"));
        assert!(!out.contains("(none — showing defaults)"));
    }

    #[test]
    fn fmt_list_renders_all_or_joined() {
        assert_eq!(fmt_list(&[]), "all");
        assert_eq!(fmt_list(&["a".into(), "b".into()]), "a, b");
    }
}
