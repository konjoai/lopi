//! Sprint E, Part 2 — the reservation ledger.
//!
//! "Admission becomes a reservation... Reservation is a real hold, not an
//! advisory number — concurrent admissions must not each see the same
//! headroom." This is the one place in lopi where a race is unacceptable:
//! [`ReservationLedger`] serializes every mutation through a single
//! `tokio::sync::Mutex` (the "single writer" the brief asks for), so two
//! concurrent `try_reserve` calls against a thin pool can never both
//! succeed when only one of them fits.
//!
//! A hold that's never reconciled or released (a crashed or killed agent)
//! would otherwise shrink the pool forever — "a leaked hold that silently
//! shrinks the pool is the worst failure mode here." Every hold carries a
//! TTL and is swept lazily (on the next ledger operation) once expired.

use lopi_core::Money;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;
use uuid::Uuid;

/// Opaque handle to one reservation. Returned by [`ReservationLedger::try_reserve`];
/// pass it back to [`ReservationLedger::reconcile`] or
/// [`ReservationLedger::release`] to close it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReservationId(Uuid);

impl ReservationId {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

struct Hold {
    amount: Money,
    created_at: Instant,
    ttl: Duration,
}

impl Hold {
    fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.created_at) >= self.ttl
    }
}

struct LedgerState {
    /// Finalized spend — amounts that were reserved and then reconciled
    /// against an actual cost. Only ever grows.
    committed: Money,
    holds: HashMap<ReservationId, Hold>,
}

/// Why a reservation was refused — carries enough to build the brief's
/// "declined: p90 estimate $4.20, headroom $2.10" message without a second
/// query.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decline {
    /// The amount that was requested and did not fit.
    pub requested: Money,
    /// Headroom actually available at decline time.
    pub headroom: Money,
}

/// Single-writer reservation ledger for one [`crate::budget::pool::PoolState`]'s
/// active [`lopi_core::Pool`]. `ceiling` is fixed at construction — a pool's
/// nominal size doesn't change mid-process; `PoolState` rebuilds the ledger
/// on a config reload instead of mutating this in place.
pub struct ReservationLedger {
    ceiling: Money,
    state: Mutex<LedgerState>,
}

impl ReservationLedger {
    /// Build an empty ledger against `ceiling` — no committed spend, no
    /// open holds.
    #[must_use]
    pub fn new(ceiling: Money) -> Self {
        Self {
            ceiling,
            state: Mutex::new(LedgerState {
                committed: Money::ZERO,
                holds: HashMap::new(),
            }),
        }
    }

    /// Attempt to reserve `amount` for up to `ttl`. Sweeps expired holds
    /// first (so a leaked hold's headroom is always recovered before the
    /// next admission decision, not just on a timer), then admits only if
    /// `amount` fits in what's left of `ceiling` after committed spend and
    /// every other still-live hold.
    ///
    /// # Errors
    /// Returns [`Decline`] with the headroom that *was* available when
    /// `amount` doesn't fit.
    pub async fn try_reserve(&self, amount: Money, ttl: Duration) -> Result<ReservationId, Decline> {
        let mut state = self.state.lock().await;
        sweep_expired(&mut state);
        let headroom = self.headroom_locked(&state);
        if amount > headroom {
            return Err(Decline {
                requested: amount,
                headroom,
            });
        }
        let id = ReservationId::new();
        state.holds.insert(
            id,
            Hold {
                amount,
                created_at: Instant::now(),
                ttl,
            },
        );
        Ok(id)
    }

    /// Close a reservation out with its real cost: removes the hold and
    /// adds `actual` to committed spend. Reconciling for an amount lower
    /// than the original reservation is exactly how the freed difference
    /// gets returned to the pool (the brief's "release the difference") —
    /// the hold's full `amount` simply stops counting against headroom the
    /// instant it's removed, replaced by whatever `actual` turned out to be.
    pub async fn reconcile(&self, id: ReservationId, actual: Money) {
        let mut state = self.state.lock().await;
        state.holds.remove(&id);
        state.committed += actual;
    }

    /// Release a reservation without recording any spend — used when a
    /// task is declined after reserving speculatively, or cancelled before
    /// any billable call happened.
    pub async fn release(&self, id: ReservationId) {
        let mut state = self.state.lock().await;
        state.holds.remove(&id);
    }

    /// Headroom remaining right now: `ceiling - committed - sum(live holds)`.
    /// Sweeps expired holds first, so a leaked reservation's TTL expiring
    /// is enough to recover its headroom on the very next read — no
    /// background sweeper task required.
    pub async fn headroom(&self) -> Money {
        let mut state = self.state.lock().await;
        sweep_expired(&mut state);
        self.headroom_locked(&state)
    }

    /// Committed (reconciled) spend to date.
    pub async fn committed(&self) -> Money {
        self.state.lock().await.committed
    }

    /// Sum of every still-live hold. Sweeps expired holds first.
    pub async fn reserved(&self) -> Money {
        let mut state = self.state.lock().await;
        sweep_expired(&mut state);
        sum_holds(&state)
    }

    /// Number of currently-live holds — for tests and diagnostics.
    pub async fn open_reservation_count(&self) -> usize {
        let mut state = self.state.lock().await;
        sweep_expired(&mut state);
        state.holds.len()
    }

    fn headroom_locked(&self, state: &LedgerState) -> Money {
        self.ceiling.saturating_sub(state.committed + sum_holds(state))
    }
}

fn sum_holds(state: &LedgerState) -> Money {
    state
        .holds
        .values()
        .fold(Money::ZERO, |acc, h| acc + h.amount)
}

fn sweep_expired(state: &mut LedgerState) {
    let now = Instant::now();
    state.holds.retain(|_, h| !h.expired(now));
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn try_reserve_admits_when_it_fits() {
        let ledger = ReservationLedger::new(Money::from_usd(10.0));
        let id = ledger
            .try_reserve(Money::from_usd(4.0), Duration::from_secs(60))
            .await
            .expect("fits under ceiling");
        assert_eq!(ledger.headroom().await, Money::from_usd(6.0));
        ledger.release(id).await;
        assert_eq!(ledger.headroom().await, Money::from_usd(10.0));
    }

    #[tokio::test]
    async fn try_reserve_declines_with_headroom_when_it_does_not_fit() {
        let ledger = ReservationLedger::new(Money::from_usd(5.0));
        let err = ledger
            .try_reserve(Money::from_usd(6.0), Duration::from_secs(60))
            .await
            .expect_err("must not fit");
        assert_eq!(err.requested, Money::from_usd(6.0));
        assert_eq!(err.headroom, Money::from_usd(5.0));
    }

    #[tokio::test]
    async fn reconcile_replaces_hold_with_actual_committed_spend() {
        let ledger = ReservationLedger::new(Money::from_usd(10.0));
        let id = ledger
            .try_reserve(Money::from_usd(4.0), Duration::from_secs(60))
            .await
            .expect("fits");
        // Actual came in lower than reserved — the difference is returned.
        ledger.reconcile(id, Money::from_usd(1.5)).await;
        assert_eq!(ledger.committed().await, Money::from_usd(1.5));
        assert_eq!(ledger.reserved().await, Money::ZERO);
        assert_eq!(ledger.headroom().await, Money::from_usd(8.5));
    }

    #[tokio::test]
    async fn concurrent_admission_against_a_thin_pool_never_oversubscribes() {
        // Ceiling fits exactly 5 of 10 concurrent $1 requests. This is the
        // brief's explicit test: "Test concurrent admission against a thin
        // pool and assert no oversubscription."
        let ledger = Arc::new(ReservationLedger::new(Money::from_usd(5.0)));
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let ledger = ledger.clone();
            tasks.push(tokio::spawn(async move {
                ledger
                    .try_reserve(Money::from_usd(1.0), Duration::from_secs(60))
                    .await
            }));
        }
        let mut admitted = 0;
        for t in tasks {
            if t.await.expect("task panicked").is_ok() {
                admitted += 1;
            }
        }
        assert_eq!(admitted, 5, "exactly 5 of 10 $1 requests fit a $5 ceiling");
        assert_eq!(ledger.reserved().await, Money::from_usd(5.0));
        assert_eq!(ledger.headroom().await, Money::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn a_leaked_hold_recovers_its_headroom_after_ttl_expiry() {
        let ledger = ReservationLedger::new(Money::from_usd(5.0));
        // Reserved and then never reconciled/released — simulates a crash.
        let _leaked = ledger
            .try_reserve(Money::from_usd(5.0), Duration::from_millis(100))
            .await
            .expect("fits exactly");
        assert_eq!(ledger.headroom().await, Money::ZERO);

        tokio::time::advance(Duration::from_millis(200)).await;

        assert_eq!(
            ledger.headroom().await,
            Money::from_usd(5.0),
            "expired hold must be swept and its headroom recovered"
        );
        assert_eq!(ledger.open_reservation_count().await, 0);
        // The leaked hold was never reconciled — no phantom spend either.
        assert_eq!(ledger.committed().await, Money::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn expiry_is_swept_on_the_next_operation_not_just_on_read() {
        let ledger = ReservationLedger::new(Money::from_usd(5.0));
        let _leaked = ledger
            .try_reserve(Money::from_usd(5.0), Duration::from_millis(50))
            .await
            .expect("fits exactly");
        tokio::time::advance(Duration::from_millis(100)).await;

        // A fresh reservation request triggers the sweep internally and
        // must succeed even though nothing explicitly called headroom().
        let id = ledger
            .try_reserve(Money::from_usd(5.0), Duration::from_secs(60))
            .await
            .expect("expired hold must have been swept before this admission check");
        ledger.release(id).await;
    }

    #[tokio::test]
    async fn release_frees_the_hold_without_recording_spend() {
        let ledger = ReservationLedger::new(Money::from_usd(5.0));
        let id = ledger
            .try_reserve(Money::from_usd(5.0), Duration::from_secs(60))
            .await
            .expect("fits exactly");
        ledger.release(id).await;
        assert_eq!(ledger.committed().await, Money::ZERO);
        assert_eq!(ledger.headroom().await, Money::from_usd(5.0));
    }

    #[tokio::test]
    async fn reserved_pool_final_balance_is_zero_after_every_hold_resolves() {
        // Mirrors the exhaustion drill's final assertion: the pool's
        // reserved balance must be exactly zero once every task is done,
        // never a leaked hold.
        let ledger = ReservationLedger::new(Money::from_usd(20.0));
        let mut ids = Vec::new();
        for _ in 0..4 {
            ids.push(
                ledger
                    .try_reserve(Money::from_usd(5.0), Duration::from_secs(60))
                    .await
                    .expect("fits"),
            );
        }
        for (i, id) in ids.into_iter().enumerate() {
            if i % 2 == 0 {
                ledger.reconcile(id, Money::from_usd(4.0)).await;
            } else {
                ledger.release(id).await;
            }
        }
        assert_eq!(ledger.reserved().await, Money::ZERO);
        assert_eq!(ledger.committed().await, Money::from_usd(8.0));
    }
}
