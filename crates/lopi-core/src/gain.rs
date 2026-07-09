//! Progress-Gating (A3) — the **gain gate** (cross-cutting seam: A3 reads A1's
//! [`EvalOutcome`](crate::EvalOutcome) score, decides accept vs reject).
//!
//! The gain gate keeps an iteration only when it is a genuine *gain* over the
//! best result seen so far. Its single hardest job — the A3 analog of A1's
//! fail-open hole — is to **not lock noise**: a run that edges above "best" on a
//! noisy signal is not a gain, and ratcheting the loop forward on it is exactly
//! the rigor failure lopi exists to avoid.
//!
//! The rule is **objective-primary**: the decision is driven by the objective,
//! deterministic sub-score (execution-ok / shell-test / suite tiers), and the
//! judge's score is treated as a **weaker, confirmatory signal** — it can veto a
//! gain the judge flatly contradicts, but it can never *manufacture* one. A
//! judge-only "improvement" within judge-score noise must not lock.
//!
//! See `docs/lopi-loop-intelligence-roadmap.md` (A3) for the design rationale.

use crate::acceptance::EvalTier;
use crate::eval_outcome::EvalOutcome;
use serde::{Deserialize, Serialize};

/// The two comparable magnitudes the gain gate reads off one evaluation: the
/// **objective** (deterministic, un-gameable) sub-score and the **judge**
/// (separate-model, noisy) sub-score. Either is `None` when no check of that
/// kind ran, so the rule can tell "no objective signal" apart from "objective
/// scored `0.0`" — the distinction that stops judge-only noise from locking.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GainSample {
    /// Weighted mean of the deterministic tiers' scores in `[0, 1]`, or `None`
    /// when the evaluation ran no objective (non-judge) check.
    pub objective: Option<f32>,
    /// Weighted mean of the judge tier's scores in `[0, 1]`, or `None` when no
    /// judge check ran. Confirmatory only — never the primary gain signal.
    pub judge: Option<f32>,
}

impl GainSample {
    /// Split an [`EvalOutcome`] into its objective and judge sub-scores.
    ///
    /// Each sub-score is the weight-weighted mean of its tier group's per-check
    /// scores; a group with zero total weight (or no members) yields `None`.
    /// The [`EvalTier::Judge`] tier is the judge group; every other tier
    /// ([`ExecutionOk`](EvalTier::ExecutionOk), [`ShellTest`](EvalTier::ShellTest),
    /// [`Suite`](EvalTier::Suite)) is objective.
    #[must_use]
    pub fn from_outcome(outcome: &EvalOutcome) -> Self {
        let (mut obj_w, mut obj_s) = (0.0_f32, 0.0_f32);
        let (mut jdg_w, mut jdg_s) = (0.0_f32, 0.0_f32);
        for c in &outcome.per_check {
            if c.tier == EvalTier::Judge {
                jdg_w += c.weight;
                jdg_s += c.weight * c.score;
            } else {
                obj_w += c.weight;
                obj_s += c.weight * c.score;
            }
        }
        Self {
            objective: (obj_w > 0.0).then(|| (obj_s / obj_w).clamp(0.0, 1.0)),
            judge: (jdg_w > 0.0).then(|| (jdg_s / jdg_w).clamp(0.0, 1.0)),
        }
    }

    /// A sample carrying only an objective magnitude — the shape the live loop
    /// builds from the heuristic `Score::weighted` (test/lint/diff are all
    /// deterministic metrics, so there is no judge component).
    #[must_use]
    pub const fn objective_only(objective: f32) -> Self {
        Self {
            objective: Some(objective),
            judge: None,
        }
    }
}

/// The gain rule's noise policy — the margins a candidate must clear to count as
/// a real gain, and the band beyond which the judge may veto one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GainRule {
    /// Minimum objective-score improvement over best that counts as a gain.
    /// A candidate whose objective delta is `<= margin` is within noise, not a
    /// gain; `< -margin` is a regression. Default `0.01` (1 %).
    pub margin: f32,
    /// The wider margin applied when the *only* signal is the judge (no
    /// objective check ran). Judge scores are noisier, so a judge-only climb
    /// must clear a larger band before it locks. Default `0.10` (10 %).
    pub judge_margin: f32,
    /// A judge-score regression larger than this vetoes an otherwise-qualifying
    /// objective gain (the judge "flatly contradicts" the improvement). Kept
    /// wide so ordinary judge noise never blocks a real objective gain.
    /// Default `0.20` (20 %).
    pub judge_veto_band: f32,
}

impl Default for GainRule {
    fn default() -> Self {
        Self {
            margin: 0.01,
            judge_margin: 0.10,
            judge_veto_band: 0.20,
        }
    }
}

/// The gain gate's verdict on one candidate versus the best so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GainDecision {
    /// A genuine gain — accept the candidate and lock it as the new best.
    Gain,
    /// The candidate sits within the noise band around best — not a gain.
    WithinNoise,
    /// The candidate regressed below best beyond the margin — reject it.
    Regression,
    /// The objective cleared the margin but the judge flatly contradicted the
    /// improvement — treated as unconfirmed, so it does not lock.
    JudgeUnconfirmed,
}

impl GainDecision {
    /// Whether this decision locks the candidate as the new best. Only
    /// [`Gain`](Self::Gain) does.
    #[must_use]
    pub const fn is_gain(self) -> bool {
        matches!(self, Self::Gain)
    }

    /// Stable wire/log string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gain => "gain",
            Self::WithinNoise => "within_noise",
            Self::Regression => "regression",
            Self::JudgeUnconfirmed => "judge_unconfirmed",
        }
    }
}

impl GainRule {
    /// Decide whether `candidate` is a gain over `best`.
    ///
    /// The first observation (`best == None`) always locks the baseline and
    /// returns [`GainDecision::Gain`]. Otherwise the rule is objective-primary:
    /// when both samples carry an objective magnitude the objective delta
    /// decides, and the judge only confirms (a large judge regression downgrades
    /// a qualifying gain to [`JudgeUnconfirmed`](GainDecision::JudgeUnconfirmed)).
    /// When neither carries an objective magnitude the judge decides behind the
    /// wider [`judge_margin`](Self::judge_margin). A candidate with no comparable
    /// signal at all is [`WithinNoise`](GainDecision::WithinNoise) — the gate
    /// never manufactures progress it cannot measure.
    #[must_use]
    pub fn decide(&self, candidate: &GainSample, best: Option<&GainSample>) -> GainDecision {
        let Some(best) = best else {
            return GainDecision::Gain;
        };
        match (candidate.objective, best.objective) {
            (Some(cand), Some(prev)) => self.decide_objective(cand - prev, candidate, best),
            _ => self.decide_judge_only(candidate, best),
        }
    }

    /// Objective-primary branch: classify the objective delta, then let the
    /// judge veto (but never create) a gain.
    fn decide_objective(
        &self,
        delta: f32,
        candidate: &GainSample,
        best: &GainSample,
    ) -> GainDecision {
        if delta < -self.margin {
            return GainDecision::Regression;
        }
        if delta <= self.margin {
            return GainDecision::WithinNoise;
        }
        if self.judge_vetoes(candidate, best) {
            return GainDecision::JudgeUnconfirmed;
        }
        GainDecision::Gain
    }

    /// Whether the judge score dropped far enough to contradict an objective
    /// gain. Requires both samples to carry a judge score; a missing judge is
    /// neutral (no veto).
    fn judge_vetoes(&self, candidate: &GainSample, best: &GainSample) -> bool {
        match (candidate.judge, best.judge) {
            (Some(cand), Some(prev)) => (cand - prev) < -self.judge_veto_band,
            _ => false,
        }
    }

    /// Judge-only branch: no objective signal exists, so the judge decides —
    /// behind the wider [`judge_margin`](Self::judge_margin). With no comparable
    /// judge score either, nothing can be a gain.
    fn decide_judge_only(&self, candidate: &GainSample, best: &GainSample) -> GainDecision {
        let (Some(cand), Some(prev)) = (candidate.judge, best.judge) else {
            return GainDecision::WithinNoise;
        };
        let delta = cand - prev;
        if delta < -self.judge_margin {
            GainDecision::Regression
        } else if delta <= self.judge_margin {
            GainDecision::WithinNoise
        } else {
            GainDecision::Gain
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::acceptance::EvalTier;
    use crate::eval_outcome::{CheckResult, EvalOutcome};

    /// Run a sequence of objective-only samples through the gain gate, locking
    /// best on each gain. Returns the per-step decisions — the shape the §2
    /// kill-test asserts against.
    fn run_objective_sequence(rule: &GainRule, scores: &[f32]) -> Vec<GainDecision> {
        let mut best: Option<GainSample> = None;
        let mut out = Vec::new();
        for &s in scores {
            let cand = GainSample::objective_only(s);
            let decision = rule.decide(&cand, best.as_ref());
            if decision.is_gain() {
                best = Some(cand);
            }
            out.push(decision);
        }
        out
    }

    // ── §2 CENTERPIECE KILL-TEST — the gain gate must not lock noise ─────────
    //
    // Pre-registered fixtures: a genuine monotonic climb, a within-noise wiggle
    // around a plateau, a real regression, and a judge-noisy sequence. Genuine
    // gains must lock; wiggles, regressions, and judge-only noise must not.

    #[test]
    fn kill_test_monotonic_climb_locks_every_step() {
        let rule = GainRule::default();
        // Each step clears the 1 % margin comfortably — all real gains.
        let decisions = run_objective_sequence(&rule, &[0.50, 0.60, 0.72, 0.85, 0.95]);
        assert!(
            decisions.iter().all(|d| d.is_gain()),
            "a genuine monotonic climb must lock every step, got {decisions:?}"
        );
    }

    #[test]
    fn kill_test_within_noise_wiggle_never_locks() {
        let rule = GainRule::default();
        // A plateau at ~0.80 with sub-margin jitter (±0.5 %). The baseline locks
        // once; nothing after it may lock.
        let decisions = run_objective_sequence(&rule, &[0.800, 0.805, 0.798, 0.803, 0.799, 0.802]);
        assert_eq!(decisions[0], GainDecision::Gain, "baseline seeds best");
        assert!(
            decisions[1..].iter().all(|d| !d.is_gain()),
            "within-noise wiggle must not ratchet, got {decisions:?}"
        );
    }

    #[test]
    fn kill_test_real_regression_is_rejected() {
        let rule = GainRule::default();
        let decisions = run_objective_sequence(&rule, &[0.90, 0.60]);
        assert_eq!(decisions[0], GainDecision::Gain);
        assert_eq!(
            decisions[1],
            GainDecision::Regression,
            "a real drop below best must be a regression, not a gain"
        );
    }

    #[test]
    fn kill_test_judge_noise_cannot_lock_when_objective_is_flat() {
        let rule = GainRule::default();
        // Objective pinned at 0.80; judge wiggles wildly. The judge must never
        // manufacture a gain the objective didn't earn.
        let mut best: Option<GainSample> = None;
        let judge_seq = [0.40_f32, 0.95, 0.30, 0.99, 0.20];
        let mut decisions = Vec::new();
        for &j in &judge_seq {
            let cand = GainSample {
                objective: Some(0.80),
                judge: Some(j),
            };
            let d = rule.decide(&cand, best.as_ref());
            if d.is_gain() {
                best = Some(cand);
            }
            decisions.push(d);
        }
        assert_eq!(decisions[0], GainDecision::Gain, "baseline seeds best");
        assert!(
            decisions[1..].iter().all(|d| !d.is_gain()),
            "judge noise on a flat objective must not lock, got {decisions:?}"
        );
    }

    #[test]
    fn judge_only_climb_must_clear_the_wider_band() {
        let rule = GainRule::default();
        let sample = |j: f32| GainSample {
            objective: None,
            judge: Some(j),
        };
        // Baseline locks; a +0.05 judge step is within the 10 % judge band, so
        // it is noise; a +0.20 step clears it and locks.
        let mut best = Some(sample(0.50));
        assert_eq!(
            rule.decide(&sample(0.55), best.as_ref()),
            GainDecision::WithinNoise,
            "a sub-judge-margin climb is noise, not a gain"
        );
        best = Some(sample(0.50));
        assert_eq!(
            rule.decide(&sample(0.75), best.as_ref()),
            GainDecision::Gain,
            "a judge climb past the wider band is a real gain"
        );
    }

    #[test]
    fn objective_gain_with_large_judge_contradiction_is_unconfirmed() {
        let rule = GainRule::default();
        let best = GainSample {
            objective: Some(0.60),
            judge: Some(0.90),
        };
        let candidate = GainSample {
            objective: Some(0.80), // clears the objective margin
            judge: Some(0.50),     // but the judge cratered by 0.40 > 0.20 band
        };
        assert_eq!(
            rule.decide(&candidate, Some(&best)),
            GainDecision::JudgeUnconfirmed,
            "an objective gain the judge flatly contradicts must not lock"
        );
    }

    #[test]
    fn small_judge_dip_does_not_veto_a_real_objective_gain() {
        let rule = GainRule::default();
        let best = GainSample {
            objective: Some(0.60),
            judge: Some(0.90),
        };
        let candidate = GainSample {
            objective: Some(0.80),
            judge: Some(0.82), // within the veto band — ordinary judge noise
        };
        assert_eq!(
            rule.decide(&candidate, Some(&best)),
            GainDecision::Gain,
            "ordinary judge noise must not block a real objective gain"
        );
    }

    #[test]
    fn first_observation_always_locks_the_baseline() {
        let rule = GainRule::default();
        assert_eq!(
            rule.decide(&GainSample::objective_only(0.3), None),
            GainDecision::Gain
        );
    }

    #[test]
    fn no_comparable_signal_is_never_a_gain() {
        let rule = GainRule::default();
        let empty = GainSample {
            objective: None,
            judge: None,
        };
        assert_eq!(
            rule.decide(&empty, Some(&empty)),
            GainDecision::WithinNoise,
            "the gate must not manufacture progress it cannot measure"
        );
    }

    #[test]
    fn from_outcome_splits_objective_and_judge_by_tier() {
        let mut det = CheckResult::pass(EvalTier::ShellTest, 1.0, true);
        det.score = 0.9;
        let mut judge = CheckResult::pass(EvalTier::Judge, 1.0, true);
        judge.score = 0.4;
        let outcome = EvalOutcome::aggregate(vec![det, judge]);
        let sample = GainSample::from_outcome(&outcome);
        assert_eq!(sample.objective, Some(0.9));
        assert_eq!(sample.judge, Some(0.4));
    }

    #[test]
    fn from_outcome_judge_only_has_no_objective() {
        let mut judge = CheckResult::pass(EvalTier::Judge, 1.0, true);
        judge.score = 0.7;
        let outcome = EvalOutcome::aggregate(vec![judge]);
        let sample = GainSample::from_outcome(&outcome);
        assert_eq!(sample.objective, None);
        assert_eq!(sample.judge, Some(0.7));
    }

    #[test]
    fn from_outcome_zero_weight_check_contributes_no_score() {
        // A zero-weight objective check yields no objective magnitude.
        let outcome =
            EvalOutcome::aggregate(vec![CheckResult::pass(EvalTier::ExecutionOk, 0.0, false)]);
        let sample = GainSample::from_outcome(&outcome);
        assert_eq!(sample.objective, None);
        assert_eq!(sample.judge, None);
    }

    #[test]
    fn gain_decision_strings_are_stable() {
        assert_eq!(GainDecision::Gain.as_str(), "gain");
        assert_eq!(GainDecision::WithinNoise.as_str(), "within_noise");
        assert_eq!(GainDecision::Regression.as_str(), "regression");
        assert_eq!(GainDecision::JudgeUnconfirmed.as_str(), "judge_unconfirmed");
    }

    #[test]
    fn rule_round_trips_through_json() {
        let rule = GainRule::default();
        let json = serde_json::to_string(&rule).unwrap();
        let back: GainRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
    }
}
