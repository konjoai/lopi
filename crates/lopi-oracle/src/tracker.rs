use crate::signature::ConflictSignature;
use crate::snapshot::{snapshot_pair, WatchedRef};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
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
/// De-duplicates on [`ConflictSignature`] **per notified side**: a still-open
/// collision alerts each participant once, and only re-alerts when its
/// signature changes (the merge-base advances, or the conflicted file set
/// changes) or it resolves and later recurs. This is the direct fix for
/// KT-2's noise-floor finding (`KILL_TEST_REGISTER.md`): a poll-and-count
/// design that alerts on every check reproduces a 120/hour rate off a single
/// still-open collision. `CollisionOracle` alerts on distinct collision
/// *onsets*, not polls.
///
/// The ledger is keyed by signature *and* by the label it was delivered to,
/// rather than by signature alone. A signature-only ledger makes delivery a
/// race: whichever side of a colliding pair polls first consumes the alert,
/// and the other side never learns it is colliding at all. Both sides of a
/// collision need the warning — neither can coordinate on information only
/// its counterpart received.
pub struct CollisionOracle {
    repo_path: PathBuf,
    /// Still-open signatures, each mapped to the set of labels already
    /// notified of it. A signature absent from a poll is dropped entirely,
    /// so a resolved-then-recurring collision alerts every side afresh.
    delivered: HashMap<ConflictSignature, HashSet<String>>,
}

impl CollisionOracle {
    /// Build an oracle watching `repo_path`.
    #[must_use]
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
            delivered: HashMap::new(),
        }
    }

    /// How many distinct signatures are currently tracked as still-open.
    /// Exposed for tests and observability, not load-bearing logic.
    #[must_use]
    pub fn open_count(&self) -> usize {
        self.delivered.len()
    }

    /// Check every pair in `refs` for a textual collision, returning the
    /// [`Alert`]s that `requester_label` has not already been told about.
    ///
    /// An alert is returned only when `requester_label` is one side of the
    /// colliding pair — a caller is never handed a collision between two
    /// other agents, which it could not act on — and only when that
    /// signature has not previously been delivered to *this* label.
    /// Signatures absent from this poll are forgotten, so a
    /// resolved-then-recurring collision alerts again as new information
    /// rather than staying silently suppressed forever.
    ///
    /// # Errors
    /// Returns `Err` if any pairwise `git merge-tree` invocation fails
    /// outright (a real git/process error, not a conflict). One failing pair
    /// aborts the whole poll rather than silently skipping it — a partial
    /// poll would under-report collisions with no signal that it did.
    pub async fn poll_for(
        &mut self,
        requester_label: &str,
        refs: &[WatchedRef],
    ) -> Result<Vec<Alert>> {
        let mut alerts = Vec::new();
        let mut still_open: HashMap<ConflictSignature, HashSet<String>> = HashMap::new();

        for (i, a) in refs.iter().enumerate() {
            for b in &refs[i + 1..] {
                let Some(collision) = snapshot_pair(&self.repo_path, a, b).await? else {
                    continue;
                };
                let sig =
                    ConflictSignature::new(collision.files.clone(), collision.merge_base.clone());
                // Carry the prior notification set forward, so a signature
                // seen across several polls accumulates its recipients
                // instead of resetting each cycle. Two distinct pairs can
                // hash to one signature; `entry` merges them rather than
                // letting the later pair overwrite the earlier one's record.
                let prior = self.delivered.get(&sig).cloned().unwrap_or_default();
                let notified = still_open.entry(sig).or_insert(prior);

                let involves_requester = a.label == requester_label || b.label == requester_label;
                if involves_requester && notified.insert(requester_label.to_string()) {
                    alerts.push(Alert {
                        a_label: a.label.clone(),
                        b_label: b.label.clone(),
                        files: collision.files,
                    });
                }
            }
        }

        self.delivered = still_open;
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

        let alerts = oracle.poll_for("task-a", &refs).await.unwrap();
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

        let first = oracle.poll_for("task-a", &refs).await.unwrap();
        assert_eq!(first.len(), 1);

        let mut total_re_alerts = 0;
        for _ in 0..11 {
            total_re_alerts += oracle.poll_for("task-a", &refs).await.unwrap().len();
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

        assert_eq!(oracle.poll_for("task-a", &refs).await.unwrap().len(), 1);

        // Resolve: fast-forward `a` to merge `b` cleanly (repo built via file()).
        git(&repo, &["checkout", "a"]);
        git(&repo, &["merge", "-X", "theirs", "b", "-m", "resolve"]);

        assert_eq!(
            oracle.poll_for("task-a", &refs).await.unwrap().len(),
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

        let recurrence = oracle.poll_for("task-a", &refs).await.unwrap();
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
        assert!(oracle.poll_for("task-a", &[]).await.unwrap().is_empty());
        assert!(oracle
            .poll_for("task-a", &[WatchedRef::new("task-a", "a")])
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn both_sides_of_a_collision_each_receive_the_alert_exactly_once() {
        // The fairness regression test. Under a signature-only ledger the
        // first caller to poll consumed the alert and the second was told
        // nothing, so only one of two colliding agents ever learned it was
        // colliding. Both sides must be told, and neither twice.
        let (_dir, repo) = conflicting_repo();
        let mut oracle = CollisionOracle::new(&repo);
        let refs = vec![
            WatchedRef::new("task-a", "a"),
            WatchedRef::new("task-b", "b"),
        ];

        let to_a = oracle.poll_for("task-a", &refs).await.unwrap();
        assert_eq!(to_a.len(), 1, "the first side must be told");

        let to_b = oracle.poll_for("task-b", &refs).await.unwrap();
        assert_eq!(
            to_b.len(),
            1,
            "the second side must be told too, not starved by the first side's poll"
        );
        assert!(to_b.first().unwrap().advisory_text().contains("task-b"));

        assert!(
            oracle.poll_for("task-a", &refs).await.unwrap().is_empty(),
            "no side re-alerts on a still-open collision"
        );
        assert!(oracle.poll_for("task-b", &refs).await.unwrap().is_empty());
        assert_eq!(oracle.open_count(), 1, "still one distinct signature");
    }

    #[tokio::test]
    async fn a_collision_between_two_other_agents_is_not_delivered() {
        // `task-c` does not conflict with anyone. It must not be handed the
        // a/b collision, which it cannot act on.
        let (_dir, repo) = conflicting_repo();
        let mut oracle = CollisionOracle::new(&repo);
        let refs = vec![
            WatchedRef::new("task-a", "a"),
            WatchedRef::new("task-b", "b"),
            WatchedRef::new("task-c", "c"),
        ];

        assert!(
            oracle.poll_for("task-c", &refs).await.unwrap().is_empty(),
            "an uninvolved agent receives nothing"
        );
        assert_eq!(
            oracle.open_count(),
            1,
            "the a/b signature is still tracked as open"
        );
        assert_eq!(
            oracle.poll_for("task-a", &refs).await.unwrap().len(),
            1,
            "and task-c's poll did not consume task-a's alert"
        );
    }
}
