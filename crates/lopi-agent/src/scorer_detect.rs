//! Stack detection for [`Scorer`](crate::scorer::Scorer) — which test runner
//! (if any) applies to a repo. Split out of `scorer.rs` purely to keep that
//! file under the 500-line CI file-size gate.
//!
//! Sprint F2 Phase 1 — the pre-F2 scorer recognized only `Cargo.toml` and
//! `package.json`, silently scoring every other stack via the "no runner
//! detected" fallback (see `scorer.rs`'s `unevaluated_reason`). Python is the
//! highest-value addition per the sprint brief: the largest language in the
//! AI-adjacent work lopi targets, and the one that scored best while being
//! checked least.

use std::path::Path;

/// A detected (or explicitly configured) test runner for a repo.
///
/// [`Explicit`](Runner::Explicit) — the `.lopi/loop.toml` `test_command`
/// escape hatch — always wins over detection when set; it covers whatever
/// detection misses rather than trying to special-case every build tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Runner {
    /// Operator-supplied command from `.lopi/loop.toml`'s `test_command`.
    /// Shelled out via `sh -c` since it's a free-form string, not a fixed argv.
    Explicit(String),
    /// `Cargo.toml` present.
    Cargo,
    /// `pnpm-lock.yaml` present — checked before `Npm`/`Yarn` so a repo with
    /// multiple lockfiles (migration in progress) prefers the newest tool.
    Pnpm,
    /// `yarn.lock` present (and no `pnpm-lock.yaml`).
    Yarn,
    /// `package.json` present (and neither `pnpm-lock.yaml` nor `yarn.lock`).
    Npm,
    /// A Python project manifest (`pyproject.toml`, `setup.py`, `setup.cfg`,
    /// `pytest.ini`, or `requirements.txt`).
    Pytest,
    /// `go.mod` present.
    Go,
    /// `build.gradle` or `build.gradle.kts` present.
    Gradle,
    /// `pom.xml` present.
    Maven,
}

/// Detect the test runner for `repo_path`. `explicit` (from
/// `.lopi/loop.toml`'s `test_command`) always wins when set — an operator
/// override covers whatever detection misses without needing a new case
/// added here. Returns `None` when nothing recognized applies.
pub(crate) fn detect(repo_path: &Path, explicit: Option<&str>) -> Option<Runner> {
    if let Some(cmd) = explicit {
        return Some(Runner::Explicit(cmd.to_string()));
    }
    if repo_path.join("Cargo.toml").exists() {
        return Some(Runner::Cargo);
    }
    if repo_path.join("pnpm-lock.yaml").exists() {
        return Some(Runner::Pnpm);
    }
    if repo_path.join("yarn.lock").exists() {
        return Some(Runner::Yarn);
    }
    if repo_path.join("package.json").exists() {
        return Some(Runner::Npm);
    }
    if is_python_project(repo_path) {
        return Some(Runner::Pytest);
    }
    if repo_path.join("go.mod").exists() {
        return Some(Runner::Go);
    }
    if repo_path.join("build.gradle").exists() || repo_path.join("build.gradle.kts").exists() {
        return Some(Runner::Gradle);
    }
    if repo_path.join("pom.xml").exists() {
        return Some(Runner::Maven);
    }
    None
}

/// True when `repo_path` carries a recognized Python project manifest.
/// Checks the common set rather than just `pyproject.toml` — a Python repo
/// without packaging metadata (a plain `pytest.ini` + `requirements.txt`
/// project) is still common enough to be worth detecting directly.
fn is_python_project(repo_path: &Path) -> bool {
    [
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
        "pytest.ini",
        "requirements.txt",
    ]
    .iter()
    .any(|f| repo_path.join(f).exists())
}

/// Human-readable reason recorded on [`Score::unevaluated_reason`](lopi_core::Score::unevaluated_reason)
/// when [`detect`] finds nothing — Sprint F2 Phase 2's stated-reason
/// requirement: the block must say *why*, not just fail silently.
pub(crate) const NO_RUNNER_REASON: &str = "no recognized test runner for this repo (looked for \
     Cargo.toml, package.json/pnpm-lock.yaml/yarn.lock, a Python manifest \
     [pyproject.toml/setup.py/setup.cfg/pytest.ini/requirements.txt], go.mod, \
     build.gradle(.kts), or pom.xml) — configure `test_command` in \
     .lopi/loop.toml to name one explicitly";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), "").expect("write fixture file");
    }

    #[test]
    fn explicit_test_command_wins_over_every_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "Cargo.toml");
        assert_eq!(
            detect(dir.path(), Some("make test")),
            Some(Runner::Explicit("make test".to_string()))
        );
    }

    #[test]
    fn detects_cargo() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "Cargo.toml");
        assert_eq!(detect(dir.path(), None), Some(Runner::Cargo));
    }

    #[test]
    fn detects_pnpm_before_npm() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "package.json");
        touch(dir.path(), "pnpm-lock.yaml");
        assert_eq!(detect(dir.path(), None), Some(Runner::Pnpm));
    }

    #[test]
    fn detects_yarn_before_npm() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "package.json");
        touch(dir.path(), "yarn.lock");
        assert_eq!(detect(dir.path(), None), Some(Runner::Yarn));
    }

    #[test]
    fn detects_bare_npm() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "package.json");
        assert_eq!(detect(dir.path(), None), Some(Runner::Npm));
    }

    #[test]
    fn detects_pytest_via_pyproject() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "pyproject.toml");
        assert_eq!(detect(dir.path(), None), Some(Runner::Pytest));
    }

    #[test]
    fn detects_pytest_via_requirements_txt_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "requirements.txt");
        assert_eq!(detect(dir.path(), None), Some(Runner::Pytest));
    }

    #[test]
    fn detects_go() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "go.mod");
        assert_eq!(detect(dir.path(), None), Some(Runner::Go));
    }

    #[test]
    fn detects_gradle_kts() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "build.gradle.kts");
        assert_eq!(detect(dir.path(), None), Some(Runner::Gradle));
    }

    #[test]
    fn detects_maven() {
        let dir = tempfile::tempdir().expect("tempdir");
        touch(dir.path(), "pom.xml");
        assert_eq!(detect(dir.path(), None), Some(Runner::Maven));
    }

    #[test]
    fn detects_nothing_for_an_empty_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(detect(dir.path(), None), None);
    }
}
