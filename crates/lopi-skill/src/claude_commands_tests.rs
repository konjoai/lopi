//! Unit tests for `claude_commands.rs` — split out to keep the discovery
//! module under the 500-line file gate. Included via `#[path]` from
//! `claude_commands.rs` so `super::*` still resolves to its items.
#![allow(clippy::unwrap_used)]

use super::*;

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// Sort by name so comparisons don't depend on `BUILTIN_COMMANDS`'s
/// declaration order.
fn names_of(cmds: &[ClaudeCommand]) -> Vec<String> {
    let mut names: Vec<String> = cmds.iter().map(|c| c.name.clone()).collect();
    names.sort();
    names
}

#[test]
fn empty_repo_and_no_home_yields_only_builtins() {
    let tmp = tempfile::tempdir().unwrap();
    let found = discover_claude_commands(tmp.path(), None);
    assert_eq!(
        names_of(&found),
        names_of(&builtin_commands()),
        "with nothing on disk and no home, only Claude's built-ins remain"
    );
}

#[test]
fn builtin_commands_include_well_known_names() {
    let names = names_of(&builtin_commands());
    for expected in ["help", "review", "security-review", "model"] {
        assert!(names.contains(&expected.to_string()), "missing built-in: {expected}");
    }
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

    let found = discover_claude_commands(tmp.path(), None);
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

    let found = discover_claude_commands(tmp.path(), None);
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
    let found = discover_claude_commands(tmp.path(), None);
    let konjo: Vec<&ClaudeCommand> = found.iter().filter(|c| c.name == "konjo").collect();
    assert_eq!(konjo.len(), 1);
    assert_eq!(konjo[0].hint, "");
}

#[test]
fn non_user_invocable_skill_is_never_offered() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        &tmp.path().join(".claude/skills/auto-only/SKILL.md"),
        "---\nname: auto-only\ndescription: auto-trigger only\n---\n\nbody",
    );
    assert!(
        !discover_claude_commands(tmp.path(), None)
            .iter()
            .any(|c| c.name == "auto-only"),
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

    let found = discover_claude_commands(tmp.path(), None);
    assert!(found.iter().any(|c| c.name == "fine"), "the valid skill is found");
    assert!(
        !found.iter().any(|c| c.name == "broken"),
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
    assert!(
        !discover_claude_commands(tmp.path(), None)
            .iter()
            .any(|c| c.name == "readme"),
        "a non-.md file must never be offered as a command"
    );
}

#[test]
fn home_level_legacy_command_is_discovered() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write(
        &home.path().join(".claude/commands/simplify.md"),
        "---\ndescription: simplify the diff\n---\n\nbody",
    );
    let found = discover_claude_commands(repo.path(), Some(home.path()));
    let simplify: Vec<&ClaudeCommand> = found.iter().filter(|c| c.name == "simplify").collect();
    assert_eq!(simplify.len(), 1, "a user-level command is discovered");
    assert_eq!(simplify[0].hint, "simplify the diff");
}

#[test]
fn home_level_skill_is_discovered() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write(
        &home.path().join(".claude/skills/loop/SKILL.md"),
        "---\nname: loop\ndescription: run on an interval\nuser-invocable: true\n---\n\nbody",
    );
    let found = discover_claude_commands(repo.path(), Some(home.path()));
    assert!(found.iter().any(|c| c.name == "loop"), "a user-level skill is discovered");
}

#[test]
fn no_home_means_home_scoped_sources_are_skipped() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write(
        &home.path().join(".claude/commands/simplify.md"),
        "a home command",
    );
    assert!(
        !discover_claude_commands(repo.path(), None)
            .iter()
            .any(|c| c.name == "simplify"),
        "home is opt-in via the `home` parameter, never read implicitly"
    );
}

#[test]
fn repo_command_overrides_same_named_home_command() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write(
        &home.path().join(".claude/commands/dup.md"),
        "---\ndescription: home version\n---\n\nbody",
    );
    write(
        &repo.path().join(".claude/commands/dup.md"),
        "---\ndescription: repo version\n---\n\nbody",
    );
    let found = discover_claude_commands(repo.path(), Some(home.path()));
    let dup = found.iter().find(|c| c.name == "dup").unwrap();
    assert_eq!(dup.hint, "repo version", "the more specific repo-level command wins");
}

#[test]
fn plugin_command_is_discovered_at_user_scope() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write(
        &home
            .path()
            .join(".claude/plugins/repos/anthropics/claude-code-plugins/code-simplifier/commands/tidy.md"),
        "---\ndescription: tidy up\n---\n\nbody",
    );
    let found = discover_claude_commands(repo.path(), Some(home.path()));
    assert!(
        found.iter().any(|c| c.name == "tidy"),
        "a nested plugin command is discovered regardless of exact nesting depth"
    );
}

#[test]
fn plugin_skill_is_discovered_at_project_scope() {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join(".claude/plugins/local/my-plugin/skills/greet/SKILL.md"),
        "---\nname: greet\ndescription: say hi\nuser-invocable: true\n---\n\nbody",
    );
    let found = discover_claude_commands(repo.path(), None);
    assert!(
        found.iter().any(|c| c.name == "greet"),
        "project-scoped plugins (<repo>/.claude/plugins) are discovered without a home dir"
    );
}

#[test]
fn repo_command_overrides_same_named_plugin_command() {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join(".claude/plugins/local/my-plugin/commands/dup.md"),
        "---\ndescription: plugin version\n---\n\nbody",
    );
    write(
        &repo.path().join(".claude/commands/dup.md"),
        "---\ndescription: repo version\n---\n\nbody",
    );
    let found = discover_claude_commands(repo.path(), None);
    let dup = found.iter().find(|c| c.name == "dup").unwrap();
    assert_eq!(dup.hint, "repo version", "a real repo command outranks a plugin of the same name");
}

#[test]
fn plugin_dir_with_neither_commands_nor_skills_contributes_nothing() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join(".claude/plugins/local/empty-plugin")).unwrap();
    // Must not panic and must not surface anything from the empty plugin dir.
    let found = discover_claude_commands(repo.path(), None);
    assert_eq!(names_of(&found), names_of(&builtin_commands()));
}
