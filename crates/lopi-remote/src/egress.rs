//! Sprint S2, Phase 4 — egress allowlist for proactive outbound sends.
//!
//! Constrains where lopi's automated, non-reply Telegram sends (completion
//! notifications, report-on-finish) may go, checked in the transport layer
//! — not in a prompt or a policy string an agent's own output could
//! influence. Deny by default: an empty allowlist means no outbound sends,
//! not unrestricted ones, mirroring the fail-closed shape
//! `lopi_ui::web::auth_policy` already uses for Phase 1.
//!
//! Deliberately separate from `allowed_chat_ids` (inbound command authz,
//! `telegram/mod.rs`'s `BotDeps::allowed`): that list defaults *open*
//! ("empty = allow all chats", a documented dev-mode convenience) because a
//! reply only ever goes back to a chat that already passed the inbound
//! check on the way in. A proactive send has no such upstream gate, so its
//! allowlist defaults *closed*.

use tracing::warn;

/// Whether `destination` may receive a proactive/automated send.
#[must_use]
pub fn is_allowed_destination(allowlist: &[i64], destination: i64) -> bool {
    allowlist.contains(&destination)
}

/// Check `destination` against `allowlist`; log a security event (not a
/// generic warning) and return `false` when it's refused, so callers can
/// skip the send without duplicating the log line.
pub fn check_egress(allowlist: &[i64], destination: i64, kind: &str) -> bool {
    if is_allowed_destination(allowlist, destination) {
        return true;
    }
    warn!(
        target: "lopi_remote::security",
        chat_id = destination,
        kind,
        "egress denied: destination not in egress_allowed_chat_ids"
    );
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlisted_destination_is_allowed() {
        assert!(is_allowed_destination(&[111, 222], 111));
    }

    #[test]
    fn non_allowlisted_destination_is_denied() {
        assert!(!is_allowed_destination(&[111, 222], 333));
    }

    /// The specific empty-allowlist regression this phase's verify criteria
    /// calls out: empty must deny, never fall through to "unrestricted".
    #[test]
    fn empty_allowlist_denies_rather_than_permits() {
        assert!(!is_allowed_destination(&[], 111));
        assert!(!check_egress(&[], 111, "notify"));
    }

    #[test]
    fn check_egress_matches_is_allowed_destination() {
        assert!(check_egress(&[111], 111, "notify"));
        assert!(!check_egress(&[111], 222, "notify"));
    }
}
