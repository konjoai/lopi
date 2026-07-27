//! Sprint S11, Phase 0 — single-use, short-lived tickets for browser-based
//! `/ws`, `/ws/tasks`, `/sse` auth.
//!
//! Browsers cannot set custom headers on a `WebSocket`/`EventSource` upgrade
//! request, so those three routes accept a ticket (`?ticket=<value>`) as an
//! alternative to `Authorization: Bearer`. A ticket is minted only by an
//! already-authenticated `POST /api/ws-ticket` call, is valid for
//! [`TICKET_TTL`], and is consumed (removed) on its first successful use —
//! it cannot be replayed even within its TTL.

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a minted ticket remains redeemable if never used.
const TICKET_TTL: Duration = Duration::from_secs(30);

/// Shared, `Clone`-cheap store of outstanding tickets.
#[derive(Clone, Default)]
pub struct TicketStore(Arc<DashMap<String, Instant>>);

impl TicketStore {
    /// Mint a new single-use ticket valid for [`TICKET_TTL`].
    pub fn mint(&self) -> String {
        let ticket = uuid::Uuid::new_v4().to_string();
        self.0.insert(ticket.clone(), Instant::now() + TICKET_TTL);
        self.sweep_expired();
        ticket
    }

    /// Redeem `ticket`: valid and unexpired tickets are removed (single-use)
    /// and return `true`; anything else — unknown, already-consumed, or
    /// expired — returns `false` without side effects beyond the removal
    /// itself (an expired entry is dropped either way).
    pub fn consume(&self, ticket: &str) -> bool {
        match self.0.remove(ticket) {
            Some((_, expiry)) => Instant::now() < expiry,
            None => false,
        }
    }

    /// Opportunistic cleanup so an abandoned (never-redeemed) ticket doesn't
    /// sit in the map forever. Called on every mint; cheap at the volumes a
    /// per-IP-rate-limited mint endpoint can produce.
    fn sweep_expired(&self) {
        let now = Instant::now();
        self.0.retain(|_, expiry| *expiry > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_freshly_minted_ticket_is_consumable_exactly_once() {
        let store = TicketStore::default();
        let ticket = store.mint();
        assert!(store.consume(&ticket), "first redemption must succeed");
        assert!(
            !store.consume(&ticket),
            "a consumed ticket must not be redeemable again"
        );
    }

    #[test]
    fn an_unknown_ticket_is_rejected() {
        let store = TicketStore::default();
        assert!(!store.consume("never-minted"));
    }

    #[test]
    fn an_expired_ticket_is_rejected_even_if_never_consumed() {
        let store = TicketStore::default();
        let ticket = uuid::Uuid::new_v4().to_string();
        // Insert directly with an already-past expiry — equivalent to a
        // ticket minted long enough ago to have aged out.
        store
            .0
            .insert(ticket.clone(), Instant::now() - Duration::from_secs(1));
        assert!(!store.consume(&ticket));
    }

    #[test]
    fn distinct_mints_are_independent() {
        let store = TicketStore::default();
        let a = store.mint();
        let b = store.mint();
        assert_ne!(a, b);
        assert!(store.consume(&a));
        assert!(store.consume(&b), "consuming `a` must not affect `b`");
    }
}
