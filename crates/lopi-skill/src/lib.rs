//! lopi-skill — a runtime registry of `SKILL.md` project knowledge.
//!
//! Skills are the institutional memory an agent would otherwise re-guess every
//! session — Osmani's "how you stop re-explaining the same project context."
//! Each lives in a `<dir>/<name>/SKILL.md` with YAML-ish frontmatter (`name`,
//! `description`, `user-invocable`, optional `version`/`triggers`) followed by a
//! markdown body. This crate parses them into a typed [`Skill`] and a validated
//! [`SkillRegistry`] the runner can load and inject.
//!
//! Frontmatter is a flat `key: value` block, so it is parsed by hand rather than
//! pulling in a YAML dependency. Malformed input fails loudly — with the file
//! and line — never silently.

/// Lesson → skill promotion detection — the self-evolving Ratchet's detector.
pub mod promote;
/// Writing promotion drafts to a pending-review directory.
pub mod promoter;
/// The [`SkillRegistry`](registry::SkillRegistry): discovery + validation.
pub mod registry;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use promote::{draft_skill_md, promotion_candidates, PromotionCandidate};
pub use promoter::{promote_lessons, PromotionReport, PENDING_SKILLS_DIR};
pub use registry::SkillRegistry;

/// Default version when a skill omits the `version` field.
const DEFAULT_VERSION: &str = "0.0.0";

/// A single skill: parsed frontmatter plus its markdown body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    /// Unique skill name (the `name` frontmatter field).
    pub name: String,
    /// One-line description of when the skill applies.
    pub description: String,
    /// Whether a user may invoke the skill directly (the `user-invocable` field).
    pub user_invocable: bool,
    /// Semver-ish version string; `"0.0.0"` when unspecified.
    pub version: String,
    /// Optional trigger keywords (comma-separated in frontmatter) used by
    /// relevance matching. Empty when unspecified.
    pub triggers: Vec<String>,
    /// The markdown body after the frontmatter.
    pub body: String,
    /// Path the skill was loaded from.
    pub source: PathBuf,
}

/// Why a `SKILL.md` failed to parse or a registry failed to validate.
#[derive(Debug, Error)]
pub enum SkillError {
    /// The file did not start with a `---` frontmatter fence.
    #[error("{path}: missing `---` frontmatter fence on the first line")]
    MissingFrontmatter {
        /// File that failed.
        path: PathBuf,
    },
    /// The opening `---` fence was never closed.
    #[error("{path}: unterminated frontmatter — no closing `---`")]
    UnterminatedFrontmatter {
        /// File that failed.
        path: PathBuf,
    },
    /// A required frontmatter field was absent.
    #[error("{path}:{line}: missing required frontmatter field `{field}`")]
    MissingField {
        /// File that failed.
        path: PathBuf,
        /// Line of the closing fence (where the field was expected by).
        line: usize,
        /// The absent field name.
        field: &'static str,
    },
    /// Two skills declared the same `name`.
    #[error("duplicate skill name `{name}`: {first} and {second}")]
    DuplicateName {
        /// The colliding name.
        name: String,
        /// First file declaring it.
        first: PathBuf,
        /// Second file declaring it.
        second: PathBuf,
    },
    /// A skill file could not be read.
    #[error("{path}: {message}")]
    Io {
        /// File that failed.
        path: PathBuf,
        /// Underlying IO error text.
        message: String,
    },
}

impl Skill {
    /// Parse a `SKILL.md`'s `text` (loaded from `source`).
    ///
    /// # Errors
    /// Returns [`SkillError`] when the frontmatter fence is missing/unterminated
    /// or a required field (`name`, `description`) is absent.
    pub fn parse(text: &str, source: &Path) -> Result<Self, SkillError> {
        let fm = Frontmatter::extract(text, source)?;
        let name = fm.required("name")?;
        let description = fm.required("description")?;
        Ok(Self {
            name,
            description,
            user_invocable: fm.get("user-invocable").is_some_and(|v| v == "true"),
            version: fm
                .get("version")
                .map_or_else(|| DEFAULT_VERSION.to_string(), str::to_string),
            triggers: fm.get("triggers").map(split_csv).unwrap_or_default(),
            body: fm.body,
            source: source.to_path_buf(),
        })
    }
}

/// Split a comma-separated frontmatter value into trimmed, non-empty entries.
fn split_csv(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parsed frontmatter: flat key/value pairs plus the body that follows.
struct Frontmatter {
    fields: Vec<(String, String)>,
    body: String,
    /// 1-based line of the closing fence, for error messages.
    close_line: usize,
    path: PathBuf,
}

impl Frontmatter {
    /// Split `text` into its `---`-fenced frontmatter and the body after it.
    fn extract(text: &str, source: &Path) -> Result<Self, SkillError> {
        let mut lines = text.lines().enumerate();
        // The first non-empty line must be the opening fence.
        match lines.find(|(_, l)| !l.trim().is_empty()) {
            Some((_, l)) if l.trim() == "---" => {}
            _ => {
                return Err(SkillError::MissingFrontmatter {
                    path: source.to_path_buf(),
                })
            }
        }
        let mut fields = Vec::new();
        for (idx, line) in lines.by_ref() {
            if line.trim() == "---" {
                let body = body_after(text, idx);
                return Ok(Self {
                    fields,
                    body,
                    close_line: idx + 1,
                    path: source.to_path_buf(),
                });
            }
            if let Some((k, v)) = line.split_once(':') {
                fields.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        Err(SkillError::UnterminatedFrontmatter {
            path: source.to_path_buf(),
        })
    }

    /// Value for `key`, if present.
    fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Value for a required `key`, erroring with file+line when absent or empty.
    fn required(&self, key: &'static str) -> Result<String, SkillError> {
        match self.get(key) {
            Some(v) if !v.is_empty() => Ok(v.to_string()),
            _ => Err(SkillError::MissingField {
                path: self.path.clone(),
                line: self.close_line,
                field: key,
            }),
        }
    }
}

/// The body is everything after the closing-fence line index `close_idx`.
fn body_after(text: &str, close_idx: usize) -> String {
    text.lines()
        .skip(close_idx + 1)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests;
