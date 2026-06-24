//! Discovery and validation of [`Skill`]s across one or more skill directories.

use crate::{Skill, SkillError};
use std::path::{Path, PathBuf};

/// A validated set of skills loaded from disk.
///
/// Construction is the only validation point: a malformed `SKILL.md` or two
/// skills sharing a `name` fail the whole load, so a live registry is always
/// internally consistent. Lookup is by `name`; iteration is in a stable,
/// directory-sorted order.
#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: Vec<Skill>,
}

impl SkillRegistry {
    /// Discover and parse every `<dir>/<name>/SKILL.md` across `dirs`, in order.
    ///
    /// A missing directory is skipped (not an error) so optional roots like
    /// `.lopi/skills` are tolerated. Duplicate names across any dirs are
    /// rejected so the agent never injects two skills under one name.
    ///
    /// # Errors
    /// Returns [`SkillError`] on an unreadable file, a malformed `SKILL.md`, or a
    /// duplicate skill name.
    pub fn load_from_dirs(dirs: &[PathBuf]) -> Result<Self, SkillError> {
        let mut skills: Vec<Skill> = Vec::new();
        for dir in dirs {
            for path in skill_files(dir) {
                let text = std::fs::read_to_string(&path).map_err(|e| SkillError::Io {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
                let skill = Skill::parse(&text, &path)?;
                if let Some(existing) = skills.iter().find(|s| s.name == skill.name) {
                    return Err(SkillError::DuplicateName {
                        name: skill.name,
                        first: existing.source.clone(),
                        second: skill.source,
                    });
                }
                skills.push(skill);
            }
        }
        Ok(Self { skills })
    }

    /// The skill named `name`, if loaded.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.name == name)
    }

    /// Skills whose triggers fire for `goal`, in registry order.
    ///
    /// Matching is keyword-based today — a case-insensitive substring of any
    /// trigger in the goal — but isolated in `skill_matches` so a semantic /
    /// embedding matcher can replace it without touching callers. A skill with
    /// no triggers never auto-injects (it must be requested explicitly), so a
    /// task that matches nothing pulls in nothing: no context bloat.
    #[must_use]
    pub fn relevant_to(&self, goal: &str) -> Vec<&Skill> {
        let goal_lower = goal.to_lowercase();
        self.skills
            .iter()
            .filter(|s| skill_matches(s, &goal_lower))
            .collect()
    }

    /// The names of all loaded skills, in registry order.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.skills.iter().map(|s| s.name.as_str()).collect()
    }

    /// Iterate over the loaded skills.
    pub fn iter(&self) -> std::slice::Iter<'_, Skill> {
        self.skills.iter()
    }

    /// Number of loaded skills.
    #[must_use]
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether no skills are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

/// Whether any of `skill`'s triggers appears in the already-lowercased `goal`.
/// The single matching predicate — swap this for an embedding score to make
/// relevance semantic without changing [`SkillRegistry::relevant_to`]'s shape.
fn skill_matches(skill: &Skill, goal_lower: &str) -> bool {
    skill.triggers.iter().any(|t| {
        let t = t.to_lowercase();
        !t.is_empty() && goal_lower.contains(&t)
    })
}

/// List `<dir>/<name>/SKILL.md` paths under `dir`, sorted for determinism. A
/// missing or unreadable `dir` yields an empty list.
fn skill_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path().join("SKILL.md"))
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    out
}
