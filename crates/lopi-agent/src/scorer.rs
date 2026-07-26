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
            score.errors.push(scorer_detect::NO_RUNNER_REASON.to_string());
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
            Runner::Pnpm => self.run_command("pnpm", &["test"], "pnpm test", score).await,
            Runner::Yarn => self.run_command("yarn", &["test"], "yarn test", score).await,
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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

    /// Init a real git repo at `repo` with one committed file, then modify
    /// `tracked_file` (writing `content`) so `should_skip_build_check`
    /// sees a real, non-docs, non-lockfile source change and `score()`
    /// actually runs detection instead of taking the "nothing changed" exit.
    fn init_repo_with_tracked_change(repo: &Path, tracked_file: &str, content: &str) {
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo)
            .status()
            .expect("git init");
        std::process::Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(repo)
            .status()
            .expect("git config email");
        std::process::Command::new("git")
            .args(["config", "user.name", "t"])
            .current_dir(repo)
            .status()
            .expect("git config name");
        std::fs::write(repo.join(".gitkeep"), "").expect("write .gitkeep");
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo)
            .status()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-q", "-m", "init"])
            .current_dir(repo)
            .status()
            .expect("git commit");
        std::fs::write(repo.join(tracked_file), content).expect("write tracked file");
    }

    /// KT-2.1 (Sprint F2) — reproduces the pre-fix defect this repo's own
    /// scorer had: a repo with no recognized manifest (no `Cargo.toml`,
    /// `package.json`, pytest/go/gradle/maven marker) and a real tracked
    /// source change scored `test_pass_rate = 1.0` — a perfect score having
    /// run zero tests. See `.konjo/killtests/F2/KT-2.1.md` for the recorded
    /// pre-fix output this test was built from.
    #[tokio::test]
    async fn unrecognized_stack_no_longer_reports_a_perfect_pass() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        init_repo_with_tracked_change(repo, "app.py", "print('hi')\ndef foo(): pass\n");

        let score = Scorer::new(repo).score().await.expect("score");

        // Pre-fix (see KT-2.1.md): `test_pass_rate == 1.0`, `passed() ==
        // true` — a passing score having run nothing. Post-Phase-2, an
        // unevaluable repo must never read as a pass, and must carry a
        // stated reason rather than a silent restriction.
        assert!(
            !score.passed(),
            "an unrecognized stack must never score as passing"
        );
        assert!(
            score.unevaluated_reason.is_some(),
            "the unevaluable case must carry a stated reason, not just a low number"
        );
    }

    /// Phase 1 verify — a pytest repo with a failing test scores as failing
    /// (not as an unrecognized-stack pass). Real `pytest` binary, real repo,
    /// per the anti-mocking rule for E2E coverage.
    #[tokio::test]
    async fn pytest_repo_with_a_failing_test_scores_as_failing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        std::fs::write(repo.join("pyproject.toml"), "[project]\nname = \"fixture\"\n")
            .expect("write pyproject.toml");
        init_repo_with_tracked_change(repo, "test_sample.py", "def test_fails():\n    assert False\n");

        let score = Scorer::new(repo).score().await.expect("score");

        assert!(!score.passed(), "a failing pytest run must not pass");
        assert!(score.unevaluated_reason.is_none(), "pytest was detected and ran — not unevaluated");
        assert_eq!(score.test_pass_rate, 0.0);
    }

    /// Phase 1 verify — a Go repo with a failing test scores as failing.
    /// Real `go test` binary, real repo.
    #[tokio::test]
    async fn go_repo_with_a_failing_test_scores_as_failing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        std::fs::write(repo.join("go.mod"), "module fixture\n\ngo 1.21\n").expect("write go.mod");
        init_repo_with_tracked_change(
            repo,
            "fixture_test.go",
            "package fixture\n\nimport \"testing\"\n\nfunc TestFails(t *testing.T) {\n\tt.Fatal(\"boom\")\n}\n",
        );

        let score = Scorer::new(repo).score().await.expect("score");

        assert!(!score.passed(), "a failing go test run must not pass");
        assert!(score.unevaluated_reason.is_none(), "go.mod was detected and ran — not unevaluated");
        assert_eq!(score.test_pass_rate, 0.0);
    }
}
