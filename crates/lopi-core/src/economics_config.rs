//! Sprint E — the `[economics]` table in `lopi.toml`. Split out of
//! `economics.rs` to keep that file under the 500-line CI file-size gate
//! once [`LadderThresholds`]/[`EconomicsConfig`] landed alongside
//! [`crate::Money`]/[`crate::Pool`]/[`crate::BudgetTier`] — same pattern as
//! `event.rs`'s split into `event_tests.rs`/`event_wire_format_tests.rs`.

use crate::economics::{BudgetTier, Money, Pool};
use serde::{Deserialize, Serialize};

/// Fraction-of-ceiling thresholds that drive [`BudgetTier`] transitions.
/// Expressed as "headroom remaining ≤ this fraction of the pool's
/// `ceiling()`" — config-tunable per the brief's "a ladder, with each rung
/// a config-tunable threshold on remaining pool." Each threshold must be
/// strictly tighter than the one above it; [`Self::validate`] catches a
/// misconfigured ladder at load time instead of producing a ladder that
/// can never reach its lower rungs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LadderThresholds {
    /// Enter `Conserve` once remaining headroom drops to this fraction of
    /// the pool ceiling or below.
    #[serde(default = "default_conserve_below")]
    pub conserve_below: f64,
    /// Enter `Essential` once remaining headroom drops to this fraction.
    #[serde(default = "default_essential_below")]
    pub essential_below: f64,
    /// Enter `Drain` once remaining headroom drops to this fraction.
    #[serde(default = "default_drain_below")]
    pub drain_below: f64,
    /// Enter `Halt` once remaining headroom drops to this fraction.
    #[serde(default = "default_halt_below")]
    pub halt_below: f64,
}

fn default_conserve_below() -> f64 {
    0.5
}
fn default_essential_below() -> f64 {
    0.2
}
fn default_drain_below() -> f64 {
    0.1
}
fn default_halt_below() -> f64 {
    0.02
}

impl Default for LadderThresholds {
    fn default() -> Self {
        Self {
            conserve_below: default_conserve_below(),
            essential_below: default_essential_below(),
            drain_below: default_drain_below(),
            halt_below: default_halt_below(),
        }
    }
}

impl LadderThresholds {
    /// Classify a headroom ratio (`remaining / ceiling`, `[0.0, 1.0+]`)
    /// into the tier it lands in. Checked most-severe-first so a ratio at
    /// or below every threshold lands on `Halt`, not the first match.
    #[must_use]
    pub fn tier_for_ratio(&self, ratio: f64) -> BudgetTier {
        if ratio <= self.halt_below {
            BudgetTier::Halt
        } else if ratio <= self.drain_below {
            BudgetTier::Drain
        } else if ratio <= self.essential_below {
            BudgetTier::Essential
        } else if ratio <= self.conserve_below {
            BudgetTier::Conserve
        } else {
            BudgetTier::Full
        }
    }

    /// `Err` describing the first ordering violation, or `Ok(())` when
    /// every threshold is strictly tighter than the one above it — a
    /// ladder that isn't monotonically decreasing could skip rungs or get
    /// permanently stuck on one.
    ///
    /// # Errors
    /// Returns a human-readable description of which pair of thresholds is
    /// out of order.
    pub fn validate(&self) -> Result<(), String> {
        let pairs = [
            (
                "conserve_below",
                self.conserve_below,
                "essential_below",
                self.essential_below,
            ),
            (
                "essential_below",
                self.essential_below,
                "drain_below",
                self.drain_below,
            ),
            (
                "drain_below",
                self.drain_below,
                "halt_below",
                self.halt_below,
            ),
        ];
        for (a_name, a, b_name, b) in pairs {
            if a <= b {
                return Err(format!(
                    "{a_name} ({a}) must be strictly greater than {b_name} ({b})"
                ));
            }
        }
        Ok(())
    }
}

/// The `[economics]` table in `lopi.toml` — Sprint E's config surface.
/// `pool: None` (the default) means the economics layer is entirely
/// inactive: no reservation, no ladder, no runaway detection. This is a
/// deliberate opt-in — existing installs with no `[economics]` table keep
/// today's behavior exactly, and turning it on requires the operator to
/// actually say what pool they're spending against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EconomicsConfig {
    /// The single active spending resource. `None` disables the layer.
    #[serde(default)]
    pub pool: Option<Pool>,
    /// Fraction-of-ceiling thresholds driving the degradation ladder.
    #[serde(default)]
    pub ladder: LadderThresholds,
    /// Absolute cost cap per session, unconditional — runaway detector #3.
    #[serde(default = "default_hard_session_ceiling")]
    pub hard_session_ceiling: Money,
    /// How long a reservation may sit un-reconciled before it expires and
    /// its hold is released automatically (crash/cancellation recovery).
    #[serde(default = "default_reservation_ttl_secs")]
    pub reservation_ttl_secs: u64,
    /// Rolling window over which burn rate (tokens/minute) is measured —
    /// runaway detector #1.
    #[serde(default = "default_burn_rate_window_secs")]
    pub burn_rate_window_secs: u64,
    /// A session is "looping" once its spend since the last gate pass
    /// exceeds this multiple of the stage's p90 — runaway detector #2.
    #[serde(default = "default_cost_per_progress_multiplier")]
    pub cost_per_progress_multiplier: f64,
    /// Minimum historical sample size before [`crate::Money`] estimates are
    /// treated as confident rather than cold-start-widened.
    #[serde(default = "default_cold_start_sample_min")]
    pub cold_start_sample_min: usize,
}

impl Default for EconomicsConfig {
    // Hand-written (not `#[derive(Default)]`) so `EconomicsConfig::default()`
    // matches `toml::from_str("")` byte-for-byte — a derived `Default` would
    // give every field its type's zero value instead of the `#[serde(default
    // = "...")]` functions below, silently diverging from what an absent
    // `[economics]` table actually deserializes to.
    fn default() -> Self {
        Self {
            pool: None,
            ladder: LadderThresholds::default(),
            hard_session_ceiling: default_hard_session_ceiling(),
            reservation_ttl_secs: default_reservation_ttl_secs(),
            burn_rate_window_secs: default_burn_rate_window_secs(),
            cost_per_progress_multiplier: default_cost_per_progress_multiplier(),
            cold_start_sample_min: default_cold_start_sample_min(),
        }
    }
}

fn default_hard_session_ceiling() -> Money {
    Money::from_usd(10.0)
}
fn default_reservation_ttl_secs() -> u64 {
    1800
}
fn default_burn_rate_window_secs() -> u64 {
    300
}
fn default_cost_per_progress_multiplier() -> f64 {
    3.0
}
fn default_cold_start_sample_min() -> usize {
    5
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn ladder_default_thresholds_are_valid() {
        assert!(LadderThresholds::default().validate().is_ok());
    }

    #[test]
    fn ladder_rejects_out_of_order_thresholds() {
        let bad = LadderThresholds {
            conserve_below: 0.1,
            essential_below: 0.2,
            drain_below: 0.05,
            halt_below: 0.01,
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn ladder_tier_for_ratio_covers_every_rung() {
        let t = LadderThresholds::default();
        assert_eq!(t.tier_for_ratio(1.0), BudgetTier::Full);
        assert_eq!(t.tier_for_ratio(0.5), BudgetTier::Conserve);
        assert_eq!(t.tier_for_ratio(0.2), BudgetTier::Essential);
        assert_eq!(t.tier_for_ratio(0.1), BudgetTier::Drain);
        assert_eq!(t.tier_for_ratio(0.02), BudgetTier::Halt);
        assert_eq!(t.tier_for_ratio(0.0), BudgetTier::Halt);
    }

    #[test]
    fn economics_config_disabled_by_default() {
        let cfg = EconomicsConfig::default();
        assert!(cfg.pool.is_none(), "economics layer must default to off");
    }

    #[test]
    fn economics_config_round_trips_through_toml() {
        let mut cfg = EconomicsConfig::default();
        cfg.pool = Some(Pool::AgentSdkCredits {
            monthly_allotment: Money::from_usd(100.0),
            resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date"),
        });
        let text = toml::to_string(&cfg).expect("serialize");
        let back: EconomicsConfig = toml::from_str(&text).expect("deserialize");
        assert_eq!(cfg, back);
    }

    #[test]
    fn economics_config_missing_table_uses_defaults() {
        let back: EconomicsConfig = toml::from_str("").expect("empty table uses defaults");
        assert_eq!(back, EconomicsConfig::default());
    }
}
