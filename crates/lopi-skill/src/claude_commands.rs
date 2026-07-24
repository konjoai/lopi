//! Discovery of every real Claude Code `/name` command available to a user
//! working against a *target* repo: repo-level `.claude/commands/*.md` +
//! `.claude/skills/*/SKILL.md`, the same pair at user scope
//! (`~/.claude/...`), plugins installed at either scope, and a static list
//! of Claude Code's own built-in commands. Feeds the composer's
//! `/`-triggered autocomplete (Composer-Grammar-2): given a repo path,
//! returns the flat catalog of tokens a user could type.
//!
//! Deliberately does not reuse [`crate::SkillRegistry::load_from_dirs`],
//! whose all-or-nothing validation (one malformed `SKILL.md` fails the
//! entire load) is the right call for lopi's own trusted `.claude/` but
//! wrong here: `repo` (and a user's home directory, and a plugin's own
//! root) are locations the caller does not control, so a single bad file
//! must degrade that one entry, not the whole autocomplete list. Every
//! skipped entry is `tracing::warn!`'d, never silent.
//!
//! Four sources feed [`discover_claude_commands`], later ones winning a name
//! collision (most-specific-wins, matching Claude Code's own project-over-
//! user precedence):
//! 1. [`builtin_commands`] — commands hardcoded into the Claude Code binary
//!    itself (`/help`, `/review`, ...). There is no offline/filesystem way
//!    for a third-party tool to enumerate these — the only live discovery
//!    path is the SDK's `system/init` message's `slash_commands` array,
//!    which needs a running session — so this list is maintained by hand
//!    and may drift from what a given Claude Code version actually ships.
//! 2. Plugins installed under `<claude_dir>/plugins/**` at both user scope
//!    (`~/.claude`) and project scope (`<repo>/.claude`) — each plugin root
//!    (any directory holding its own `commands/` and/or `skills/`) is
//!    scanned the same way a repo's `.claude/` is. Claude Code's on-disk
//!    plugin layout is not part of any published schema, so plugin roots
//!    are found structurally (walk + look for `commands`/`skills`) rather
//!    than by assuming an exact nesting depth.
//! 3. User-level commands/skills (`~/.claude/commands/*.md`,
//!    `~/.claude/skills/*/SKILL.md`) — identical file format to repo-level.
//! 4. Repo-level commands/skills — the original scan, unchanged.

use crate::Skill;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

/// How deep to walk `<claude_dir>/plugins/` looking for plugin roots. Real
/// installs commonly nest a couple of levels (e.g.
/// `plugins/repos/<marketplace>/<plugin>/`); this leaves headroom without
/// risking a runaway walk on a pathological directory tree.
const PLUGIN_SCAN_MAX_DEPTH: u8 = 6;

/// Discover every `/name` command available to a user working against
/// `repo`: Claude Code's own built-ins, installed plugins (user + project
/// scope), user-level commands/skills, and `repo`'s own — see the module
/// doc comment for the full precedence order. `home` is the user's home
/// directory; pass `None` to skip every home-scoped source (which is how
/// the unit tests below get a hermetic, environment-independent result
/// regardless of what happens to live under the real `$HOME`). Production
/// callers pass `std::env::var("HOME")`.
#[must_use]
pub fn discover_claude_commands(repo: &Path, home: Option<&Path>) -> Vec<ClaudeCommand> {
    let mut by_name = BTreeMap::new();
    let merge = |cmds: Vec<ClaudeCommand>, map: &mut BTreeMap<String, ClaudeCommand>| {
        for cmd in cmds {
            map.insert(cmd.name.clone(), cmd);
        }
    };
    merge(builtin_commands(), &mut by_name);
    if let Some(home) = home {
        merge(scan_plugins(&home.join(".claude")), &mut by_name);
        merge(commands_and_skills_under(home), &mut by_name);
    }
    merge(scan_plugins(&repo.join(".claude")), &mut by_name);
    merge(commands_and_skills_under(repo), &mut by_name);
    by_name.into_values().collect()
}

/// Claude Code's own native commands — hardcoded into the CLI binary, not
/// backed by any file on disk. Maintained by hand from the public
/// slash-command reference; expect this to drift as Claude Code ships new
/// ones or retires old ones.
const BUILTIN_COMMANDS: &[(&str, &str)] = &[
    ("add-dir", "Add an additional working directory"),
    ("agents", "Manage custom subagents"),
    ("bug", "Report a bug to Anthropic"),
    ("clear", "Clear conversation history"),
    ("compact", "Compact conversation history"),
    ("config", "View or modify configuration"),
    ("context", "Show current context usage"),
    ("cost", "Show token usage and cost"),
    ("doctor", "Diagnose installation health"),
    ("exit", "Exit the session"),
    ("export", "Export the conversation"),
    ("help", "Show help and available commands"),
    ("hooks", "Manage hook configuration"),
    ("init", "Bootstrap a CLAUDE.md for this repo"),
    ("install-github-app", "Install the Claude GitHub app"),
    ("login", "Authenticate with Anthropic"),
    ("logout", "Sign out"),
    ("mcp", "Manage MCP server connections"),
    ("memory", "Edit CLAUDE.md memory files"),
    ("model", "Show or change the active model"),
    ("output-style", "Change the response output style"),
    ("permissions", "View or change tool permissions"),
    ("pr-comments", "Show pull request review comments"),
    ("resume", "Resume a previous session"),
    ("review", "Review a pull request"),
    (
        "security-review",
        "Run a security review of pending changes",
    ),
    ("status", "Show session/account status"),
    ("statusline", "Configure the status line"),
    ("terminal-setup", "Configure terminal integration"),
    ("todos", "Show the current todo list"),
    ("usage", "Show plan usage"),
    ("vim", "Toggle vim key bindings"),
];

/// [`BUILTIN_COMMANDS`] rendered as [`ClaudeCommand`]s.
#[must_use]
pub fn builtin_commands() -> Vec<ClaudeCommand> {
    BUILTIN_COMMANDS
        .iter()
        .map(|(name, hint)| ClaudeCommand {
            name: (*name).to_string(),
            hint: (*hint).to_string(),
        })
        .collect()
}

/// Legacy commands + user-invocable skills under `<base>/.claude/`, merged
/// with a skill winning over a legacy command of the same name (current
/// format takes precedence over legacy, matching Claude Code's own
/// resolution order). `base` is either a repo root or a user's home
/// directory — both lay out `.claude/` identically.
fn commands_and_skills_under(base: &Path) -> Vec<ClaudeCommand> {
    let mut by_name = BTreeMap::new();
    for cmd in scan_legacy_commands(base) {
        by_name.insert(cmd.name.clone(), cmd);
    }
    for cmd in scan_skill_commands(base) {
        by_name.insert(cmd.name.clone(), cmd);
    }
    by_name.into_values().collect()
}

/// Scan `<base>/.claude/commands/*.md` — the legacy command format. The
/// token name is the filename stem (`foo.md` → `foo`); the hint is an
/// optional `description:` frontmatter field, matching lopi's own
/// `.claude/commands/konjo.md` example, which carries no frontmatter at all
/// (a bare hint is not an error, just an empty one).
fn scan_legacy_commands(base: &Path) -> Vec<ClaudeCommand> {
    commands_in_dir(&base.join(".claude").join("commands"))
}

/// Scan `<base>/.claude/skills/*/SKILL.md` — the current format. Only
/// `user_invocable` skills are returned: a skill without that flag is
/// meant for auto-trigger relevance matching only (see
/// [`crate::SkillRegistry::relevant_to`]), never a token a human is meant
/// to type directly, so it must never appear in a `/`-typed autocomplete
/// list.
fn scan_skill_commands(base: &Path) -> Vec<ClaudeCommand> {
    skills_in_dir(&base.join(".claude").join("skills"))
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

/// Scan `<dir>` for legacy `*.md` command files directly (no `.claude`
/// join) — the low-level scanner reused by both [`scan_legacy_commands`]
/// (`<base>/.claude/commands`) and plugin discovery (`<plugin-root>/commands`).
/// Each file is read independently: one unreadable file is logged and
/// skipped, never fatal to the rest of the catalog.
fn commands_in_dir(dir: &Path) -> Vec<ClaudeCommand> {
    let Ok(entries) = std::fs::read_dir(dir) else {
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

/// Scan `<dir>` for `*/SKILL.md` skill directories directly (no `.claude`
/// join) — the low-level scanner reused by both [`scan_skill_commands`]
/// (`<base>/.claude/skills`) and plugin discovery (`<plugin-root>/skills`).
/// Only `user_invocable: true` skills are returned; each is parsed
/// independently — one malformed `SKILL.md` is logged and skipped, not
/// fatal to the others.
fn skills_in_dir(dir: &Path) -> Vec<ClaudeCommand> {
    let Ok(entries) = std::fs::read_dir(dir) else {
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

/// Find every plugin root under `dir` (recursively, bounded by `depth`) —
/// any directory that itself holds a `commands/` or `skills/` subdirectory.
/// Appends into `out` rather than returning, so the recursion doesn't
/// reallocate a fresh `Vec` per level.
fn find_plugin_roots(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("commands").is_dir() || path.join("skills").is_dir() {
            out.push(path.clone());
        }
        find_plugin_roots(&path, depth - 1, out);
    }
}

/// Every command/skill contributed by plugins installed under
/// `<claude_dir>/plugins`. Plugin roots are visited in sorted-path order so
/// a name collision between two plugins resolves deterministically. Neither
/// `plugins/` existing, nor any given plugin root having neither
/// `commands/` nor `skills/`, is an error — most repos/home directories
/// have no plugins installed at all.
fn scan_plugins(claude_dir: &Path) -> Vec<ClaudeCommand> {
    let mut roots = Vec::new();
    find_plugin_roots(
        &claude_dir.join("plugins"),
        PLUGIN_SCAN_MAX_DEPTH,
        &mut roots,
    );
    roots.sort();
    let mut by_name = BTreeMap::new();
    for root in roots {
        for cmd in commands_in_dir(&root.join("commands")) {
            by_name.insert(cmd.name.clone(), cmd);
        }
        for cmd in skills_in_dir(&root.join("skills")) {
            by_name.insert(cmd.name.clone(), cmd);
        }
    }
    by_name.into_values().collect()
}

#[cfg(test)]
#[path = "claude_commands_tests.rs"]
mod tests;
