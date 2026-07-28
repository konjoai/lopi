//! Sprint E — the economics layer's pure data types.
//!
//! [`Money`] is the one minor-unit integer type every cost surface in the
//! `budget` module (`lopi-orchestrator::budget`) is built on — never an
//! `f64`, so accumulated reservations, spend, and thresholds never drift
//! from floating-point rounding. [`Pool`] models the distinct credit
//! resources lopi can be spending against; each has different exhaustion
//! semantics (a hard wall, an unbounded key that needs a ceiling, or a
//! depleting bundle), which is exactly why the old single-`BudgetLimit`
//! model (`lopi_ratelimit::budget`) could not degrade gracefully — it never
//! knew which kind of "out" it was looking at. [`BudgetTier`] is the
//! degradation ladder's five rungs, ordered least-to-most severe so `Ord`
//! gives a real "worse than" comparison for free.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// An integer minor-unit money type: **micro-USD** (one unit = one
/// millionth of a dollar; `1_000_000` units = `$1.00`). Micro-dollar
/// granularity (not cents) is deliberate — a single LLM turn routinely
/// costs a fraction of a cent, and rounding those to whole cents at every
/// accumulation step would make thousands of small reservations drift from
/// the real ledger. Every arithmetic op here is plain integer add/sub —
/// no floating point ever re-enters the accounting path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Money(i64);

impl Money {
    /// The zero amount.
    pub const ZERO: Self = Self(0);

    /// One micro-USD unit — `1_000_000` of these make one dollar.
    const MICROS_PER_USD: f64 = 1_000_000.0;

    /// Build from a raw micro-USD count. Prefer [`Self::from_usd`] at
    /// display/config boundaries — this is for code that already computed
    /// in micro-dollars (e.g. a rate-table multiplication).
    #[must_use]
    pub const fn from_micros(micros: i64) -> Self {
        Self(micros)
    }

    /// Build from a floating-point USD amount (e.g. `4.20`). This is the
    /// one place an `f64` is allowed to touch `Money` — config files and
    /// the Anthropic CLI's own `total_cost_usd` are both `f64`, so the
    /// boundary conversion has to live somewhere. Non-finite input clamps
    /// to zero rather than panicking or propagating a NaN into the ledger.
    #[must_use]
    pub fn from_usd(usd: f64) -> Self {
        if !usd.is_finite() {
            return Self::ZERO;
        }
        #[allow(clippy::cast_possible_truncation)]
        let micros = (usd * Self::MICROS_PER_USD).round() as i64;
        Self(micros)
    }

    /// Convert back to a floating-point USD amount, for display or for
    /// interop with the (still-`f64`) `turn_metrics` ledger.
    #[must_use]
    pub fn to_usd(self) -> f64 {
        self.0 as f64 / Self::MICROS_PER_USD
    }

    /// Raw micro-USD count.
    #[must_use]
    pub const fn micros(self) -> i64 {
        self.0
    }

    /// `true` if this amount is exactly zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Addition that saturates instead of overflowing/panicking — a runaway
    /// accumulation should hit `i64::MAX` and stay display-safe, not wrap.
    #[must_use]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// Subtraction that floors at zero — spend/headroom math should never
    /// go negative from a subtraction alone; callers that need to detect an
    /// over-spend compare before subtracting. `i64::saturating_sub` already
    /// floors at `i64::MIN`, not zero, so a negative result is clamped
    /// explicitly rather than relying on the built-in saturation.
    #[must_use]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        let diff = self.0.saturating_sub(rhs.0);
        Self(if diff < 0 { 0 } else { diff })
    }

    /// This amount as a fraction of `whole`, in `[0.0, 1.0]` (unclamped
    /// above 1.0 if this exceeds `whole` — callers decide what an
    /// over-100% ratio means). `whole == 0` returns `0.0` rather than
    /// dividing by zero.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn ratio_of(self, whole: Self) -> f64 {
        if whole.0 == 0 {
            return 0.0;
        }
        self.0 as f64 / whole.0 as f64
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${:.4}", self.to_usd())
    }
}

impl Add for Money {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        self.saturating_add(rhs)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Money {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self.saturating_sub(rhs)
    }
}

impl SubAssign for Money {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Default for Money {
    fn default() -> Self {
        Self::ZERO
    }
}

/// The billing period an [`Pool::ApiKey`] hard ceiling resets on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    /// Resets every UTC midnight.
    Daily,
    /// Resets every 7 days from the ceiling's configured anchor.
    Weekly,
    /// Resets on a fixed day-of-month.
    Monthly,
}

/// One configured spending resource. Finding #10: these have genuinely
/// different exhaustion semantics, and the old governor's single
/// `usd_per_hour` model conflated them — an `AgentSdkCredits` pool hits a
/// wall (no spillover into general subscription usage), an `ApiKey` has no
/// wall at all (a ceiling here is mandatory, not a nicety), and
/// `ExtraUsage` depletes monotonically and never refills on its own.
///
/// **Explicit non-goal** (see Sprint E brief, Part 1): lopi never rotates
/// across pools to route around exhaustion. Exactly one `Pool` is active at
/// a time; when it's out, every admission refuses and says so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pool {
    /// Subscription-backed Agent SDK credits (Finding #10's reinstated
    /// pool). Fixed monthly allotment, resets on a known date, cannot
    /// spill into general subscription usage — exhaustion is a hard wall
    /// until `resets_on`.
    AgentSdkCredits {
        /// Total credits granted each reset cycle.
        monthly_allotment: Money,
        /// The next date this pool refills to `monthly_allotment`.
        resets_on: NaiveDate,
    },
    /// Direct Anthropic API key billing. No natural wall — spend is
    /// unbounded unless lopi enforces one, which is why `hard_ceiling` is
    /// a required field here, not optional.
    ApiKey {
        /// The operator-configured hard spending ceiling.
        hard_ceiling: Money,
        /// How often `hard_ceiling`'s consumption resets.
        period: Period,
    },
    /// A prepaid extra-usage bundle purchased once credits ran out.
    /// Depletes monotonically; never resets on its own.
    ExtraUsage {
        /// Remaining balance in the bundle.
        remaining: Money,
    },
}

impl Pool {
    /// The total ceiling this pool enforces right now — for
    /// `AgentSdkCredits`/`ApiKey` this is the configured allotment/ceiling;
    /// for `ExtraUsage` it's whatever is left (there is no larger "total"
    /// to compare against — it only ever shrinks).
    #[must_use]
    pub const fn ceiling(&self) -> Money {
        match self {
            Self::AgentSdkCredits {
                monthly_allotment, ..
            } => *monthly_allotment,
            Self::ApiKey { hard_ceiling, .. } => *hard_ceiling,
            Self::ExtraUsage { remaining } => *remaining,
        }
    }

    /// Stable tag for logs/events — matches the `kind` serde tag.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::AgentSdkCredits { .. } => "agent_sdk_credits",
            Self::ApiKey { .. } => "api_key",
            Self::ExtraUsage { .. } => "extra_usage",
        }
    }
}

/// The degradation ladder's five rungs (Sprint E, Part 3), ordered
/// least-to-most severe — declaration order backs the derived [`Ord`], so
/// `BudgetTier::Drain > BudgetTier::Conserve` is a real comparison, not
/// just a discriminant coincidence.
///
/// The invariant every rung must uphold: **no agent is ever killed
/// mid-stage.** Every stop path (`Essential` onward) goes through the
/// handoff writer (`lopi_orchestrator::budget::ladder::write_handoff`)
/// before a task stops being admitted to its next stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetTier {
    /// Configured models and effort throughout — no degradation.
    Full,
    /// Drop effort one level on implement/optimize stages only. Plan,
    /// verify, and the adversarial reviewer are never touched — cutting
    /// reasoning there costs more in retries than it saves.
    Conserve,
    /// No new task admissions. In-flight tasks may only run stages that
    /// reach a clean handoff.
    Essential,
    /// In-flight agents run to their next handoff checkpoint and stop.
    /// Queue frozen.
    Drain,
    /// Hard stop. Everything dead-lettered with resumable state.
    Halt,
}

impl BudgetTier {
    /// Stable wire tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Conserve => "conserve",
            Self::Essential => "essential",
            Self::Drain => "drain",
            Self::Halt => "halt",
        }
    }

    /// `true` once this tier stops admitting brand-new tasks
    /// (`Essential` and everything more severe).
    #[must_use]
    pub const fn admits_new_tasks(self) -> bool {
        matches!(self, Self::Full | Self::Conserve)
    }

    /// `true` once this tier requires every in-flight task to stop at its
    /// next handoff checkpoint rather than continue to a new stage
    /// (`Essential` and everything more severe).
    #[must_use]
    pub const fn requires_handoff_checkpoint(self) -> bool {
        !matches!(self, Self::Full | Self::Conserve)
    }
}

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
            ("conserve_below", self.conserve_below, "essential_below", self.essential_below),
            ("essential_below", self.essential_below, "drain_below", self.drain_below),
            ("drain_below", self.drain_below, "halt_below", self.halt_below),
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

    #[test]
    fn money_round_trips_usd() {
        let m = Money::from_usd(4.20);
        assert!((m.to_usd() - 4.20).abs() < 1e-9);
        assert_eq!(m.micros(), 4_200_000);
    }

    #[test]
    fn money_non_finite_clamps_to_zero() {
        assert_eq!(Money::from_usd(f64::NAN), Money::ZERO);
        assert_eq!(Money::from_usd(f64::INFINITY), Money::ZERO);
    }

    #[test]
    fn money_saturating_sub_floors_at_zero() {
        let a = Money::from_usd(1.0);
        let b = Money::from_usd(5.0);
        assert_eq!(a.saturating_sub(b), Money::ZERO);
    }

    #[test]
    fn money_saturating_add_does_not_overflow() {
        let a = Money::from_micros(i64::MAX - 1);
        let b = Money::from_micros(10);
        assert_eq!(a.saturating_add(b), Money::from_micros(i64::MAX));
    }

    #[test]
    fn money_ops_traits_match_saturating_methods() {
        let a = Money::from_usd(2.0);
        let b = Money::from_usd(1.5);
        assert_eq!(a + b, a.saturating_add(b));
        assert_eq!(a - b, a.saturating_sub(b));
        let mut c = a;
        c += b;
        assert_eq!(c, a + b);
        c -= b;
        assert_eq!(c, a);
    }

    #[test]
    fn money_ratio_of_handles_zero_whole() {
        assert!((Money::from_usd(5.0).ratio_of(Money::ZERO) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn money_ratio_of_computes_fraction() {
        let spent = Money::from_usd(25.0);
        let total = Money::from_usd(100.0);
        assert!((spent.ratio_of(total) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn money_display_formats_as_usd() {
        assert_eq!(Money::from_usd(4.2).to_string(), "$4.2000");
    }

    #[test]
    fn pool_ceiling_matches_variant() {
        let sdk = Pool::AgentSdkCredits {
            monthly_allotment: Money::from_usd(100.0),
            resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date"),
        };
        assert_eq!(sdk.ceiling(), Money::from_usd(100.0));
        assert_eq!(sdk.kind(), "agent_sdk_credits");

        let key = Pool::ApiKey {
            hard_ceiling: Money::from_usd(50.0),
            period: Period::Daily,
        };
        assert_eq!(key.ceiling(), Money::from_usd(50.0));

        let extra = Pool::ExtraUsage {
            remaining: Money::from_usd(12.5),
        };
        assert_eq!(extra.ceiling(), Money::from_usd(12.5));
    }

    #[test]
    fn pool_round_trips_through_json() {
        let p = Pool::AgentSdkCredits {
            monthly_allotment: Money::from_usd(100.0),
            resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date"),
        };
        let json = serde_json::to_string(&p).expect("serialize");
        let back: Pool = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn budget_tier_ordering_is_severity_order() {
        assert!(BudgetTier::Full < BudgetTier::Conserve);
        assert!(BudgetTier::Conserve < BudgetTier::Essential);
        assert!(BudgetTier::Essential < BudgetTier::Drain);
        assert!(BudgetTier::Drain < BudgetTier::Halt);
    }

    #[test]
    fn budget_tier_admits_new_tasks_only_below_essential() {
        assert!(BudgetTier::Full.admits_new_tasks());
        assert!(BudgetTier::Conserve.admits_new_tasks());
        assert!(!BudgetTier::Essential.admits_new_tasks());
        assert!(!BudgetTier::Drain.admits_new_tasks());
        assert!(!BudgetTier::Halt.admits_new_tasks());
    }

    #[test]
    fn budget_tier_handoff_checkpoint_required_from_essential_on() {
        assert!(!BudgetTier::Full.requires_handoff_checkpoint());
        assert!(!BudgetTier::Conserve.requires_handoff_checkpoint());
        assert!(BudgetTier::Essential.requires_handoff_checkpoint());
        assert!(BudgetTier::Drain.requires_handoff_checkpoint());
        assert!(BudgetTier::Halt.requires_handoff_checkpoint());
    }

    #[test]
    fn budget_tier_wire_tags_are_stable() {
        assert_eq!(BudgetTier::Full.as_str(), "full");
        assert_eq!(BudgetTier::Conserve.as_str(), "conserve");
        assert_eq!(BudgetTier::Essential.as_str(), "essential");
        assert_eq!(BudgetTier::Drain.as_str(), "drain");
        assert_eq!(BudgetTier::Halt.as_str(), "halt");
    }

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
