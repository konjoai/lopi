//! Phase 16.3 — L1–L4 autonomy-level enforcement at the end of a passing loop.
//!
//! The success path of [`AgentRunner::run`](super::AgentRunner) hands off to
//! [`AgentRunner::finalize`] once a score (or post-fix score) passes. The
//! autonomy ladder ([`AutonomyLevel`]) then dictates the outcome:
//!
//! | Level | Behaviour |
//! |-------|-----------|
//! | L1 `ReportOnly` | commit + emit a report; **no PR**, `pr_url: None` |
//! | L2 `DraftPr`    | commit + open a **draft** PR (the review is the gate) |
//! | L3 `VerifiedPr` | force the verifier on, then open a normal PR |
//! | L4 `AutoMerge`  | verifier + score gate, open a PR, then **auto-merge** |
//!
//! The pure decision functions ([`pr_decision`], [`requires_verifier`]) carry
//! the branching logic so it can be value-pinned in tests, keeping the
//! IO-bearing methods thin.

use super::AgentRunner;
use lopi_core::loop_config::AutonomyLevel;
use lopi_core::{AgentEvent, Deliverable, LoopConfig, Score, TaskStatus};
use lopi_git::GitManager;

/// What the runner should do with a passing attempt's branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PrDecision {
    /// L1 — emit a report only; never open a PR.
    ReportOnly,
    /// L2 — open a draft PR.
    Draft,
    /// L3 — open a normal PR (the verifier is enforced upstream).
    Normal,
    /// L4 — open a normal PR, then enable auto-merge.
    AutoMerge,
}

/// Map an [`AutonomyLevel`] to the end-of-loop [`PrDecision`].
pub(super) fn pr_decision(level: AutonomyLevel) -> PrDecision {
    match level {
        AutonomyLevel::ReportOnly => PrDecision::ReportOnly,
        AutonomyLevel::DraftPr => PrDecision::Draft,
        AutonomyLevel::VerifiedPr => PrDecision::Normal,
        AutonomyLevel::AutoMerge => PrDecision::AutoMerge,
    }
}

/// Whether the verifier must run before the PR for this attempt: either the
/// runner was explicitly built `with_verifier()`, or the autonomy level
/// (L3/L4) forces it on regardless.
pub(super) fn requires_verifier(verifier_enabled: bool, level: AutonomyLevel) -> bool {
    verifier_enabled || level.requires_verifier()
}

/// Whether to enable auto-merge: only for L4, and only when a PR was actually
/// opened (`pr_opened`) — never auto-merge a branch with no PR.
pub(super) fn should_auto_merge(decision: PrDecision, pr_opened: bool) -> bool {
    decision == PrDecision::AutoMerge && pr_opened
}

/// Whether a zero-diff attempt should conclude as a success rather than be
/// rejected for retry (intent-aware success). True when the goal is
/// review-only — it legitimately changes nothing — or whenever the loop's
/// `until` exit condition already fired, which ends the loop early regardless
/// of this attempt's own output.
pub(super) fn zero_diff_is_success(deliverable: Deliverable, until_satisfied: bool) -> bool {
    until_satisfied || deliverable.allows_zero_diff_success()
}

impl AgentRunner {
    /// Sprint Successor-1 — take the successor task derived from this run's
    /// completion, if any. Meant to be called once, after `run()` returns;
    /// the pool submits the returned task to the queue and attributes it to
    /// this run in `AgentEvent::TaskCompleted::successor`.
    pub fn take_pending_successor(&mut self) -> Option<lopi_core::Task> {
        self.pending_successor.take()
    }

    /// Finalize a passing attempt according to the task's autonomy level.
    ///
    /// Runs the verifier first when [`requires_verifier`] holds; on verifier
    /// rejection it rolls back, marks the task `Retrying`, and returns `None`
    /// so the caller continues to the next attempt. Otherwise it commits the
    /// work and applies the level's [`PrDecision`], returning the terminal
    /// [`TaskStatus::Success`] (with `pr_url: None` for L1).
    pub(super) async fn finalize(
        &mut self,
        branch: &str,
        git: &GitManager,
        score: &Score,
        until_satisfied: bool,
        attempt: u8,
    ) -> Option<TaskStatus> {
        let level = self.task.autonomy_level;
        // A1 — score the run against its explicit acceptance goal (if any)
        // *before* the autonomy-level verifier gate. Fail-closed: a non-passing
        // outcome rejects the finalize. Additive — a task with no acceptance is
        // untouched, and the verifier's own critique-routing below still fires.
        if !self.evaluate_acceptance_gate(score, attempt).await
            || (requires_verifier(self.verifier_enabled, level)
                && !self.run_verifier_pass(attempt, &score.errors).await)
        {
            git.hard_rollback().await.ok();
            git.checkout_default().await.ok();
            self.status(TaskStatus::Retrying { attempt }, attempt);
            return None;
        }

        // Zero-diff handling (intent-aware success). `commit_all` uses
        // libgit2, not the `git` CLI — unlike `git commit` (which refuses on
        // an empty tree), `Repository::commit` happily creates a commit whose
        // tree is byte-identical to its parent's, so an attempt that changed
        // nothing must never be committed or PR'd. But *what a zero diff
        // means* depends on the goal: a "write research.md" goal that wrote
        // nothing is a failure, while a "review the auth module" goal
        // legitimately produces no changes. `conclude_zero_diff` routes on the
        // task's `Deliverable` (see below). `score.diff_lines` is already
        // computed (`Scorer::score`, incl. untracked new files) — the single
        // source of truth for "is there anything to commit".
        if score.diff_lines == 0 {
            return self
                .conclude_zero_diff(branch, git, until_satisfied, attempt)
                .await;
        }

        self.log(format!("✅ finalizing ({}) — committing…", level.tag()));
        git.commit_all(&format!("lopi: {}", self.task.goal))
            .await
            .ok();
        let decision = pr_decision(level);
        // Land on the advanced default before pushing. L1 opens no PR, so skip.
        if decision != PrDecision::ReportOnly {
            if let Some(conflict) = self.rebase_before_pr(git).await {
                return Some(conflict);
            }
        }
        let pr_url = self
            .apply_pr_decision(decision, branch, git, score, attempt)
            .await;
        self.derive_and_stash_successor();
        Some(TaskStatus::Success {
            branch: branch.to_string(),
            pr_url,
        })
    }

    /// Sprint Successor-1 — derive this run's successor task (if any) and
    /// stash it for the pool to collect via
    /// [`take_pending_successor`](AgentRunner::take_pending_successor) once
    /// `run()` returns. Gated on `Task::successor_enabled`, mirroring
    /// `emit_report`'s "an unset lever changes nothing" precedent: a task
    /// with the default `successor_enabled: false` — every task before this
    /// sprint, and every task that doesn't opt in — takes this branch and
    /// stashes nothing.
    ///
    /// For this sprint the proposed [`Successor`](lopi_core::Successor) is
    /// `Task::successor_fixture` — a config/test-fixture value, never parsed
    /// from the agent's own output (that's Sprint Successor-2). A rejection
    /// from any containment gate is logged, never silent, and simply leaves
    /// no successor stashed — it does not fail this (already-successful)
    /// attempt.
    fn derive_and_stash_successor(&mut self) {
        if !self.task.successor_enabled {
            return;
        }
        let Some(successor) = self.task.successor_fixture.clone() else {
            return;
        };
        match lopi_core::derive_successor_task(
            &self.task,
            &successor,
            lopi_core::DEFAULT_MAX_CHAIN_DEPTH,
        ) {
            Ok(child) => {
                self.log(format!(
                    "🔗 successor derived: \"{}\" (depth {})",
                    child.goal, child.chain_depth
                ));
                self.pending_successor = Some(child);
            }
            Err(e) => {
                self.warn(format!("successor not derived: {e}"));
            }
        }
    }

    /// Decide what a zero-diff attempt means for this task (intent-aware
    /// success). A review-only goal — or any attempt whose `until` exit
    /// condition fired — legitimately concludes with no changes: check out
    /// the default and return `Success` (no PR, nothing to commit). A goal
    /// that expects file changes but produced none is *not* a success: roll
    /// back, seed the next planning prompt with a pointed critique (when
    /// adaptive retry is on), mark `Retrying`, and return `None` so the loop
    /// tries again (and ultimately fails honestly on `MaxIterations` rather
    /// than reporting a phantom `goal_met`).
    async fn conclude_zero_diff(
        &mut self,
        branch: &str,
        git: &GitManager,
        until_satisfied: bool,
        attempt: u8,
    ) -> Option<TaskStatus> {
        if zero_diff_is_success(self.task.deliverable_kind(), until_satisfied) {
            self.log("● no file changes produced — concluding (none expected for this goal)");
            git.checkout_default().await.ok();
            return Some(TaskStatus::Success {
                branch: branch.to_string(),
                pr_url: None,
            });
        }
        self.warn(
            "● no file changes produced, but this goal expects file edits — \
             rejecting attempt",
        );
        if self.adaptive_retry {
            self.last_error = Some(format!(
                "Attempt {attempt} finished without changing any files, but the goal \
                 requires creating or editing files. Use the Write/Edit tools to make \
                 the actual changes on disk before finishing — a summary is not enough."
            ));
        }
        git.hard_rollback().await.ok();
        git.checkout_default().await.ok();
        self.status(TaskStatus::Retrying { attempt }, attempt);
        None
    }

    /// Rebase the committed branch onto the advanced default before a PR.
    ///
    /// Returns `Some(TaskStatus::Conflict)` (after restoring a clean default
    /// checkout) when the rebase conflicts, so the loop stops with the colliding
    /// paths rather than opening a doomed PR. A non-conflict rebase error is
    /// logged and treated as "proceed" — pushing the un-rebased branch is safer
    /// than dropping the work — and a clean/no-op rebase returns `None`.
    async fn rebase_before_pr(&self, git: &GitManager) -> Option<TaskStatus> {
        match git.rebase_onto_default().await {
            Ok(conflicts) if !conflicts.is_empty() => {
                self.warn(format!("rebase conflict on: {}", conflicts.join(", ")));
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
                Some(TaskStatus::Conflict { paths: conflicts })
            }
            Ok(_) => None,
            Err(e) => {
                self.warn(format!("rebase skipped (non-conflict error): {e}"));
                None
            }
        }
    }

    /// Carry out the [`PrDecision`] for a committed branch, returning the PR
    /// URL (or `None` for L1 report-only / a failed `gh` invocation).
    async fn apply_pr_decision(
        &self,
        decision: PrDecision,
        branch: &str,
        git: &GitManager,
        score: &Score,
        attempt: u8,
    ) -> Option<String> {
        match decision {
            PrDecision::ReportOnly => {
                self.emit_report(branch, score, attempt);
                None
            }
            PrDecision::Draft => {
                let url = git.open_draft_pr(branch, &self.task.goal).await.ok();
                if let Some(ref u) = url {
                    self.log(format!("🔗 draft PR opened: {u}"));
                }
                url
            }
            PrDecision::Normal | PrDecision::AutoMerge => {
                let url = git.open_pr(branch, &self.task.goal).await.ok();
                if let Some(ref u) = url {
                    self.log(format!("🔗 PR opened: {u}"));
                }
                if should_auto_merge(decision, url.is_some()) {
                    match git.auto_merge(branch).await {
                        Ok(()) => self.log("🚀 auto-merge enabled (squash)"),
                        Err(e) => self.warn(format!("auto-merge failed: {e}")),
                    }
                }
                url
            }
        }
    }

    /// L1 — log a diff/score report in lieu of opening a PR, and — when the
    /// task declares a [`Task::report`](lopi_core::Task::report) channel —
    /// broadcast an [`AgentEvent::ReportReady`] so a subscriber (e.g.
    /// `lopi-remote`'s Telegram notifier) can deliver it.
    ///
    /// Report on Finish (Loop Engineering primitive 6). Reuses the existing
    /// `EventBus<AgentEvent>` instead of a direct `lopi-agent` → `lopi-remote`
    /// call, which would create a dependency cycle (`lopi-remote` already
    /// depends on `lopi-orchestrator`, which depends on `lopi-agent`) — see
    /// `LEDGER.md`'s Sprint 3 entry. An unset or unrecognized channel is
    /// never a silent no-op: `None` simply skips the broadcast, and an
    /// unparseable channel name warns loudly instead of being sent.
    fn emit_report(&self, branch: &str, score: &Score, attempt: u8) {
        self.log(format!(
            "📄 report-only (L1): branch={branch} pass={:.0}% lint={} diff={}L — no PR opened",
            score.test_pass_rate * 100.0,
            score.lint_errors,
            score.diff_lines,
        ));
        let Some(channel) = self.task.report.clone() else {
            return;
        };
        if let Err(e) = lopi_core::ReportChannel::parse(&channel) {
            self.warn(format!("report-on-finish: not sending — {e}"));
            return;
        }
        let summary = build_report_summary(&self.task.goal, branch, score, attempt);
        self.bus.send(AgentEvent::ReportReady {
            task_id: self.id(),
            channel,
            summary,
        });
    }

    /// Load the repo's `no_progress_limit` from `.lopi/loop.toml`, off the
    /// async reactor. Returns `0` (guard disabled) on any read/parse error so a
    /// malformed loop config can never wedge the retry loop.
    pub(super) async fn no_progress_limit(&self) -> u8 {
        let repo = self.repo_path.clone();
        tokio::task::spawn_blocking(move || {
            LoopConfig::load_from_repo(&repo)
                .map(|c| c.no_progress_limit)
                .unwrap_or(0)
        })
        .await
        .unwrap_or(0)
    }
}

/// Render the plain-text summary broadcast by [`AgentRunner::emit_report`].
/// Pure and IO-free so its wording is covered by unit tests independent of
/// the event bus. `emit_report` only ever calls this on a passing attempt
/// (see its own doc comment), so the verdict is always "pass" today; the
/// wording is still spelled out explicitly rather than assumed, so a future
/// failure-path caller has an honest word to change.
pub(super) fn build_report_summary(goal: &str, branch: &str, score: &Score, attempt: u8) -> String {
    format!(
        "📄 report — verdict: pass\n{goal}\nattempt {attempt} · pass {:.0}% · lint {} · diff {}L\nbranch: {branch}",
        score.test_pass_rate * 100.0,
        score.lint_errors,
        score.diff_lines,
    )
}

#[cfg(test)]
#[path = "finalize_tests.rs"]
mod tests;
