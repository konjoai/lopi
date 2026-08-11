//! Collision-Oracle-Build — advisory planning-prompt injection for
//! cross-agent textual collisions.
//!
//! Split out of `seed.rs` purely to keep that file under the 500-line CI
//! gate, matching the `builder.rs` precedent (see that file's own doc
//! comment). No new behavior beyond what's described here.
//!
//! Detection-only, opt-in, best-effort: unset (`with_collision_oracle` never
//! called) is behavior-identical to before this file existed. A resolution
//! or poll failure warns and returns nothing rather than blocking the
//! attempt — `lopi-oracle` is advisory, never a hard gate.

use super::AgentRunner;
use lopi_oracle::WatchedRef;

impl AgentRunner {
    /// Poll the shared collision oracle (if wired) against every sibling
    /// runner's registered ref, returning this task's share of any
    /// newly-observed collisions as bounded planning-prompt constraints.
    ///
    /// Registers this runner's own [`WatchedRef`] into the shared peer list
    /// on first call (idempotent — replaces any stale entry under the same
    /// label rather than duplicating it) and reuses it on every later
    /// attempt. A poll only ever returns *new* signatures (see
    /// `CollisionOracle::poll`), so a still-open collision from a prior
    /// attempt is not re-injected every retry — the first attempt that saw
    /// it already carried the warning forward via its own plan/constraints.
    pub(super) async fn seed_collision_alerts(&mut self) -> Vec<String> {
        let (Some(oracle), Some(peers)) = (&self.collision_oracle, &self.collision_peers) else {
            return vec![];
        };

        if self.collision_self_ref.is_none() {
            let Some(branch) = self.current_branch().await else {
                return vec![];
            };
            let self_ref = WatchedRef::new(self.task.id.0.to_string(), branch);
            {
                let mut guard = peers.lock().await;
                guard.retain(|r| r.label != self_ref.label);
                guard.push(self_ref.clone());
            }
            self.collision_self_ref = Some(self_ref);
        }

        let refs = { peers.lock().await.clone() };
        let alerts = {
            let mut oracle = oracle.lock().await;
            match oracle.poll(&refs).await {
                Ok(alerts) => alerts,
                Err(e) => {
                    self.warn(format!("collision-oracle: poll failed: {e}"));
                    return vec![];
                }
            }
        };

        let Some(self_ref) = &self.collision_self_ref else {
            return vec![];
        };
        let mine: Vec<String> = alerts
            .iter()
            .filter(|a| a.a_label == self_ref.label || a.b_label == self_ref.label)
            .map(lopi_oracle::Alert::advisory_text)
            .collect();
        if !mine.is_empty() {
            self.log(format!("⚠️ {} collision alert(s) injected", mine.len()));
        }
        mine
    }

    /// Resolve the branch currently checked out at `self.repo_path`. `None`
    /// on any git error or a detached `HEAD` (`--abbrev-ref` reports `HEAD`
    /// literally in that case, which is not a usable ref for cross-worktree
    /// collision checks — a detached run simply opts out of oracle wiring).
    async fn current_branch(&self) -> Option<String> {
        let out = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .await
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if branch.is_empty() || branch == "HEAD" {
            return None;
        }
        Some(branch)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::AgentRunner;
    use lopi_core::{AgentEvent, EventBus, Task};
    use lopi_oracle::CollisionOracle;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

    fn git(repo: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
    }

    /// A repo with two branches, `lopi/a` and `lopi/b`, both editing the
    /// same line of the same file — a real textual conflict.
    fn conflicting_repo() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        git(&path, &["init", "-b", "main"]);
        git(&path, &["config", "user.email", "t@konjoai.dev"]);
        git(&path, &["config", "user.name", "tester"]);
        std::fs::write(path.join("f.txt"), "line1\n").unwrap();
        git(&path, &["add", "."]);
        git(&path, &["commit", "-m", "init"]);

        git(&path, &["checkout", "-b", "lopi/a"]);
        std::fs::write(path.join("f.txt"), "line1\na-change\n").unwrap();
        git(&path, &["commit", "-am", "a change"]);

        git(&path, &["checkout", "main"]);
        git(&path, &["checkout", "-b", "lopi/b"]);
        std::fs::write(path.join("f.txt"), "line1\nb-change\n").unwrap();
        git(&path, &["commit", "-am", "b change"]);

        (dir, path)
    }

    fn runner_on_branch(repo: &std::path::Path, task_id_seed: &str) -> AgentRunner {
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let (_tx, rx) = tokio::sync::oneshot::channel();
        let task = Task::new(task_id_seed);
        AgentRunner::new(
            task,
            repo.to_path_buf(),
            bus,
            None,
            rx,
            Arc::new(AtomicUsize::new(0)),
        )
    }

    #[tokio::test]
    async fn unwired_runner_seeds_nothing() {
        let (_dir, repo) = conflicting_repo();
        git(&repo, &["checkout", "lopi/a"]);
        let mut runner = runner_on_branch(&repo, "task-a");
        assert!(runner.seed_collision_alerts().await.is_empty());
    }

    #[tokio::test]
    async fn two_sibling_runners_on_colliding_branches_produce_one_advisory() {
        let (_dir, repo) = conflicting_repo();
        let oracle = Arc::new(Mutex::new(CollisionOracle::new(&repo)));
        let peers = Arc::new(Mutex::new(Vec::new()));

        git(&repo, &["checkout", "lopi/b"]);
        let mut runner_b =
            runner_on_branch(&repo, "task-b").with_collision_oracle(oracle.clone(), peers.clone());
        // Register b's ref first so a's poll below sees both sides. b's own
        // first poll (with only itself registered) finds no pair to check.
        assert!(runner_b.seed_collision_alerts().await.is_empty());

        git(&repo, &["checkout", "lopi/a"]);
        let mut runner_a =
            runner_on_branch(&repo, "task-a").with_collision_oracle(oracle.clone(), peers.clone());
        let alerts = runner_a.seed_collision_alerts().await;
        assert_eq!(alerts.len(), 1, "a's poll now sees the a/b collision");
        assert!(alerts.first().unwrap().contains("f.txt"));

        // A second poll from either side must not re-inject the same
        // still-open collision (the direct KT-2 regression check).
        assert!(runner_a.seed_collision_alerts().await.is_empty());
    }

    #[tokio::test]
    async fn non_colliding_branch_pair_seeds_nothing() {
        let (_dir, repo) = conflicting_repo();
        git(&repo, &["checkout", "main"]);
        git(&repo, &["checkout", "-b", "lopi/c"]);
        std::fs::write(repo.join("g.txt"), "unrelated\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "c change"]);

        let oracle = Arc::new(Mutex::new(CollisionOracle::new(&repo)));
        let peers = Arc::new(Mutex::new(Vec::new()));

        git(&repo, &["checkout", "lopi/a"]);
        let mut runner_a =
            runner_on_branch(&repo, "task-a").with_collision_oracle(oracle.clone(), peers.clone());
        assert!(runner_a.seed_collision_alerts().await.is_empty());

        git(&repo, &["checkout", "lopi/c"]);
        let mut runner_c = runner_on_branch(&repo, "task-c").with_collision_oracle(oracle, peers);
        assert!(
            runner_c.seed_collision_alerts().await.is_empty(),
            "a and c touch disjoint files, no collision to report"
        );
    }
}
