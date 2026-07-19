//! Discovery of every real Claude Code `/name` command registered in a
//! *target* repo — legacy `.claude/commands/*.md` files (frontmatter
//! optional) plus current-format `.claude/skills/*/SKILL.md` skills. Feeds
//! the composer's `/`-triggered autocomplete (Composer-Grammar-2): given a
//! repo path, returns the flat catalog of tokens a user could type.
//!
//! Deliberately does not reuse [`crate::SkillRegistry::load_from_dirs`],
//! whose all-or-nothing validation (one malformed `SKILL.md` fails the
//! entire load) is the right call for lopi's own trusted `.claude/` but
//! wrong here: `repo` is an arbitrary target the caller does not control, so
//! a single bad file must degrade that one entry, not the whole
//! autocomplete list. Every skipped entry is `tracing::warn!`'d, never
//! silent.

use crate::Skill;
use serde::Serialize;
use std::path::Path;

/// One real Claude Code command or skill invokable as `/name` in the
/// composer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaudeCommand {
    /// The token typed after `/` to invoke it.
    pub name: String,
    /// One-line description for the autocomplete hint — empty when the
    /// source file carries none (legacy commands are not required to).
    pub hint: String,
}

/// Discover every `/name` command registered in `repo`: legacy
/// `.claude/commands/*.md` files plus user-invocable
/// `.claude/skills/*/SKILL.md` skills. A skill wins over a legacy command of
/// the same name — current format takes precedence over legacy, matching
/// Claude Code's own resolution order. Neither directory existing is not an
/// error — most repos have neither, some have one, few have both.
#[must_use]
pub fn discover_claude_commands(repo: &Path) -> Vec<ClaudeCommand> {
    let mut by_name = std::collections::BTreeMap::new();
    for cmd in scan_legacy_commands(repo) {
        by_name.insert(cmd.name.clone(), cmd);
    }
    for cmd in scan_skill_commands(repo) {
        by_name.insert(cmd.name.clone(), cmd);
    }
    by_name.into_values().collect()
}

/// Scan `<repo>/.claude/commands/*.md` — the legacy command format. The
/// token name is the filename stem (`foo.md` → `foo`); the hint is an
/// optional `description:` frontmatter field, matching lopi's own
/// `.claude/commands/konjo.md` example, which carries no frontmatter at all
/// (a bare hint is not an error, just an empty one).
fn scan_legacy_commands(repo: &Path) -> Vec<ClaudeCommand> {
    let dir = repo.join(".claude").join("commands");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => out.push(ClaudeCommand {
                name: name.to_string(),
                hint: legacy_description(&text),
            }),
            Err(e) => tracing::warn!(
                path = %path.display(),
                error = %e,
                "skipping unreadable legacy command"
            ),
        }
    }
    out
}

/// Best-effort `description:` extraction from a legacy command's optional
/// frontmatter. Unlike [`Skill::parse`], frontmatter here is not required —
/// a file with none, or with no `description:` field, simply yields an
/// empty hint rather than an error.
fn legacy_description(text: &str) -> String {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some((key, val)) = line.split_once(':') {
            if key.trim() == "description" {
                return val.trim().to_string();
            }
        }
    }
    String::new()
}

/// Scan `<repo>/.claude/skills/*/SKILL.md` — the current format. Only
/// `user_invocable` skills are returned: a skill without that flag is
/// meant for auto-trigger relevance matching only (see
/// [`crate::SkillRegistry::relevant_to`]), never a token a human is meant
/// to type directly, so it must never appear in a `/`-typed autocomplete
/// list. Each file is parsed independently — one malformed `SKILL.md` in
/// the target repo is logged and skipped, not fatal to the others.
fn scan_skill_commands(repo: &Path) -> Vec<ClaudeCommand> {
    let dir = repo.join(".claude").join("skills");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let skill_path = entry.path().join("SKILL.md");
        if !skill_path.is_file() {
            continue;
        }
        let text = match std::fs::read_to_string(&skill_path) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    path = %skill_path.display(),
                    error = %e,
                    "skipping unreadable skill"
                );
                continue;
            }
        };
        match Skill::parse(&text, &skill_path) {
            Ok(skill) if skill.user_invocable => out.push(ClaudeCommand {
                name: skill.name,
                hint: skill.description,
            }),
            Ok(_) => {} // not user-invocable — auto-trigger only, never offered here
            Err(e) => tracing::warn!(
                path = %skill_path.display(),
                error = %e,
                "skipping malformed skill"
            ),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn empty_repo_yields_no_commands() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(discover_claude_commands(tmp.path()).is_empty());
    }

    /// Kill-test 2 — a legacy command and a skill, both present, both found.
    #[test]
    fn finds_both_legacy_commands_and_skills() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join(".claude/commands/foo.md"),
            "---\ndescription: does foo things\n---\n\nDo the foo thing.",
        );
        write(
            &tmp.path().join(".claude/skills/bar/SKILL.md"),
            "---\nname: bar\ndescription: does bar things\nuser-invocable: true\n---\n\nDo the bar thing.",
        );

        let found = discover_claude_commands(tmp.path());
        let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"foo"), "legacy command found: {names:?}");
        assert!(names.contains(&"bar"), "skill found: {names:?}");
        assert_eq!(
            found.iter().find(|c| c.name == "foo").unwrap().hint,
            "does foo things"
        );
        assert_eq!(
            found.iter().find(|c| c.name == "bar").unwrap().hint,
            "does bar things"
        );
    }

    /// Kill-test 2 — a deliberate name collision: the skill wins.
    #[test]
    fn skill_wins_on_name_collision_with_legacy_command() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join(".claude/commands/dup.md"),
            "---\ndescription: the legacy version\n---\n\nlegacy body",
        );
        write(
            &tmp.path().join(".claude/skills/dup/SKILL.md"),
            "---\nname: dup\ndescription: the skill version\nuser-invocable: true\n---\n\nskill body",
        );

        let found = discover_claude_commands(tmp.path());
        let dups: Vec<&ClaudeCommand> = found.iter().filter(|c| c.name == "dup").collect();
        assert_eq!(dups.len(), 1, "no duplicate entries for a colliding name");
        assert_eq!(dups[0].hint, "the skill version", "skill wins over legacy");
    }

    #[test]
    fn legacy_command_with_no_frontmatter_gets_an_empty_hint() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join(".claude/commands/konjo.md"),
            "Run the Konjo session boot sequence for lopi.",
        );
        let found = discover_claude_commands(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "konjo");
        assert_eq!(found[0].hint, "");
    }

    #[test]
    fn non_user_invocable_skill_is_never_offered() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join(".claude/skills/auto-only/SKILL.md"),
            "---\nname: auto-only\ndescription: auto-trigger only\n---\n\nbody",
        );
        assert!(
            discover_claude_commands(tmp.path()).is_empty(),
            "a skill without `user-invocable: true` must never be offered as a `/name` token"
        );
    }

    #[test]
    fn a_malformed_skill_is_skipped_not_fatal_to_the_rest() {
        let tmp = tempfile::tempdir().unwrap();
        // Missing required `description` field.
        write(
            &tmp.path().join(".claude/skills/broken/SKILL.md"),
            "---\nname: broken\n---\n\nbody",
        );
        write(
            &tmp.path().join(".claude/skills/fine/SKILL.md"),
            "---\nname: fine\ndescription: this one parses\nuser-invocable: true\n---\n\nbody",
        );

        let found = discover_claude_commands(tmp.path());
        let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["fine"],
            "the malformed skill is skipped, not fatal"
        );
    }

    #[test]
    fn non_md_files_and_missing_dirs_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join(".claude/commands/readme.txt"),
            "not a command",
        );
        assert!(discover_claude_commands(tmp.path()).is_empty());
    }
}
