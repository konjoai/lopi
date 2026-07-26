use anyhow::Result;
use lopi_core::Score;
use scorer_detect::Runner;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use which::which;

#[path = "scorer_detect.rs"]
mod scorer_detect;

/// Runs tests and linters against a repository and produces a `Score`.
pub struct Scorer {
    repo_path: PathBuf,
    /// Sprint F2 Phase 1 — `.lopi/loop.toml`'s `test_command` escape hatch.
    /// When set, always wins over stack detection (see `scorer_detect::detect`).
    test_command: Option<String>,
}

impl Scorer {
    /// Create a scorer rooted at `repo_path`, with no `test_command` override.
    pub fn new(repo_path: impl AsRef<Path>) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
            test_command: None,
        }
    }

    /// Wire the repo's `.lopi/loop.toml` `test_command` override, if any —
    /// an explicit escape hatch for stacks `scorer_detect::detect` can't
    /// recognize. `None` (the default) leaves detection as the sole source.
    #[must_use]
    pub fn with_test_command(mut self, test_command: Option<String>) -> Self {
        self.test_command = test_command;
        self
    }

    /// Run the project's test + lint commands and produce a Score.
    ///
    /// Detection (`scorer_detect::detect`) prefers an explicit
    /// `test_command` override, then a recognized manifest (Rust, Node via
    /// npm/pnpm/yarn, Python, Go, Gradle, Maven) in that order. A repo with
    /// none of these gets a *blocking*, stated-reason
    /// [`unevaluated_reason`](lopi_core::Score::unevaluated_reason) — Sprint
    /// F2 Phase 2 — rather than the score reading as a pass.
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

        if skip_build_check {
            score.test_pass_rate = 1.0;
            tracing::info!(?changed, "no source changes to verify — skipping test/lint");
        } else {
            self.run_detected(&mut score).await?;
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

    /// Detect and run this repo's test runner, populating `score` in place.
    /// Sprint F2 Phase 2 — a repo with no recognized runner (and no
    /// `test_command` override) gets a blocking, stated
    /// `unevaluated_reason` rather than `test_pass_rate = 1.0`: "I could not
    /// evaluate this" must never read as "this passed".
    async fn run_detected(&self, score: &mut Score) -> Result<()> {
        let Some(runner) = scorer_detect::detect(&self.repo_path, self.test_command.as_deref())
        else {
            score
                .errors
                .push(scorer_detect::NO_RUNNER_REASON.to_string());
            score.unevaluated_reason = Some(scorer_detect::NO_RUNNER_REASON.to_string());
            return Ok(());
        };
        match runner {
            Runner::Cargo => self.run_cargo(score).await,
            Runner::Explicit(cmd) => {
                self.run_command("sh", &["-c", &cmd], "configured test_command", score)
                    .await
            }
            Runner::Gradle => {
                let program = if self.repo_path.join("gradlew").exists() {
                    "./gradlew"
                } else {
                    "gradle"
                };
                self.run_command(program, &["test"], "gradle test", score)
                    .await
            }
            Runner::Npm => self.run_command("npm", &["test"], "npm test", score).await,
            Runner::Pnpm => {
                self.run_command("pnpm", &["test"], "pnpm test", score)
                    .await
            }
            Runner::Yarn => {
                self.run_command("yarn", &["test"], "yarn test", score)
                    .await
            }
            Runner::Pytest => self.run_command("pytest", &[], "pytest", score).await,
            Runner::Go => {
                self.run_command("go", &["test", "./..."], "go test", score)
                    .await
            }
            Runner::Maven => self.run_command("mvn", &["test"], "mvn test", score).await,
        }
    }

    /// `cargo test` (the only runner with a paired lint signal, `cargo
    /// clippy`) — moved verbatim from the pre-Phase-1 `score()` body.
    async fn run_cargo(&self, score: &mut Score) -> Result<()> {
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
        Ok(())
    }

    /// Run a single test-runner invocation and record pass/fail on `score`.
    /// Shared by every non-Cargo runner (npm/pnpm/yarn/pytest/go/gradle/maven
    /// and the `test_command` escape hatch) — none of these have a paired
    /// lint step, only `Score.test_pass_rate`.
    async fn run_command(
        &self,
        program: &str,
        args: &[&str],
        label: &str,
        score: &mut Score,
    ) -> Result<()> {
        let out = Command::new(program)
            .args(args)
            .current_dir(&self.repo_path)
            .output()
            .await?;
        score.test_pass_rate = if out.status.success() { 1.0 } else { 0.0 };
        if !out.status.success() {
            score.errors.push(format!(
                "{label} failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(())
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
#[path = "scorer_tests.rs"]
mod tests;
