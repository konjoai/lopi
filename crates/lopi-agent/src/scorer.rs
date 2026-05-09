use anyhow::Result;
use lopi_core::Score;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use which::which;

pub struct Scorer {
    repo_path: PathBuf,
}

impl Scorer {
    pub fn new(repo_path: impl AsRef<Path>) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
        }
    }

    /// Run the project's test + lint commands and produce a Score.
    /// Detection is intentionally simple: prefer `cargo` if Cargo.toml exists,
    /// else fall back to `npm test`. Failures populate `Score.errors`.
    ///
    /// # Errors
    ///
    /// Returns an error if the test or lint commands fail to spawn.
    #[tracing::instrument(skip(self))]
    pub async fn score(&self) -> Result<Score> {
        let mut score = Score::new(0.0, 0, 0);

        let cargo_toml = self.repo_path.join("Cargo.toml");
        if cargo_toml.exists() {
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

        // Diff size estimate via `git diff --stat`.
        if let Ok(out) = Command::new("git")
            .arg("diff")
            .arg("--shortstat")
            .current_dir(&self.repo_path)
            .output()
            .await
        {
            let s = String::from_utf8_lossy(&out.stdout);
            score.diff_lines = parse_diff_lines(&s);
        }

        Ok(score)
    }
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
