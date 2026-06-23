#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::registry::SkillRegistry;
use std::path::Path;
use tempfile::TempDir;

const SAMPLE: &str = "---\n\
name: konjo-boot\n\
description: Boot a Konjo session. Use at the start of any work session.\n\
user-invocable: true\n\
---\n\
# Konjo Session Boot\n\
\n\
## Step 1 — Read\n";

#[test]
fn parses_frontmatter_and_body() {
    let skill = Skill::parse(SAMPLE, Path::new("/skills/konjo-boot/SKILL.md")).unwrap();
    assert_eq!(skill.name, "konjo-boot");
    assert!(skill.description.starts_with("Boot a Konjo session"));
    assert!(skill.user_invocable);
    assert_eq!(skill.version, "0.0.0", "default version");
    assert!(skill.triggers.is_empty());
    assert!(skill.body.starts_with("# Konjo Session Boot"));
    assert!(skill.body.contains("Step 1"));
}

#[test]
fn description_with_colons_is_preserved() {
    // The description splits on the *first* colon only.
    let text = "---\nname: x\ndescription: The Konjo Way: pillars, values: kept.\n---\nbody\n";
    let skill = Skill::parse(text, Path::new("x/SKILL.md")).unwrap();
    assert_eq!(skill.description, "The Konjo Way: pillars, values: kept.");
}

#[test]
fn optional_version_and_triggers_parse() {
    let text =
        "---\nname: x\ndescription: d\nversion: 1.2.3\ntriggers: build, test , , ci\n---\nb\n";
    let skill = Skill::parse(text, Path::new("x/SKILL.md")).unwrap();
    assert_eq!(skill.version, "1.2.3");
    assert_eq!(skill.triggers, vec!["build", "test", "ci"]);
    assert!(
        !skill.user_invocable,
        "absent user-invocable defaults false"
    );
}

#[test]
fn missing_opening_fence_errors() {
    let err = Skill::parse("# no frontmatter\n", Path::new("a/SKILL.md")).unwrap_err();
    assert!(matches!(err, SkillError::MissingFrontmatter { .. }));
}

#[test]
fn unterminated_frontmatter_errors() {
    let err = Skill::parse("---\nname: x\n", Path::new("a/SKILL.md")).unwrap_err();
    assert!(matches!(err, SkillError::UnterminatedFrontmatter { .. }));
}

#[test]
fn missing_required_field_errors_with_line() {
    let err = Skill::parse("---\nname: x\n---\nbody\n", Path::new("a/SKILL.md")).unwrap_err();
    match err {
        SkillError::MissingField { field, line, .. } => {
            assert_eq!(field, "description");
            assert_eq!(line, 3, "closing fence line reported");
        }
        other => panic!("expected MissingField, got {other:?}"),
    }
}

// ── Registry ────────────────────────────────────────────────────────────────

fn write_skill(root: &Path, name: &str, body: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), body).unwrap();
}

#[test]
fn registry_loads_and_sorts_skills() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    write_skill(
        &root,
        "zeta",
        "---\nname: zeta\ndescription: z\n---\nbody\n",
    );
    write_skill(
        &root,
        "alpha",
        "---\nname: alpha\ndescription: a\n---\nbody\n",
    );

    let reg = SkillRegistry::load_from_dirs(&[root]).unwrap();
    assert_eq!(reg.len(), 2);
    assert!(!reg.is_empty());
    // Sorted by directory path → alpha before zeta.
    assert_eq!(reg.names(), vec!["alpha", "zeta"]);
    assert_eq!(reg.get("alpha").unwrap().description, "a");
    assert!(reg.get("missing").is_none());
    assert_eq!(reg.iter().count(), 2);
}

#[test]
fn registry_skips_missing_dirs() {
    let reg = SkillRegistry::load_from_dirs(&[std::path::PathBuf::from("/no/such/dir")]).unwrap();
    assert!(reg.is_empty());
}

#[test]
fn registry_rejects_duplicate_names() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    write_skill(a.path(), "dup", "---\nname: dup\ndescription: a\n---\nx\n");
    write_skill(
        b.path(),
        "dup-dir",
        "---\nname: dup\ndescription: b\n---\nx\n",
    );

    let err = SkillRegistry::load_from_dirs(&[a.path().to_path_buf(), b.path().to_path_buf()])
        .unwrap_err();
    assert!(matches!(err, SkillError::DuplicateName { name, .. } if name == "dup"));
}

#[test]
fn registry_propagates_malformed_skill() {
    let dir = TempDir::new().unwrap();
    write_skill(dir.path(), "bad", "no frontmatter here\n");
    let err = SkillRegistry::load_from_dirs(&[dir.path().to_path_buf()]).unwrap_err();
    assert!(matches!(err, SkillError::MissingFrontmatter { .. }));
}

/// Sprint 2.1 DoD: every bundled `.claude/skills/*/SKILL.md` parses cleanly.
#[test]
fn loads_the_repos_bundled_skills() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude/skills");
    if !root.exists() {
        return; // tolerate running outside a full checkout
    }
    let reg = SkillRegistry::load_from_dirs(&[root]).unwrap();
    assert!(
        reg.len() >= 6,
        "expected ≥6 bundled skills, got {}",
        reg.len()
    );
    assert!(reg.get("konjo-boot").is_some(), "konjo-boot present");
    assert!(reg.get("lopi-context").is_some(), "lopi-context present");
}
