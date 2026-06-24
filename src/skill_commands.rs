//! `lopi skill promote` — the lesson→skill promotion trigger (Pentad M2.3).
//!
//! Pulls a repo's recurring lessons from the memory store, detects which recur
//! often enough to promote, and drafts them into `.lopi/skills-pending/` for
//! review. Drafts never auto-activate (see [`lopi_skill::promote_lessons`]); an
//! operator reviews them and moves the approved ones into `.lopi/skills/`.
//!
//! [`promote`] does the IO; [`format_report`] renders the summary purely, so the
//! output is unit-testable without a database.

use anyhow::Result;
use clap::Subcommand;
use lopi_memory::MemoryStore;
use lopi_skill::PromotionReport;
use std::path::{Path, PathBuf};

/// `lopi skill` subcommands.
#[derive(Subcommand)]
pub enum SkillCmd {
    /// Detect recurring lessons and draft them into `.lopi/skills-pending/`.
    Promote {
        /// Repository to scan (its lessons are read from the store).
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        /// Minimum occurrences before a lesson cluster is promoted.
        #[arg(long, default_value = "3")]
        min: usize,
        /// How many recent lessons to scan.
        #[arg(long, default_value = "200")]
        limit: i64,
    },
}

/// Detect + draft promotable lessons for `repo`, returning a printable summary.
///
/// `min_occurrences` is the recurrence threshold; `limit` caps how many recent
/// lessons are scanned.
///
/// # Errors
/// Returns `Err` if the store cannot be opened/queried or a draft cannot be
/// written.
pub async fn promote(
    repo: &Path,
    db_path: PathBuf,
    min_occurrences: usize,
    limit: i64,
) -> Result<String> {
    // Lessons are keyed by the repo path the runner recorded them under, which
    // is canonical/absolute; match that so the query actually finds them.
    let repo_key = repo
        .canonicalize()
        .unwrap_or_else(|_| repo.to_path_buf())
        .display()
        .to_string();
    let store = MemoryStore::open(db_path).await?;
    let rows = store.load_lessons(&repo_key, limit).await?;
    let scanned = rows.len();
    let lessons: Vec<(String, String)> =
        rows.into_iter().map(|r| (r.category, r.content)).collect();

    // Drafting writes files — keep the blocking fs work off the async runtime.
    let repo_owned = repo.to_path_buf();
    let report = tokio::task::spawn_blocking(move || {
        lopi_skill::promote_lessons(&repo_owned, &lessons, min_occurrences)
    })
    .await?
    .map_err(|e| anyhow::anyhow!("writing skill drafts: {e}"))?;

    Ok(format_report(repo, scanned, min_occurrences, &report))
}

/// Render a promotion sweep as a printable block. Pure — no IO — so the summary
/// is testable without a store.
#[must_use]
pub fn format_report(
    repo: &Path,
    scanned: usize,
    min_occurrences: usize,
    report: &PromotionReport,
) -> String {
    let mut out = format!(
        "⟲ lopi skill promote — {}\n  scanned: {scanned} lesson(s) (threshold ≥{min_occurrences})\n",
        repo.display()
    );
    out.push_str(&format!(
        "  drafted: {} → {}\n",
        report.drafted.len(),
        lopi_skill::PENDING_SKILLS_DIR
    ));
    for name in &report.drafted {
        out.push_str(&format!("    • {name}\n"));
    }
    if !report.skipped.is_empty() {
        out.push_str(&format!(
            "  skipped: {} (already drafted or active)\n",
            report.skipped.len()
        ));
    }
    if report.drafted.is_empty() {
        out.push_str("  nothing new to promote.\n");
    } else {
        out.push_str("  Review drafts, then move approved ones into .lopi/skills/ to activate.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::{format_report, promote};
    use lopi_memory::MemoryStore;
    use lopi_skill::PromotionReport;
    use std::path::Path;
    use tempfile::TempDir;

    /// Exercises the full `promote` path: seed recurring lessons in a real
    /// store, then assert it loads, detects, drafts to the pending dir, and
    /// reports — which also kills the "replace body with a constant" mutant.
    #[tokio::test]
    async fn promote_drafts_recurring_lessons_from_the_store() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let db = repo.join("lopi.db");
        // `promote` keys lessons by the canonical repo path — seed under the same.
        let key = repo.canonicalize().unwrap().display().to_string();

        let store = MemoryStore::open(&db).await.unwrap();
        for content in [
            "Run the tests after refactor",
            "after refactor run tests",
            "TESTS, after refactor, run!",
        ] {
            store
                .save_lesson(&key, "recovery", content, None, 1.0)
                .await
                .unwrap();
        }
        drop(store); // release the write pool before `promote` reopens the db

        let out = promote(repo, db, 3, 200).await.unwrap();
        assert!(out.contains("⟲ lopi skill promote"));
        assert!(out.contains("scanned: 3"));
        assert!(out.contains("drafted: 1"));
        assert!(out.contains("learned-after-refactor-tests"));
        assert!(
            repo.join(".lopi/skills-pending/learned-after-refactor-tests/SKILL.md")
                .is_file(),
            "draft written to the pending dir"
        );
    }

    #[test]
    fn report_lists_drafted_and_skipped() {
        let report = PromotionReport {
            drafted: vec!["learned-a-b".into()],
            skipped: vec!["learned-c-d".into()],
        };
        let out = format_report(Path::new("/repo"), 12, 3, &report);
        assert!(out.contains("scanned: 12 lesson(s) (threshold ≥3)"));
        assert!(out.contains("drafted: 1 → .lopi/skills-pending"));
        assert!(out.contains("• learned-a-b"));
        assert!(out.contains("skipped: 1"));
        assert!(out.contains("move approved ones into .lopi/skills/"));
    }

    #[test]
    fn report_says_nothing_when_empty() {
        let out = format_report(Path::new("/repo"), 4, 3, &PromotionReport::default());
        assert!(out.contains("drafted: 0"));
        assert!(out.contains("nothing new to promote"));
        assert!(!out.contains("skipped"));
    }
}
