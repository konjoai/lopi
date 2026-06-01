//! lopi-context — token-budget context window with phase-aware eviction for agent message history.

/// Error types for context window operations.
pub mod error;
/// Eviction policy implementations.
pub mod eviction;
/// Statistics and eviction record types.
pub mod stats;
/// Token count estimation utilities.
pub mod tokens;
/// Core message types: roles, phases, pin policies, and tagged messages.
pub mod types;
/// The `ContextWindow` and `ApiMessage` types.
pub mod window;

pub use error::ContextError;
pub use stats::{ContextStats, EvictionReason, EvictionRecord, EvictionStats};
pub use tokens::estimate_tokens;
pub use types::{ContentBlock, Phase, PinPolicy, Role, TaggedMessage, ToolPairId, TurnId};
pub use window::{ApiMessage, ContextWindow};
