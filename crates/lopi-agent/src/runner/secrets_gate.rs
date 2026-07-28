//! Verification gate — secrets check (Finding #1).
//!
//! A stage that can fail is not a gate unless something actually blocks a bad
//! artifact from advancing. Before this module, a leaked credential in an
//! agent's diff had no dedicated check anywhere in the finalize path — only
//! [`crate::runner::verifier_runner`]'s model-graded review might happen to
//! notice one, and only if the rubric happened to ask. This is the
//! deterministic, no-model-call floor: scan the diff for known secret shapes
//! ([`lopi_core::scan_for_secrets`], the same pattern list
//! [`lopi_core::redact_secrets`] uses for log output) and refuse to commit if
//! any are found.
//!
//! Split out of `finalize.rs` purely to keep that file under the CLAUDE.md
//! file-size budget — pure code motion, same pattern as `test_phase.rs`'s
//! own doc comment.

use super::AgentRunner;
use lopi_core::TaskStatus;
use lopi_git::GitManager;

impl AgentRunner {
    /// Scan `diff` for known secret shapes before it is ever committed.
    ///
    /// Returns `true` when a leak was found — the caller must not proceed to
    /// commit; this method has already rolled back and marked the attempt
    /// `Retrying`. Evidence carried into the next attempt's prompt (when
    /// adaptive retry is enabled) names only the pattern *labels* found,
    /// never the matched values — `self.last_error` reaches the next
    /// planning prompt verbatim, so leaking the secret into the evidence
    /// meant to prevent the leak would defeat the whole point.
    pub(super) async fn secrets_gate(&mut self, diff: &str, git: &GitManager, attempt: u8) -> bool {
        let labels = lopi_core::scan_for_secrets(diff);
        if labels.is_empty() {
            return false;
        }
        let msg = secrets_gate_message(&labels);
        self.warn(format!("🔒 {msg}"));
        if self.adaptive_retry {
            self.last_error = Some(format!(
                "Attempt {attempt} was rejected before commit: {msg} Remove the leaked \
                 credential(s) from the diff and use environment variables or a secrets \
                 manager instead of hardcoding them."
            ));
        }
        git.hard_rollback().await.ok();
        git.checkout_default().await.ok();
        self.status(TaskStatus::Retrying { attempt }, attempt);
        true
    }
}

/// Render the secrets-gate rejection message. Pure so the wording is
/// unit-tested without git/IO — mirrors `finalize.rs`'s
/// `build_report_summary` precedent.
fn secrets_gate_message(labels: &[&str]) -> String {
    format!(
        "secrets gate blocked the commit — {} leaked credential pattern(s) found in the diff: {}",
        labels.len(),
        labels.join(", ")
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopi_core::{AgentEvent, Task};
    use std::path::PathBuf;

    fn git_cmd(repo: &std::path::Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A real, minimal git repo — `secrets_gate` calls `git.hard_rollback()`
    /// / `git.checkout_default()`, which need an actual `.git` to operate on.
    fn init_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path().to_path_buf();
        git_cmd(&repo, &["init", "-b", "main"]);
        git_cmd(&repo, &["config", "user.email", "t@konjoai.dev"]);
        git_cmd(&repo, &["config", "user.name", "tester"]);
        std::fs::write(repo.join("file.txt"), "base\n").unwrap();
        git_cmd(&repo, &["add", "."]);
        git_cmd(&repo, &["commit", "-m", "base"]);
        (dir, repo)
    }

    #[test]
    fn message_names_labels_not_values() {
        let msg = secrets_gate_message(&["anthropic_key", "aws_access_key_id"]);
        assert!(msg.contains("anthropic_key"));
        assert!(msg.contains("aws_access_key_id"));
        assert!(msg.contains('2'), "count is stated: {msg}");
    }

    #[test]
    fn message_singular_count() {
        let msg = secrets_gate_message(&["github_token"]);
        assert!(msg.contains('1'));
        assert!(msg.contains("github_token"));
    }

    #[tokio::test]
    async fn clean_diff_is_not_blocked() {
        let (_dir, repo) = init_repo();
        let task = Task::new("write a helper function");
        let (mut runner, _bus) = AgentRunner::standalone(task, repo.clone());
        let git = GitManager::new(&repo).unwrap();
        let blocked = runner
            .secrets_gate("+pub fn add(a: i32, b: i32) -> i32 { a + b }", &git, 1)
            .await;
        assert!(!blocked);
        assert!(runner.last_error.is_none());
    }

    #[tokio::test]
    async fn leaked_key_blocks_and_marks_retrying_without_leaking_the_value() {
        let (_dir, repo) = init_repo();
        let task = Task::new("wire up the api client");
        let (mut runner, bus) = AgentRunner::standalone(task, repo.clone());
        runner.adaptive_retry = true;
        let mut rx = bus.subscribe();
        let git = GitManager::new(&repo).unwrap();
        let secret = "sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789";
        let diff = format!("+const KEY: &str = \"{secret}\";");

        let blocked = runner.secrets_gate(&diff, &git, 2).await;
        assert!(blocked);

        let last_error = runner.last_error.expect("adaptive retry stashes evidence");
        assert!(last_error.contains("anthropic_key"));
        assert!(
            !last_error.contains(secret),
            "the retry prompt must never carry the leaked value forward"
        );

        let mut saw_retrying = false;
        while let Ok(ev) = rx.try_recv() {
            if let AgentEvent::StatusChanged {
                status: TaskStatus::Retrying { attempt },
                ..
            } = ev
            {
                saw_retrying = true;
                assert_eq!(attempt, 2);
            }
        }
        assert!(
            saw_retrying,
            "must mark the attempt Retrying, not silently drop it"
        );
    }

    #[tokio::test]
    async fn leaked_key_without_adaptive_retry_still_blocks_but_stashes_no_evidence() {
        let (_dir, repo) = init_repo();
        let task = Task::new("wire up the api client");
        let (mut runner, _bus) = AgentRunner::standalone(task, repo.clone());
        let git = GitManager::new(&repo).unwrap();
        let diff = "+token = \"ghp_abcdefghijklmnopqrstuvwxyz0123456789AB\"";

        let blocked = runner.secrets_gate(diff, &git, 1).await;
        assert!(blocked);
        assert!(
            runner.last_error.is_none(),
            "adaptive_retry is off — no evidence stashing, but still blocked"
        );
    }
}
