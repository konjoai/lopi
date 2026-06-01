use crate::types::TurnId;
use thiserror::Error;

/// Errors that can occur during context window operations.
#[derive(Error, Debug)]
pub enum ContextError {
    /// The token budget is exhausted and eviction could not free enough space.
    #[error(
        "context window full: budget {budget}, needed {needed}, after_eviction {after_eviction}"
    )]
    Full {
        /// Configured token budget.
        budget: usize,
        /// Tokens needed to insert the new message.
        needed: usize,
        /// Tokens remaining after eviction attempt.
        after_eviction: usize,
    },

    #[error("cannot evict turn {id}: partner tool turn {partner_id} would be orphaned")]
    /// Evicting this turn would leave its tool-use/tool-result partner without a pair.
    OrphanedToolPair {
        /// The turn that was a candidate for eviction.
        id: TurnId,
        /// The partner turn that would become an orphan.
        partner_id: TurnId,
    },

    /// The turn is pinned and the caller did not set the force flag.
    #[error("cannot evict pinned turn {id} without force flag")]
    ForcedPinViolation {
        /// The pinned turn that cannot be evicted.
        id: TurnId,
    },

    /// No turn with the given ID exists in the window.
    #[error("turn {id} not found")]
    TurnNotFound {
        /// The missing turn identifier.
        id: TurnId,
    },
}
