//! AVO-Supervisor-2 (Feature 2) — stall/thrash detection.
//!
//! Layered *under* Progress-Gating (A3)'s existing `no_progress_limit`
//! termination guard, not a replacement for it: A3 still owns "when does
//! the loop give up and stop spending." This module only decides "is the
//! *current* non-gaining attempt worth flagging as `TaskStatus::Stuck` and
//! steering the next attempt with a summary" — an earlier, softer signal
//! than the harder budget cutoff, and one that never changes how many
//! attempts a task gets.
//!
//! Two cheap, no-ML signals, either sufficient on its own (brief: "diff-
//! similarity... plus the score-improvement signal... plateau"):
//! - **Thrash**: this attempt's diff is near-identical to the previous
//!   attempt's (line-overlap ratio on changed `+`/`-` lines).
//! - **Plateau**: the gain gate's non-gain streak has reached a small
//!   threshold — well below the harder `no_progress_limit` that actually
//!   stops the loop, so this fires as an early, informational warning.

use std::path::Path;
use tokio::process::Command;

/// Line-overlap ratio at or above which two attempts' diffs count as
/// "near-identical" (thrash) rather than merely similar.
pub(super) const THRASH_SIMILARITY_THRESHOLD: f32 = 0.8;

/// Consecutive non-gain rounds before the *informational* `Stuck` status
/// fires. Deliberately smaller than the loop's own `no_progress_limit`
/// (which still governs termination, unchanged) — this is an earlier
/// heads-up, not a second stop condition.
pub(super) const PLATEAU_STREAK_THRESHOLD: u8 = 2;

/// The stall detector's verdict for one attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StallSignal {
    pub thrash: bool,
    pub plateau: bool,
}

impl StallSignal {
    /// Whether either signal fired.
    pub(super) const fn is_stuck(self) -> bool {
        self.thrash || self.plateau
    }

    /// Short machine tag for the dashboard badge and the `stuck_reason`
    /// column. Only meaningful when [`Self::is_stuck`] is `true`.
    pub(super) const fn reason(self) -> &'static str {
        match (self.thrash, self.plateau) {
            (true, true) => "plateau+thrash",
            (true, false) => "thrash",
            (false, true) => "plateau",
            (false, false) => "",
        }
    }
}

/// Classify one attempt's non-gain streak and diff similarity into a
/// [`StallSignal`]. Pure, so the threshold logic is unit-testable without a
/// runner, git checkout, or gate.
pub(super) fn detect(non_gain_streak: u8, diff_similarity: f32) -> StallSignal {
    StallSignal {
        thrash: diff_similarity >= THRASH_SIMILARITY_THRESHOLD,
        plateau: non_gain_streak >= PLATEAU_STREAK_THRESHOLD,
    }
}

/// Jaccard similarity (`|intersection| / |union|`) over the changed lines
/// (lines starting with `+`/`-`, excluding the `+++`/`---` file headers) of
/// two unified diffs. `0.0` when either side has no changed lines — an
/// empty diff is not "identical to" anything, it's the absence of a
/// comparison. No ML, no alignment — a cheap starting signal per the
/// brief's own "line-overlap ratio is enough to start."
pub(super) fn line_overlap_ratio(a: &str, b: &str) -> f32 {
    let set_a = changed_line_set(a);
    let set_b = changed_line_set(b);
    if set_a.is_empty() || set_b.is_empty() {
        return 0.0;
    }
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    #[allow(clippy::cast_precision_loss)]
    let ratio = intersection as f32 / union as f32;
    ratio
}

/// The set of changed (`+`/`-`) content lines in a unified diff, skipping
/// the `+++`/`---` file-header lines (which carry a path, not content, and
/// would otherwise inflate the intersection between any two diffs that
/// touch the same file).
fn changed_line_set(diff: &str) -> std::collections::HashSet<&str> {
    diff.lines()
        .filter(|l| {
            (l.starts_with('+') && !l.starts_with("+++"))
                || (l.starts_with('-') && !l.starts_with("---"))
        })
        .collect()
}

/// Capture the working tree's current unified diff (tracked changes only,
/// same scope `git diff` covers everywhere else in this crate — see
/// `Scorer::score`'s own `git diff --shortstat` call). `None` on any git
/// failure or when there is nothing to diff; never fatal to the run.
pub(super) async fn capture_diff(repo_path: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("diff")
        .current_dir(repo_path)
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    (!text.trim().is_empty()).then_some(text)
}

/// Render a stall verdict as a planning-prompt steering constraint —
/// AVO's "tell the next attempt what already plateaued" behavior. Pure, so
/// the wording is unit-testable without a runner. Appended to (not a
/// replacement for) the existing adaptive-retry failure framing, so the
/// next attempt still sees *why* it failed as well as *that* it's stuck.
pub(super) fn steering_constraint(attempt: u8, signal: StallSignal, streak: u8) -> String {
    let cause = match (signal.thrash, signal.plateau) {
        (true, true) => {
            format!(
                "its diff was nearly identical to attempt {}'s AND the score has not \
                 improved for {streak} consecutive attempts",
                attempt.saturating_sub(1)
            )
        }
        (true, false) => format!(
            "its diff was nearly identical to attempt {}'s — resubmitting the same \
             change is not going to pass this time either",
            attempt.saturating_sub(1)
        ),
        (false, true) => format!("the score has not improved for {streak} consecutive attempts"),
        (false, false) => "no forward progress was detected".to_string(),
    };
    format!(
        "Attempt {attempt} is stuck: {cause}. Do not repeat the same change — try a \
         materially different approach (a different file, a different root cause, or \
         re-reading the failing test/lint output more carefully) rather than a small \
         variation on what already plateaued."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_diffs_score_similarity_one() {
        let diff = "diff --git a/x.rs b/x.rs\n+let x = 1;\n-let x = 0;\n";
        assert_eq!(line_overlap_ratio(diff, diff), 1.0);
    }

    #[test]
    fn disjoint_diffs_score_similarity_zero() {
        let a = "+let x = 1;\n";
        let b = "+let y = 2;\n";
        assert_eq!(line_overlap_ratio(a, b), 0.0);
    }

    #[test]
    fn partial_overlap_is_between_zero_and_one() {
        let a = "+let x = 1;\n+let y = 2;\n";
        let b = "+let x = 1;\n+let z = 3;\n";
        let ratio = line_overlap_ratio(a, b);
        assert!(ratio > 0.0 && ratio < 1.0, "got {ratio}");
    }

    #[test]
    fn empty_diff_is_never_similar_to_anything() {
        assert_eq!(line_overlap_ratio("", "+let x = 1;\n"), 0.0);
        assert_eq!(line_overlap_ratio("", ""), 0.0);
    }

    #[test]
    fn file_header_lines_are_excluded_from_the_comparison() {
        // Two diffs touching the same file (identical +++/--- headers) but
        // with completely different content must not read as similar just
        // because the headers matched.
        let a = "--- a/x.rs\n+++ b/x.rs\n+let x = 1;\n";
        let b = "--- a/x.rs\n+++ b/x.rs\n+let y = 2;\n";
        assert_eq!(line_overlap_ratio(a, b), 0.0);
    }

    #[test]
    fn detect_fires_thrash_at_the_threshold() {
        let signal = detect(0, THRASH_SIMILARITY_THRESHOLD);
        assert!(signal.thrash);
        assert!(!signal.plateau);
        assert!(signal.is_stuck());
        assert_eq!(signal.reason(), "thrash");
    }

    #[test]
    fn detect_fires_plateau_at_the_threshold() {
        let signal = detect(PLATEAU_STREAK_THRESHOLD, 0.0);
        assert!(!signal.thrash);
        assert!(signal.plateau);
        assert!(signal.is_stuck());
        assert_eq!(signal.reason(), "plateau");
    }

    #[test]
    fn detect_combines_both_signals() {
        let signal = detect(PLATEAU_STREAK_THRESHOLD, THRASH_SIMILARITY_THRESHOLD);
        assert_eq!(signal.reason(), "plateau+thrash");
    }

    #[test]
    fn detect_is_quiet_below_both_thresholds() {
        let signal = detect(0, 0.0);
        assert!(!signal.is_stuck());
    }

    #[test]
    fn steering_constraint_names_the_prior_attempt_on_thrash() {
        let signal = StallSignal {
            thrash: true,
            plateau: false,
        };
        let msg = steering_constraint(3, signal, 1);
        assert!(msg.contains("attempt 2"));
        assert!(msg.contains("materially different approach"));
    }

    #[test]
    fn steering_constraint_names_the_streak_on_plateau() {
        let signal = StallSignal {
            thrash: false,
            plateau: true,
        };
        let msg = steering_constraint(4, signal, 3);
        assert!(msg.contains("3 consecutive attempts"));
    }

    #[tokio::test]
    async fn capture_diff_returns_none_outside_a_git_repo() {
        let dir = std::env::temp_dir();
        // Not necessarily git-free on every CI box, but `git diff` outside
        // any repo (or in a repo with a clean tree) reliably yields either
        // an error or empty output — both map to `None` here.
        let result = capture_diff(&dir).await;
        if let Some(text) = result {
            assert!(
                !text.trim().is_empty(),
                "None on empty diff, not Some(\"\")"
            );
        }
    }
}
