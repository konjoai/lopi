use crate::signature::ConflictSignature;
use crate::snapshot::{snapshot_pair, WatchedRef};
use anyhow::Result;
use std::collections::HashSet;
use std::path::PathBuf;

/// An advisory notice for one newly-observed collision: which refs collide,
/// on what files.
///
/// Never a hard block — see the crate-level docs for why this stays
/// detection-only. Callers surface [`advisory_text`](Alert::advisory_text) as
/// additional planning-prompt context, never as a `permissionDecision: deny`.
#[derive(Debug, Clone)]
pub struct Alert {
    /// Label of the first watched ref in the colliding pair.
    pub a_label: String,
    /// Label of the second watched ref in the colliding pair.
    pub b_label: String,
    /// Files the merge could not auto-resolve.
    pub files: Vec<String>,
}

impl Alert {
    /// Render this alert as advisory prompt text.
    ///
    /// Mirrors the advisory-string shape `lopi-agent`'s existing planning
    /// seeds already use (e.g. `reflection_constraint` in
    /// `crates/lopi-agent/src/runner/seed.rs`) — a labeled heads-up injected
    /// into the receiving agent's next planning pass, never a tool-call block.
    #[must_use]
    pub fn advisory_text(&self) -> String {
        let noun = if self.files.len() == 1 {
            "one file"
        } else {
            "these files"
        };
        format!(
            "Collision alert: `{}` and `{}` both touch {} ({}) and cannot auto-merge textually. \
             Coordinate before finishing this attempt, or expect a manual conflict resolution on merge.",
            self.a_label,
            self.b_label,
            noun,
            self.files.join(", "),
        )
    }
}

/// Textual collision detector over a set of live worktrees.
///
/// De-duplicates on [`ConflictSignature`]: a still-open collision alerts
/// once, and only re-alerts when its signature changes (the merge-base
/// advances, or the conflicted file set changes) or it resolves and later
/// recurs. This is the direct fix for KT-2's noise-floor finding
/// (`KILL_TEST_REGISTER.md`): a poll-and-count design that alerts on every
/// check reproduces a 120/hour rate off a single still-open collision.
/// `CollisionOracle` alerts on distinct collision *onsets*, not polls.
pub struct CollisionOracle {
    repo_path: PathBuf,
    seen: HashSet<ConflictSignature>,
}

impl CollisionOracle {
    /// Build an oracle watching `repo_path`.
    #[must_use]
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
            seen: HashSet::new(),
        }
    }

    /// How many signatures are currently tracked as still-open. Exposed for
    /// tests and observability, not load-bearing logic.
    #[must_use]
    pub fn open_count(&self) -> usize {
        self.seen.len()
    }

    /// Check every pair in `refs` for a textual collision, returning an
    /// [`Alert`] only for pairs whose [`ConflictSignature`] was not already
    /// open as of the previous poll. Signatures absent from this poll are
    /// forgotten, so a resolved-then-recurring collision alerts again as new
    /// information rather than staying silently suppressed forever.
    ///
    /// # Errors
    /// Returns `Err` if any pairwise `git merge-tree` invocation fails
    /// outright (a real git/process error, not a conflict). One failing pair
    /// aborts the whole poll rather than silently skipping it — a partial
    /// poll would under-report collisions with no signal that it did.
    pub async fn poll(&mut self, refs: &[WatchedRef]) -> Result<Vec<Alert>> {
        let mut alerts = Vec::new();
        let mut still_open = HashSet::new();

        for (i, a) in refs.iter().enumerate() {
            for b in &refs[i + 1..] {
                let Some(collision) = snapshot_pair(&self.repo_path, a, b).await? else {
                    continue;
                };
                let sig =
                    ConflictSignature::new(collision.files.clone(), collision.merge_base.clone());
                if !self.seen.contains(&sig) {
                    alerts.push(Alert {
                        a_label: a.label.clone(),
                        b_label: b.label.clone(),
                        files: collision.files,
                    });
                }
                still_open.insert(sig);
            }
        }

        self.seen = still_open;
        Ok(alerts)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(repo: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
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

    /// A repo on `main` with branches `a` and `b`, both editing the same
    /// line of the same file (a real textual conflict — KT-1's own dominant
    /// pattern, `CHANGELOG.md`-shaped contention), plus a non-conflicting
    /// `c`.
    fn conflicting_repo() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        git(&path, &["init", "-b", "main"]);
        git(&path, &["config", "user.email", "t@konjoai.dev"]);
        git(&path, &["config", "user.name", "tester"]);
        std::fs::write(path.join("f.txt"), "line1\n").unwrap();
        git(&path, &["add", "."]);
        git(&path, &["commit", "-m", "init"]);

        git(&path, &["checkout", "-b", "a"]);
        std::fs::write(path.join("f.txt"), "line1\na-change\n").unwrap();
        git(&path, &["commit", "-am", "a change"]);

        git(&path, &["checkout", "main"]);
        git(&path, &["checkout", "-b", "b"]);
        std::fs::write(path.join("f.txt"), "line1\nb-change\n").unwrap();
        git(&path, &["commit", "-am", "b change"]);

        git(&path, &["checkout", "main"]);
        git(&path, &["checkout", "-b", "c"]);
        std::fs::write(path.join("g.txt"), "unrelated\n").unwrap();
        git(&path, &["add", "."]);
        git(&path, &["commit", "-m", "c change"]);

        (dir, path)
    }

    #[tokio::test]
    async fn first_poll_alerts_on_the_real_conflict_and_not_on_the_clean_pair() {
        let (_dir, repo) = conflicting_repo();
        let mut oracle = CollisionOracle::new(&repo);
        let refs = vec![
            WatchedRef::new("task-a", "a"),
            WatchedRef::new("task-b", "b"),
            WatchedRef::new("task-c", "c"),
        ];

        let alerts = oracle.poll(&refs).await.unwrap();
        assert_eq!(alerts.len(), 1, "only the a/b pair collides");
        let alert = alerts.first().unwrap();
        assert_eq!(alert.files, vec!["f.txt".to_string()]);
        assert!(alert.advisory_text().contains("task-a"));
        assert!(alert.advisory_text().contains("task-b"));
        assert_eq!(oracle.open_count(), 1);
    }

    #[tokio::test]
    async fn a_still_open_collision_does_not_re_alert_on_the_next_poll() {
        // The direct KT-2 regression test: 12 polls of the same unresolved
        // collision must yield 1 alert, not 12.
        let (_dir, repo) = conflicting_repo();
        let mut oracle = CollisionOracle::new(&repo);
        let refs = vec![
            WatchedRef::new("task-a", "a"),
            WatchedRef::new("task-b", "b"),
        ];

        let first = oracle.poll(&refs).await.unwrap();
        assert_eq!(first.len(), 1);

        let mut total_re_alerts = 0;
        for _ in 0..11 {
            total_re_alerts += oracle.poll(&refs).await.unwrap().len();
        }
        assert_eq!(
            total_re_alerts, 0,
            "12 total polls of one still-open collision must yield exactly 1 alert, not 12"
        );
    }

    #[tokio::test]
    async fn a_resolved_collision_stops_alerting_and_a_recurrence_alerts_again() {
        let (_dir, repo) = conflicting_repo();
        let mut oracle = CollisionOracle::new(&repo);
        let refs = vec![
            WatchedRef::new("task-a", "a"),
            WatchedRef::new("task-b", "b"),
        ];

        assert_eq!(oracle.poll(&refs).await.unwrap().len(), 1);

        // Resolve: fast-forward `a` to merge `b` cleanly (repo built via file()).
        git(&repo, &["checkout", "a"]);
        git(&repo, &["merge", "-X", "theirs", "b", "-m", "resolve"]);

        assert_eq!(
            oracle.poll(&refs).await.unwrap().len(),
            0,
            "resolved collision must not alert"
        );
        assert_eq!(
            oracle.open_count(),
            0,
            "forgotten once no longer conflicting"
        );

        // Recur: `b` advances again with a new conflicting edit.
        git(&repo, &["checkout", "b"]);
        std::fs::write(repo.join("f.txt"), "line1\nb-change\nb-change-2\n").unwrap();
        git(&repo, &["commit", "-am", "b change again"]);

        git(&repo, &["checkout", "a"]);
        std::fs::write(repo.join("f.txt"), "line1\na-change\na-change-2\n").unwrap();
        git(&repo, &["commit", "-am", "a change again"]);

        let recurrence = oracle.poll(&refs).await.unwrap();
        assert_eq!(
            recurrence.len(),
            1,
            "a genuinely new signature must re-alert"
        );
    }

    #[tokio::test]
    async fn empty_and_singleton_ref_lists_yield_no_alerts() {
        let (_dir, repo) = conflicting_repo();
        let mut oracle = CollisionOracle::new(&repo);
        assert!(oracle.poll(&[]).await.unwrap().is_empty());
        assert!(oracle
            .poll(&[WatchedRef::new("task-a", "a")])
            .await
            .unwrap()
            .is_empty());
    }
}
