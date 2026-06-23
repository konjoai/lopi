//! Per-task skill-registry loading for the agent pool (Pentad M2.2).
//!
//! Each task loads the repo's skills from `.claude/skills` and `.lopi/skills` so
//! the runner can inject the ones whose triggers match the goal. Loading is
//! failure-tolerant: a malformed `SKILL.md` is logged and yields an empty
//! registry rather than stalling task dispatch — a bad skill must never wedge
//! the loop.

use lopi_skill::SkillRegistry;
use std::path::Path;
use tracing::warn;

/// Conventional skill roots inside a repo, in precedence order.
const SKILL_DIRS: [&str; 2] = [".claude/skills", ".lopi/skills"];

/// Load `repo`'s skill registry from its conventional skill directories.
///
/// Returns an empty registry (never an error) when the dirs are absent or a
/// `SKILL.md` is malformed — the failure is logged, and the task proceeds with
/// no skills rather than failing to dispatch.
pub(super) fn load_skills(repo: &Path) -> SkillRegistry {
    let dirs: Vec<_> = SKILL_DIRS.iter().map(|d| repo.join(d)).collect();
    match SkillRegistry::load_from_dirs(&dirs) {
        Ok(reg) => reg,
        Err(e) => {
            warn!("skill registry load failed ({e}); proceeding without skills");
            SkillRegistry::default()
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::load_skills;
    use tempfile::TempDir;

    fn write_skill(root: &std::path::Path, name: &str, body: &str) {
        let dir = root.join(".claude/skills").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    #[test]
    fn loads_skills_from_claude_dir() {
        let dir = TempDir::new().unwrap();
        write_skill(
            dir.path(),
            "refactor",
            "---\nname: refactor\ndescription: d\ntriggers: refactor\n---\nbody\n",
        );
        let reg = load_skills(dir.path());
        assert_eq!(reg.len(), 1);
        assert!(reg.get("refactor").is_some());
    }

    #[test]
    fn empty_when_no_skill_dirs() {
        let dir = TempDir::new().unwrap();
        assert!(load_skills(dir.path()).is_empty());
    }

    #[test]
    fn malformed_skill_degrades_to_empty() {
        let dir = TempDir::new().unwrap();
        write_skill(dir.path(), "bad", "no frontmatter\n");
        // A malformed SKILL.md must not stall dispatch — empty, not a panic.
        assert!(load_skills(dir.path()).is_empty());
    }
}
