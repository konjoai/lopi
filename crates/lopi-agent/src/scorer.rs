use anyhow::Result;
use lopi_core::Score;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use which::which;

/// Runs tests and linters against a repository and produces a `Score`.
pub struct Scorer {
    repo_path: PathBuf,
}

impl Scorer {
    /// Create a scorer rooted at `repo_path`.
    pub fn new(repo_path: impl AsRef<Path>) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
        }
    }

    /// Run the project's test + lint commands and produce a Score.
    /// Detection is intentionally simple: prefer `cargo` if Cargo.toml exists,
    /// else fall back to `npm test`. Failures populate `Score.errors`.
    ///
    /// Skips test/lint entirely when every changed path is docs-only (see
    /// `changed_paths`/`is_docs_path`) — a goal that only touches `*.md`
    /// files has no reason to run the target repo's build, and doing so
    /// anyway means a docs-only task can never pass in a repo that doesn't
    /// build yet (no `src/`), burning every retry on a gate it was never
    /// asked to satisfy.
    ///
    /// # Errors
    ///
    /// Returns an error if the test or lint commands fail to spawn.
    #[tracing::instrument(skip(self))]
    pub async fn score(&self) -> Result<Score> {
        let mut score = Score::new(0.0, 0, 0);

        let changed = self.changed_paths().await.unwrap_or_else(|err| {
            tracing::warn!(%err, "git status failed — falling back to full test/lint");
            Vec::new()
        });
        let skip_build_check = should_skip_build_check(&changed);

        let cargo_toml = self.repo_path.join("Cargo.toml");
        if skip_build_check {
            score.test_pass_rate = 1.0;
            tracing::info!(?changed, "no source changes to verify — skipping test/lint");
        } else if cargo_toml.exists() {
            // cargo test — use sccache if available to skip unchanged artifact recompilation
            let mut cmd = Command::new("cargo");
            if which("sccache").is_ok() {
                cmd.env("RUSTC_WRAPPER", "sccache");
            }
            let out = cmd
                .arg("test")
                .arg("--quiet")
                .current_dir(&self.repo_path)
                .output()
                .await?;
            score.test_pass_rate = if out.status.success() { 1.0 } else { 0.0 };
            if !out.status.success() {
                score.errors.push(format!(
                    "cargo test failed:\n{}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            // cargo clippy as the lint signal.
            let mut cmd = Command::new("cargo");
            if which("sccache").is_ok() {
                cmd.env("RUSTC_WRAPPER", "sccache");
            }
            let lint = cmd
                .arg("clippy")
                .arg("--quiet")
                .arg("--")
                .arg("-D")
                .arg("warnings")
                .current_dir(&self.repo_path)
                .output()
                .await;
            if let Ok(lint) = lint {
                if !lint.status.success() {
                    score.lint_errors = 1;
                    score.errors.push(format!(
                        "clippy failed:\n{}",
                        String::from_utf8_lossy(&lint.stderr)
                    ));
                }
            }
        } else if self.repo_path.join("package.json").exists() {
            let out = Command::new("npm")
                .arg("test")
                .current_dir(&self.repo_path)
                .output()
                .await?;
            score.test_pass_rate = if out.status.success() { 1.0 } else { 0.0 };
            if !out.status.success() {
                score.errors.push(format!(
                    "npm test failed:\n{}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        } else {
            // No detectable test runner — treat as passing with a warning.
            score.test_pass_rate = 1.0;
            score.errors.push("no test runner detected".into());
        }

        // Diff size estimate: tracked changes via `git diff --shortstat`,
        // plus full line counts of untracked new files — `git diff` never
        // sees untracked paths, so a docs-only task that creates a brand new
        // `research.md` would otherwise always score `diff=0L` despite real
        // content having been written. Tracked-modified files are covered by
        // `--shortstat` alone; counting their full content here too would
        // double-count them.
        let mut diff_lines = 0u32;
        if let Ok(out) = Command::new("git")
            .arg("diff")
            .arg("--shortstat")
            .current_dir(&self.repo_path)
            .output()
            .await
        {
            diff_lines += parse_diff_lines(&String::from_utf8_lossy(&out.stdout));
        }
        for (untracked, path) in &changed {
            if !untracked {
                continue;
            }
            let full = self.repo_path.join(path);
            if let Ok(content) = tokio::fs::read_to_string(&full).await {
                diff_lines += content.lines().count() as u32;
            }
        }
        score.diff_lines = diff_lines;

        Ok(score)
    }

    /// Paths with pending changes (staged, unstaged, or untracked) relative
    /// to `repo_path`, via `git status --porcelain`, tagged with whether
    /// each is untracked (`??`). Used both to decide whether a diff is
    /// docs-only and to size untracked new files for the diff-line estimate
    /// above.
    async fn changed_paths(&self) -> Result<Vec<(bool, String)>> {
        let out = Command::new("git")
            .arg("status")
            .arg("--porcelain")
            .current_dir(&self.repo_path)
            .output()
            .await?;
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(parse_porcelain_line)
            .collect())
    }
}

/// Parse one `git status --porcelain` line ("XY path" or, for renames,
/// "XY old -> new") into (is-untracked, path).
fn parse_porcelain_line(line: &str) -> Option<(bool, String)> {
    let status = line.get(0..2)?;
    let rest = line.get(3..)?;
    let path = rest.split(" -> ").last().unwrap_or(rest);
    Some((status == "??", path.trim().to_string()))
}

/// Whether `score()` should skip `cargo test`/`clippy` entirely: true when
/// every changed path is docs-only or a package-manager lockfile, *and* —
/// via `Iterator::all` being vacuously true on an empty slice — when nothing
/// changed at all (e.g. an attempt that halted before writing anything).
/// None of these represent a real source change to verify; running the real
/// build check anyway against a target repo with no compilable code produces
/// a false `pass=0%` failure rather than the honest "nothing to check" this
/// is. Lockfiles are included because attempt branches carry no intermediate
/// commits until `finalize` succeeds (`GitManager::commit_all`), so `score()`
/// can't diff against a base branch to isolate *this* attempt's change from
/// working-tree noise — and the most common noise is `Scorer`'s own prior
/// `cargo test`/`clippy` invocation regenerating `Cargo.lock`, which then
/// makes a genuinely docs-only attempt look source-touching on the very next
/// `changed_paths()` read.
fn should_skip_build_check(changed: &[(bool, String)]) -> bool {
    changed
        .iter()
        .all(|(_, path)| is_docs_path(path) || is_lockfile_path(path))
}

/// True for paths that can't affect a build/lint/test result — the set this
/// gates on for "should we even run the target repo's test suite".
fn is_docs_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".md")
        || lower.ends_with(".mdx")
        || lower.ends_with(".rst")
        || lower.ends_with(".txt")
    {
        return true;
    }
    if lower.starts_with("docs/") || lower.contains("/docs/") {
        return true;
    }
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    matches!(
        base,
        "readme" | "license" | "changelog" | "authors" | "contributing"
    )
}

/// True for a package-manager lockfile — tooling-regenerated, never hand-
/// authored by an attempt, and never itself the cause of a real test/lint
/// failure. A lockfile changing alongside real source is still caught by
/// `should_skip_build_check`'s `all()` (the source path fails this check),
/// so this only widens what counts as "nothing to verify", never narrows it.
fn is_lockfile_path(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    matches!(
        base,
        "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml"
    )
}

fn parse_diff_lines(stat: &str) -> u32 {
    // Format: " 3 files changed, 42 insertions(+), 7 deletions(-)"
    let mut total: u32 = 0;
    for chunk in stat.split(',') {
        let t = chunk.trim();
        if let Some(num) = t.split_whitespace().next() {
            if let Ok(n) = num.parse::<u32>() {
                if t.contains("insertion") || t.contains("deletion") {
                    total += n;
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_paths_match_markdown_and_docs_dir() {
        assert!(is_docs_path("research.md"));
        assert!(is_docs_path("NOTES.MD"));
        assert!(is_docs_path("docs/architecture.rst"));
        assert!(is_docs_path("nested/docs/plan.txt"));
        assert!(is_docs_path("README"));
        assert!(is_docs_path("path/to/CHANGELOG"));
    }

    #[test]
    fn non_docs_paths_are_rejected() {
        assert!(!is_docs_path("src/main.rs"));
        assert!(!is_docs_path("Cargo.toml"));
        assert!(!is_docs_path("Cargo.lock"));
        assert!(!is_docs_path("package.json"));
        assert!(!is_docs_path("scripts/build.sh"));
    }

    #[test]
    fn lockfile_paths_are_recognized() {
        assert!(is_lockfile_path("Cargo.lock"));
        assert!(is_lockfile_path("nested/Cargo.lock"));
        assert!(is_lockfile_path("package-lock.json"));
        assert!(is_lockfile_path("yarn.lock"));
        assert!(is_lockfile_path("pnpm-lock.yaml"));
        assert!(!is_lockfile_path("Cargo.toml"));
        assert!(!is_lockfile_path("src/main.rs"));
    }

    #[test]
    fn skip_build_check_when_nothing_changed() {
        // The regression this guards: an attempt that halted before writing
        // anything used to fall through to a real `cargo test`/`clippy` run
        // against a target repo with no compilable code.
        assert!(should_skip_build_check(&[]));
    }

    /// The regression this guards: a docs-only attempt (`research.md`) whose
    /// working tree also carries a `Cargo.lock` regenerated by the Scorer's
    /// own earlier `cargo test`/`clippy` invocation used to read as
    /// source-touching — whole-tree `git status` can't distinguish "the
    /// attempt changed this" from "a prior probe run touched this" — forcing
    /// a real build check against a target repo whose broken/empty scaffold
    /// guarantees a false `pass=0%`.
    #[test]
    fn skip_build_check_when_docs_and_lockfile_only() {
        assert!(should_skip_build_check(&[
            (true, "research.md".to_string()),
            (false, "Cargo.lock".to_string()),
        ]));
    }

    #[test]
    fn skip_build_check_when_only_docs_changed() {
        assert!(should_skip_build_check(&[
            (true, "research.md".to_string()),
            (false, "docs/notes.md".to_string()),
        ]));
    }

    #[test]
    fn does_not_skip_build_check_when_source_changed() {
        assert!(!should_skip_build_check(&[
            (true, "research.md".to_string()),
            (false, "src/main.rs".to_string()),
        ]));
    }

    #[test]
    fn porcelain_line_parses_untracked() {
        assert_eq!(
            parse_porcelain_line("?? research.md"),
            Some((true, "research.md".to_string()))
        );
    }

    #[test]
    fn porcelain_line_parses_modified_tracked() {
        assert_eq!(
            parse_porcelain_line(" M src/main.rs"),
            Some((false, "src/main.rs".to_string()))
        );
    }

    #[test]
    fn porcelain_line_parses_rename_to_new_path() {
        assert_eq!(
            parse_porcelain_line("R  old.md -> new.md"),
            Some((false, "new.md".to_string()))
        );
    }

    #[test]
    fn parse_diff_lines_sums_insertions_and_deletions() {
        assert_eq!(
            parse_diff_lines(" 3 files changed, 42 insertions(+), 7 deletions(-)"),
            49
        );
    }

    #[test]
    fn parse_diff_lines_handles_empty_stat() {
        assert_eq!(parse_diff_lines(""), 0);
    }
}
