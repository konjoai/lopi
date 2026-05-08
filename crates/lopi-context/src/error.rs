use crate::types::TurnId;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContextError {
    #[error(
        "context window full: budget {budget}, needed {needed}, after_eviction {after_eviction}"
    )]
    Full {
        budget: usize,
        needed: usize,
        after_eviction: usize,
    },

    #[error("cannot evict turn {id}: partner tool turn {partner_id} would be orphaned")]
    OrphanedToolPair { id: TurnId, partner_id: TurnId },

    #[error("cannot evict pinned turn {id} without force flag")]
    ForcedPinViolation { id: TurnId },

    #[error("turn {id} not found")]
    TurnNotFound { id: TurnId },
}
