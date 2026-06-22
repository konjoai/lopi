//! Phase 16.7 — earned-trust auto-promotion state machine.
//!
//! Loop-engineering practice treats autonomy as *earned incrementally, never
//! assumed* (CSA Agentic Trust Framework, 2026): a loop climbs the
//! [`AutonomyLevel`] ladder (L1 report-only → L4 auto-merge) only after it has
//! **demonstrated** reliability, and loses that standing the moment a trusted
//! change has to be reverted.
//!
//! [`EarnedTrust`] is the pure state machine behind that policy — a `level`
//! plus a `clean_streak` counter, advanced by three events:
//!
//! - [`on_clean_run`](EarnedTrust::on_clean_run) — a clean, verifier-passed run;
//!   advances the streak and promotes one rung once it reaches `promote_after`
//!   (capped at a configured `ceiling`).
//! - [`on_failed_run`](EarnedTrust::on_failed_run) — a run that didn't pass;
//!   breaks the streak but does **not** demote (a failed attempt simply doesn't
//!   *earn* promotion — it isn't punished).
//! - [`on_revert`](EarnedTrust::on_revert) — a post-merge revert of a change the
//!   loop was trusted to make; demotes one rung toward a `floor` and resets the
//!   streak. This is the decisive "trust was misplaced" signal.
//!
//! All transitions are total and saturating, so the state can never escape the
//! `floor..=ceiling` band. Persistence lives in `lopi-memory`'s `trust_ledger`;
//! this type holds no I/O.

use crate::loop_config::AutonomyLevel;
use serde::{Deserialize, Serialize};

/// The earned-trust state for one scope (a repo or a schedule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EarnedTrust {
    /// The currently earned autonomy level.
    pub level: AutonomyLevel,
    /// Consecutive clean, verifier-passed runs since the last promotion or reset.
    pub clean_streak: u32,
}

impl EarnedTrust {
    /// A fresh scope pinned at `base` with no streak yet.
    #[must_use]
    pub fn new(base: AutonomyLevel) -> Self {
        Self {
            level: base,
            clean_streak: 0,
        }
    }

    /// Record a clean, verifier-passed run.
    ///
    /// Advances the streak; once it reaches `promote_after` (and `promote_after`
    /// is non-zero) the level climbs one rung — never past `ceiling` — and the
    /// streak resets. With `promote_after == 0`, auto-promotion is disabled and
    /// only the streak advances. A level already at (or above) `ceiling` holds.
    #[must_use]
    pub fn on_clean_run(self, promote_after: u32, ceiling: AutonomyLevel) -> Self {
        let streak = self.clean_streak.saturating_add(1);
        let can_promote =
            promote_after != 0 && streak >= promote_after && self.level.rank() < ceiling.rank();
        if can_promote {
            Self {
                level: self.level.promoted(),
                clean_streak: 0,
            }
        } else {
            Self {
                level: self.level,
                clean_streak: streak,
            }
        }
    }

    /// Record a run that did not pass (verifier rejection, failed tests).
    ///
    /// Breaks the clean streak but leaves the earned level untouched — a failed
    /// attempt simply doesn't *earn* a promotion; it is not a demotion trigger.
    #[must_use]
    pub fn on_failed_run(self) -> Self {
        Self {
            level: self.level,
            clean_streak: 0,
        }
    }

    /// Record a post-merge revert of a change the loop was trusted to make.
    ///
    /// Demotes one rung toward `floor` (never below it) and resets the streak —
    /// the decisive signal that the current level over-trusted the loop.
    #[must_use]
    pub fn on_revert(self, floor: AutonomyLevel) -> Self {
        let demoted = self.level.demoted();
        let level = if demoted.rank() < floor.rank() {
            floor
        } else {
            demoted
        };
        Self {
            level,
            clean_streak: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pins_base_with_zero_streak() {
        let t = EarnedTrust::new(AutonomyLevel::DraftPr);
        assert_eq!(t.level, AutonomyLevel::DraftPr);
        assert_eq!(t.clean_streak, 0);
    }

    #[test]
    fn clean_run_advances_streak_without_promoting_early() {
        let t = EarnedTrust::new(AutonomyLevel::DraftPr);
        let t = t.on_clean_run(3, AutonomyLevel::VerifiedPr);
        assert_eq!(t.level, AutonomyLevel::DraftPr);
        assert_eq!(t.clean_streak, 1);
        let t = t.on_clean_run(3, AutonomyLevel::VerifiedPr);
        assert_eq!(t.clean_streak, 2);
        assert_eq!(t.level, AutonomyLevel::DraftPr);
    }

    #[test]
    fn promotes_one_rung_and_resets_streak_at_threshold() {
        let mut t = EarnedTrust::new(AutonomyLevel::DraftPr);
        for _ in 0..3 {
            t = t.on_clean_run(3, AutonomyLevel::VerifiedPr);
        }
        assert_eq!(t.level, AutonomyLevel::VerifiedPr);
        assert_eq!(t.clean_streak, 0, "streak resets after a promotion");
    }

    #[test]
    fn promotion_is_capped_at_ceiling() {
        // Ceiling = VerifiedPr (L3): a long streak never reaches AutoMerge (L4).
        let mut t = EarnedTrust::new(AutonomyLevel::DraftPr);
        for _ in 0..12 {
            t = t.on_clean_run(2, AutonomyLevel::VerifiedPr);
        }
        assert_eq!(t.level, AutonomyLevel::VerifiedPr);
    }

    #[test]
    fn promote_after_zero_disables_promotion() {
        let mut t = EarnedTrust::new(AutonomyLevel::DraftPr);
        for _ in 0..10 {
            t = t.on_clean_run(0, AutonomyLevel::AutoMerge);
        }
        assert_eq!(t.level, AutonomyLevel::DraftPr, "0 disables auto-promotion");
        assert_eq!(t.clean_streak, 10);
    }

    #[test]
    fn failed_run_resets_streak_but_holds_level() {
        let t = EarnedTrust {
            level: AutonomyLevel::VerifiedPr,
            clean_streak: 2,
        };
        let t = t.on_failed_run();
        assert_eq!(
            t.level,
            AutonomyLevel::VerifiedPr,
            "failure does not demote"
        );
        assert_eq!(t.clean_streak, 0);
    }

    #[test]
    fn revert_demotes_one_rung_and_resets_streak() {
        let t = EarnedTrust {
            level: AutonomyLevel::VerifiedPr,
            clean_streak: 5,
        };
        let t = t.on_revert(AutonomyLevel::DraftPr);
        assert_eq!(t.level, AutonomyLevel::DraftPr);
        assert_eq!(t.clean_streak, 0);
    }

    #[test]
    fn revert_never_drops_below_floor() {
        let t = EarnedTrust {
            level: AutonomyLevel::DraftPr,
            clean_streak: 0,
        };
        // Already at the floor — a revert holds, it does not underflow to L1.
        let t = t.on_revert(AutonomyLevel::DraftPr);
        assert_eq!(t.level, AutonomyLevel::DraftPr);
    }
}
