//! Sprint E, Part 1/5 — the active spending resource + runway.
//!
//! Wraps a [`Pool`] (the operator's configured resource — `lopi.toml`'s
//! `[economics]` table) with a [`ReservationLedger`] sized to that pool's
//! `ceiling()`. This is the thing `budget::estimate`/`budget::ladder`/
//! `budget::detect` all read headroom from.

use super::reserve::{Decline, ReservationId, ReservationLedger};
use lopi_core::{Money, Pool};
use std::time::Duration;

/// The operator's configured spending resource plus its live reservation
/// ledger. One `PoolState` per running `lopi sail` process — matches the
/// brief's "exactly one `Pool` is active at a time."
pub struct PoolState {
    pool: Pool,
    ledger: ReservationLedger,
}

impl PoolState {
    /// Build a `PoolState` for `pool`, with an empty ledger sized to its
    /// `ceiling()`.
    #[must_use]
    pub fn new(pool: Pool) -> Self {
        let ledger = ReservationLedger::new(pool.ceiling());
        Self { pool, ledger }
    }

    /// The configured resource this state tracks.
    #[must_use]
    pub const fn pool(&self) -> &Pool {
        &self.pool
    }

    /// The pool's total ceiling — unchanging for the lifetime of this
    /// `PoolState` (a config reload rebuilds a new one).
    #[must_use]
    pub fn ceiling(&self) -> Money {
        self.pool.ceiling()
    }

    /// Headroom remaining: `ceiling - committed - reserved`.
    pub async fn headroom(&self) -> Money {
        self.ledger.headroom().await
    }

    /// Headroom as a fraction of ceiling, `[0.0, 1.0+]` — the input to
    /// `LadderThresholds::tier_for_ratio`.
    pub async fn headroom_ratio(&self) -> f64 {
        self.headroom().await.ratio_of(self.ceiling())
    }

    /// Reserve `amount` for up to `ttl`. See [`ReservationLedger::try_reserve`].
    ///
    /// # Errors
    /// Returns [`Decline`] when `amount` doesn't fit in current headroom.
    pub async fn try_reserve(&self, amount: Money, ttl: Duration) -> Result<ReservationId, Decline> {
        self.ledger.try_reserve(amount, ttl).await
    }

    /// Reconcile a reservation against its actual cost. See
    /// [`ReservationLedger::reconcile`].
    pub async fn reconcile(&self, id: ReservationId, actual: Money) {
        self.ledger.reconcile(id, actual).await;
    }

    /// Release a reservation without recording spend. See
    /// [`ReservationLedger::release`].
    pub async fn release(&self, id: ReservationId) {
        self.ledger.release(id).await;
    }

    /// Committed (reconciled) spend to date.
    pub async fn committed(&self) -> Money {
        self.ledger.committed().await
    }

    /// Days remaining at `daily_burn` before headroom hits zero — the
    /// "pool runway" unit economics number (Part 5). `daily_burn ==
    /// Money::ZERO` returns `f64::INFINITY` (no burn, no exhaustion date to
    /// report) rather than dividing by zero.
    #[must_use]
    pub async fn runway_days(&self, daily_burn: Money) -> f64 {
        if daily_burn.is_zero() {
            return f64::INFINITY;
        }
        #[allow(clippy::cast_precision_loss)]
        let days = self.headroom().await.micros() as f64 / daily_burn.micros() as f64;
        days.max(0.0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn sdk_pool(usd: f64) -> Pool {
        Pool::AgentSdkCredits {
            monthly_allotment: Money::from_usd(usd),
            resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date"),
        }
    }

    #[tokio::test]
    async fn fresh_pool_has_full_headroom() {
        let state = PoolState::new(sdk_pool(100.0));
        assert_eq!(state.headroom().await, Money::from_usd(100.0));
        assert!((state.headroom_ratio().await - 1.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn reservation_reduces_headroom_and_release_restores_it() {
        let state = PoolState::new(sdk_pool(100.0));
        let id = state
            .try_reserve(Money::from_usd(30.0), Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(state.headroom().await, Money::from_usd(70.0));
        state.release(id).await;
        assert_eq!(state.headroom().await, Money::from_usd(100.0));
    }

    #[tokio::test]
    async fn runway_days_computes_headroom_over_burn() {
        let state = PoolState::new(sdk_pool(70.0));
        let runway = state.runway_days(Money::from_usd(10.0)).await;
        assert!((runway - 7.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn runway_days_is_infinite_with_zero_burn() {
        let state = PoolState::new(sdk_pool(70.0));
        assert_eq!(state.runway_days(Money::ZERO).await, f64::INFINITY);
    }
}
