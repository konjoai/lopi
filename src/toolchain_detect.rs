//! Onboarding-Import-1 (Phase 2) — manifest-file toolchain detection.
//!
//! Sibling to `repo_detect.rs`: that module finds a git root; this one,
//! given a directory, derives a coarse per-project ecosystem label from the
//! manifest files sitting directly in it. `repo_detect.rs` confirmed no
//! language/toolchain detection existed anywhere in lopi before this
//! sprint — this is the first one, not a duplicate of existing logic.
//!
//! This needs filesystem read access to each historical session's project
//! directory, not just `~/.claude` — a requirement the onboarding CLI
//! surfaces explicitly rather than silently assuming (a directory that no
//! longer exists, or that this machine never had, is a normal "unknown
//! toolchain" outcome, not an error).

use std::path::Path;

/// Manifest filename → toolchain label, checked in this order (first match
/// wins) so a polyglot repo still gets one deterministic label.
const MANIFESTS: &[(&str, &str)] = &[
    ("Cargo.toml", "rust"),
    ("package.json", "node"),
    ("pyproject.toml", "python"),
    ("requirements.txt", "python"),
    ("go.mod", "go"),
    ("Gemfile", "ruby"),
];

/// Walk `dir` (its direct contents only — not recursive) for a known
/// manifest file and return the matching toolchain label.
///
/// Returns `None` when no known manifest is present, or when `dir` doesn't
/// exist or isn't readable — both are legitimate "unknown toolchain"
/// outcomes for the onboarding backfill, not error conditions.
#[must_use]
pub fn detect_toolchain(dir: &Path) -> Option<String> {
    MANIFESTS
        .iter()
        .find(|(filename, _)| dir.join(filename).is_file())
        .map(|(_, label)| (*label).to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir(suffix: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("lopi-toolchain-detect-{suffix}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn detects_rust_from_cargo_toml() {
        let dir = tmp_dir("rust");
        fs::write(dir.join("Cargo.toml"), "[package]\nname=\"x\"").unwrap();
        assert_eq!(detect_toolchain(&dir).as_deref(), Some("rust"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_node_from_package_json() {
        let dir = tmp_dir("node");
        fs::write(dir.join("package.json"), "{}").unwrap();
        assert_eq!(detect_toolchain(&dir).as_deref(), Some("node"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_python_from_pyproject_toml() {
        let dir = tmp_dir("python-pyproject");
        fs::write(dir.join("pyproject.toml"), "[project]").unwrap();
        assert_eq!(detect_toolchain(&dir).as_deref(), Some("python"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_python_from_requirements_txt() {
        let dir = tmp_dir("python-requirements");
        fs::write(dir.join("requirements.txt"), "requests").unwrap();
        assert_eq!(detect_toolchain(&dir).as_deref(), Some("python"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_go_from_go_mod() {
        let dir = tmp_dir("go");
        fs::write(dir.join("go.mod"), "module x").unwrap();
        assert_eq!(detect_toolchain(&dir).as_deref(), Some("go"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_ruby_from_gemfile() {
        let dir = tmp_dir("ruby");
        fs::write(dir.join("Gemfile"), "source 'x'").unwrap();
        assert_eq!(detect_toolchain(&dir).as_deref(), Some("ruby"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cargo_toml_wins_over_package_json_in_a_polyglot_repo() {
        let dir = tmp_dir("polyglot");
        fs::write(dir.join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.join("package.json"), "{}").unwrap();
        assert_eq!(detect_toolchain(&dir).as_deref(), Some("rust"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn returns_none_when_no_known_manifest_present() {
        let dir = tmp_dir("unknown");
        fs::write(dir.join("README.md"), "hi").unwrap();
        assert_eq!(detect_toolchain(&dir), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn returns_none_for_a_nonexistent_directory() {
        let dir = std::env::temp_dir().join("lopi-toolchain-detect-does-not-exist");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(detect_toolchain(&dir), None);
    }
}
