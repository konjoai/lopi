//! Loop-engineering finalisation: autonomy-aware handling of a passing score.
//!
//! When an attempt's heuristic score passes, the action taken depends on the
//! task's [`AutonomyLevel`] — the L1–L4 trust ladder from loop-engineering
//! practice (trust is earned incrementally, never assumed):
//!
//! - **L1 `ReportOnly`** — commit the work to the attempt branch as an
//!   inspectable artifact; never push or open a PR.
//! - **L2 `DraftPr`** — open a *draft* PR for a human to review and mark ready.
//! - **L3 `VerifiedPr`** — run the maker/checker verifier first, then open a PR.
//! - **L4 `AutoMerge`** — verify, open a PR, and enable GitHub native
//!   auto-merge so the change lands once required checks pass.
//!
//! L3+ force the verifier regardless of the `with_verifier` builder flag, so
//! raising a schedule's trust level also raises its proof obligation.

use super::AgentRunner;
use lopi_context::Phase;
use lopi_core::{AutonomyLevel, Score, TaskStatus};
use lopi_git::GitManager;

/// Outcome of [`AgentRunner::finalize_success`].
pub(super) enum Finalize {
    /// The loop reached a terminal state — return this status to the caller.
    Done(TaskStatus),
    /// The verifier rejected the output — the caller should roll back and retry.
    Rejected,
}

/// The PR action implied by an autonomy level. Kept as a pure value so it can be
/// unit-tested without touching git or the network.
#[derive(Debug, PartialEq, Eq)]
struct PrPlan {
    /// Whether to push the branch and open a pull request at all (L2+).
    open: bool,
    /// Whether the PR is opened as a draft (L2 only).
    draft: bool,
    /// Whether to enable GitHub auto-merge after opening (L4 only).
    auto_merge: bool,
}

/// Map an autonomy level to its concrete PR action.
fn pr_plan(level: AutonomyLevel) -> PrPlan {
    PrPlan {
        open: level.opens_pr(),
        draft: matches!(level, AutonomyLevel::DraftPr),
        auto_merge: level.allows_auto_merge(),
    }
}

/// Update the consecutive no-progress streak given this attempt's weighted
/// score.
///
/// The streak increments when the score fails to improve on the best seen so
/// far (within `EPSILON`) and resets to zero on any real improvement. The first
/// observation seeds the baseline and counts as zero. Returns the new streak.
///
/// This is the semantic stall detector behind `LoopConfig::no_progress_limit`:
/// a loop that keeps retrying without lifting its score is stuck, and burning
/// the rest of the retry budget on it just wastes tokens.
pub(super) fn update_no_progress_streak(best: &mut Option<f32>, streak: u8, weighted: f32) -> u8 {
    const EPSILON: f32 = 1e-4;
    match *best {
        Some(prev) if weighted > prev + EPSILON => {
            *best = Some(weighted);
            0
        }
        Some(_) => streak.saturating_add(1),
        None => {
            *best = Some(weighted);
            0
        }
    }
}

impl AgentRunner {
    /// Handle a passing score under the task's autonomy level.
    ///
    /// Returns [`Finalize::Done`] with the terminal success status, or
    /// [`Finalize::Rejected`] when the verifier vetoed the output.
    pub(super) async fn finalize_success(
        &mut self,
        git: &GitManager,
        branch: &str,
        score: &Score,
        attempt: u8,
    ) -> Finalize {
        let level = self.task.autonomy_level;
        // L3+ require the maker/checker verifier to pass before any PR. The
        // builder flag (`with_verifier`) can also enable it at lower levels.
        if (self.verifier_enabled || level.requires_verifier())
            && !self.run_verifier_pass(attempt, &score.errors).await
        {
            return Finalize::Rejected;
        }

        self.pin_success(score);
        git.commit_all(&format!("lopi: {}", self.task.goal))
            .await
            .ok();

        let pr_url = self.open_for_level(git, branch, level).await;
        Finalize::Done(TaskStatus::Success {
            branch: branch.to_string(),
            pr_url,
        })
    }

    /// Pin the success conclusion into the context window so it survives evictions.
    fn pin_success(&mut self, score: &Score) {
        self.context.pin_conclusion(
            format!(
                "Sprint succeeded — pass={:.0}% diff={}L",
                score.test_pass_rate * 100.0,
                score.diff_lines
            ),
            Phase::Conclusion,
        );
        self.log("✅ tests pass — committing…");
    }

    /// Open (or intentionally skip) a PR according to the autonomy level,
    /// returning the PR URL when one was opened.
    async fn open_for_level(
        &self,
        git: &GitManager,
        branch: &str,
        level: AutonomyLevel,
    ) -> Option<String> {
        let plan = pr_plan(level);
        if !plan.open {
            self.log("📄 L1 report-only — change committed to branch, no PR opened");
            return None;
        }
        let opened = if plan.draft {
            git.open_pr_draft(branch, &self.task.goal).await
        } else {
            git.open_pr(branch, &self.task.goal).await
        };
        let url = match opened {
            Ok(u) => u,
            Err(e) => {
                self.warn(format!("PR open failed: {e}"));
                return None;
            }
        };
        self.log(format!(
            "🔗 {} PR opened: {url}",
            if plan.draft { "draft" } else { "verified" }
        ));
        if plan.auto_merge {
            match git.enable_auto_merge(branch).await {
                Ok(()) => self.log("🤝 L4 auto-merge enabled — lands when checks pass"),
                Err(e) => self.warn(format!("auto-merge enable failed: {e}")),
            }
        }
        Some(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_plan_l1_report_only_opens_nothing() {
        let p = pr_plan(AutonomyLevel::ReportOnly);
        assert_eq!(
            p,
            PrPlan {
                open: false,
                draft: false,
                auto_merge: false
            }
        );
    }

    #[test]
    fn pr_plan_l2_is_a_draft() {
        let p = pr_plan(AutonomyLevel::DraftPr);
        assert!(p.open && p.draft && !p.auto_merge);
    }

    #[test]
    fn pr_plan_l3_is_a_ready_pr() {
        let p = pr_plan(AutonomyLevel::VerifiedPr);
        assert!(p.open && !p.draft && !p.auto_merge);
    }

    #[test]
    fn pr_plan_l4_enables_auto_merge() {
        let p = pr_plan(AutonomyLevel::AutoMerge);
        assert!(p.open && !p.draft && p.auto_merge);
    }

    #[test]
    fn no_progress_seeds_baseline_then_counts_stalls() {
        let mut best = None;
        // First observation seeds the baseline — not a stall.
        assert_eq!(update_no_progress_streak(&mut best, 0, 0.5), 0);
        assert_eq!(best, Some(0.5));
        // No improvement → streak climbs.
        assert_eq!(update_no_progress_streak(&mut best, 0, 0.5), 1);
        assert_eq!(update_no_progress_streak(&mut best, 1, 0.5), 2);
    }

    #[test]
    fn no_progress_resets_on_improvement() {
        let mut best = Some(0.4);
        assert_eq!(update_no_progress_streak(&mut best, 3, 0.9), 0);
        assert_eq!(best, Some(0.9));
        // A lower score after the improvement still counts as no progress.
        assert_eq!(update_no_progress_streak(&mut best, 0, 0.6), 1);
    }

    #[test]
    fn no_progress_ignores_sub_epsilon_noise() {
        let mut best = Some(0.5);
        // A negligible bump is not real progress.
        assert_eq!(update_no_progress_streak(&mut best, 0, 0.500_01), 1);
    }
}
