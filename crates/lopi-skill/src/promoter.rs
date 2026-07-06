//! Writing promotion drafts to a pending-review directory (Pentad M2.3).
//!
//! [`promote_lessons`] is the side-effecting half of the Ratchet: it detects
//! recurring lessons and writes `SKILL.md` drafts to the repo's
//! [`PENDING_SKILLS_DIR`]. Crucially it **never** writes into an active skills
//! directory, so a draft cannot auto-activate — the
//! [`SkillRegistry`](crate::SkillRegistry) only loads `.claude/skills` and
//! `.lopi/skills`. A human reviews a draft and moves it into place; that move is
//! the approval gate. The sweep is idempotent: a candidate whose draft (or an
//! active skill of the same name) already exists is skipped.

use crate::promote::{draft_skill_md, draft_skill_name, promotion_candidates};
use std::path::{Path, PathBuf};

/// Repo-relative directory where promotion drafts await human review.
pub const PENDING_SKILLS_DIR: &str = ".lopi/skills-pending";

/// Active skill roots a draft must not collide with (kept in sync with the
/// pool's loader). A candidate whose name already exists here is skipped.
const ACTIVE_SKILL_DIRS: [&str; 2] = [".claude/skills", ".lopi/skills"];

/// What a [`promote_lessons`] sweep produced.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PromotionReport {
    /// Names of skills newly drafted into the pending dir this sweep.
    pub drafted: Vec<String>,
    /// Names skipped because a draft or active skill already exists.
    pub skipped: Vec<String>,
}

/// Detect recurring lessons in `repo` and write `SKILL.md` drafts to its pending
/// review dir, returning what was drafted vs skipped.
///
/// `lessons` is a slice of `(category, content)` pairs from the lessons ledger.
/// Drafts land in [`PENDING_SKILLS_DIR`] only — never an active dir — so review
/// is required before a promoted skill can be injected.
///
/// # Errors
/// Returns `Err` if a draft directory or file cannot be created/written.
pub fn promote_lessons(
    repo: &Path,
    lessons: &[(String, String)],
    min_occurrences: usize,
) -> std::io::Result<PromotionReport> {
    let mut report = PromotionReport::default();
    for candidate in promotion_candidates(lessons, min_occurrences) {
        let name = draft_skill_name(&candidate.fingerprint);
        if skill_exists(repo, &name) {
            report.skipped.push(name);
            continue;
        }
        let path = pending_skill_path(repo, &name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, draft_skill_md(&candidate))?;
        report.drafted.push(name);
    }
    Ok(report)
}

/// Path of a pending draft for `name`: `<repo>/.lopi/skills-pending/<name>/SKILL.md`.
fn pending_skill_path(repo: &Path, name: &str) -> PathBuf {
    repo.join(PENDING_SKILLS_DIR).join(name).join("SKILL.md")
}

/// Whether a skill named `name` already exists as a pending draft or an active
/// skill — in which case a fresh draft would be a duplicate.
fn skill_exists(repo: &Path, name: &str) -> bool {
    if pending_skill_path(repo, name).is_file() {
        return true;
    }
    ACTIVE_SKILL_DIRS
        .iter()
        .any(|d| repo.join(d).join(name).join("SKILL.md").is_file())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::{promote_lessons, PENDING_SKILLS_DIR};
    use crate::SkillRegistry;
    use tempfile::TempDir;

    fn recurring_lessons() -> Vec<(String, String)> {
        vec![
            ("recovery".into(), "Run the tests after refactor".into()),
            ("recovery".into(), "after refactor run tests".into()),
            ("strategy".into(), "TESTS, after refactor, run!".into()),
        ]
    }

    #[test]
    fn drafts_land_in_pending_and_do_not_auto_activate() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();

        let report = promote_lessons(repo, &recurring_lessons(), 3).unwrap();
        assert_eq!(report.drafted, vec!["learned-after-refactor-tests"]);
        assert!(report.skipped.is_empty());

        // The draft exists in the pending dir...
        let draft = repo
            .join(PENDING_SKILLS_DIR)
            .join("learned-after-refactor-tests")
            .join("SKILL.md");
        assert!(draft.is_file(), "draft written to pending dir");

        // ...but the active registry (claude/lopi skills) loads nothing, so a
        // pending draft can never auto-inject without review.
        let active = SkillRegistry::load_from_dirs(&[
            repo.join(".claude/skills"),
            repo.join(".lopi/skills"),
        ])
        .unwrap();
        assert!(active.is_empty(), "pending drafts are not active");
    }

    #[test]
    fn sweep_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let lessons = recurring_lessons();

        let first = promote_lessons(repo, &lessons, 3).unwrap();
        assert_eq!(first.drafted.len(), 1);

        // A second sweep skips the already-drafted candidate.
        let second = promote_lessons(repo, &lessons, 3).unwrap();
        assert!(second.drafted.is_empty());
        assert_eq!(second.skipped, vec!["learned-after-refactor-tests"]);
    }

    #[test]
    fn skips_when_an_active_skill_already_owns_the_name() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        // Pre-create an active skill with the name the candidate would take.
        let active = repo
            .join(".lopi/skills")
            .join("learned-after-refactor-tests");
        std::fs::create_dir_all(&active).unwrap();
        std::fs::write(
            active.join("SKILL.md"),
            "---\nname: x\ndescription: d\n---\n",
        )
        .unwrap();

        let report = promote_lessons(repo, &recurring_lessons(), 3).unwrap();
        assert!(report.drafted.is_empty(), "no draft when name is taken");
        assert_eq!(report.skipped, vec!["learned-after-refactor-tests"]);
    }

    #[test]
    fn nothing_below_threshold() {
        let dir = TempDir::new().unwrap();
        let report = promote_lessons(dir.path(), &recurring_lessons(), 5).unwrap();
        assert!(report.drafted.is_empty() && report.skipped.is_empty());
    }
}
