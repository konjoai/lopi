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
use lopi_core::{Score, TaskStatus};
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

impl AgentRunner {
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
        attempt: u8,
    ) -> Option<TaskStatus> {
        let level = self.task.autonomy_level;
        if requires_verifier(self.verifier_enabled, level)
            && !self.run_verifier_pass(attempt, &score.errors).await
        {
            git.hard_rollback().await.ok();
            git.checkout_default().await.ok();
            self.status(TaskStatus::Retrying { attempt }, attempt);
            return None;
        }

        self.log(format!("✅ finalizing ({}) — committing…", level.tag()));
        git.commit_all(&format!("lopi: {}", self.task.goal))
            .await
            .ok();
        let pr_url = self
            .apply_pr_decision(pr_decision(level), branch, git, score)
            .await;
        Some(TaskStatus::Success {
            branch: branch.to_string(),
            pr_url,
        })
    }

    /// Carry out the [`PrDecision`] for a committed branch, returning the PR
    /// URL (or `None` for L1 report-only / a failed `gh` invocation).
    async fn apply_pr_decision(
        &self,
        decision: PrDecision,
        branch: &str,
        git: &GitManager,
        score: &Score,
    ) -> Option<String> {
        match decision {
            PrDecision::ReportOnly => {
                self.emit_report(branch, score);
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

    /// L1 — log a diff/score report in lieu of opening a PR.
    fn emit_report(&self, branch: &str, score: &Score) {
        self.log(format!(
            "📄 report-only (L1): branch={branch} pass={:.0}% lint={} diff={}L — no PR opened",
            score.test_pass_rate * 100.0,
            score.lint_errors,
            score.diff_lines,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{pr_decision, requires_verifier, should_auto_merge, PrDecision};
    use lopi_core::loop_config::AutonomyLevel;

    #[test]
    fn each_level_maps_to_its_decision() {
        assert_eq!(
            pr_decision(AutonomyLevel::ReportOnly),
            PrDecision::ReportOnly
        );
        assert_eq!(pr_decision(AutonomyLevel::DraftPr), PrDecision::Draft);
        assert_eq!(pr_decision(AutonomyLevel::VerifiedPr), PrDecision::Normal);
        assert_eq!(pr_decision(AutonomyLevel::AutoMerge), PrDecision::AutoMerge);
    }

    #[test]
    fn only_l1_skips_the_pr() {
        let no_pr: Vec<_> = AutonomyLevel::all()
            .into_iter()
            .filter(|l| pr_decision(*l) == PrDecision::ReportOnly)
            .collect();
        assert_eq!(no_pr, vec![AutonomyLevel::ReportOnly]);
    }

    #[test]
    fn only_l4_auto_merges() {
        let merges: Vec<_> = AutonomyLevel::all()
            .into_iter()
            .filter(|l| pr_decision(*l) == PrDecision::AutoMerge)
            .collect();
        assert_eq!(merges, vec![AutonomyLevel::AutoMerge]);
    }

    #[test]
    fn only_l2_opens_a_draft() {
        let drafts: Vec<_> = AutonomyLevel::all()
            .into_iter()
            .filter(|l| pr_decision(*l) == PrDecision::Draft)
            .collect();
        assert_eq!(drafts, vec![AutonomyLevel::DraftPr]);
    }

    #[test]
    fn l3_and_l4_force_the_verifier_even_when_disabled() {
        assert!(requires_verifier(false, AutonomyLevel::VerifiedPr));
        assert!(requires_verifier(false, AutonomyLevel::AutoMerge));
    }

    #[test]
    fn l1_and_l2_only_verify_when_explicitly_enabled() {
        assert!(!requires_verifier(false, AutonomyLevel::ReportOnly));
        assert!(!requires_verifier(false, AutonomyLevel::DraftPr));
        assert!(requires_verifier(true, AutonomyLevel::ReportOnly));
        assert!(requires_verifier(true, AutonomyLevel::DraftPr));
    }

    #[test]
    fn auto_merge_only_when_l4_and_pr_opened() {
        // L4 + PR opened → merge.
        assert!(should_auto_merge(PrDecision::AutoMerge, true));
        // L4 but the PR failed to open → never merge a branch with no PR.
        assert!(!should_auto_merge(PrDecision::AutoMerge, false));
        // Lower levels never auto-merge, even with a PR open.
        for d in [
            PrDecision::ReportOnly,
            PrDecision::Draft,
            PrDecision::Normal,
        ] {
            assert!(!should_auto_merge(d, true));
        }
    }
}
