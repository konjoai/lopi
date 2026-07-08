//! Shared `TaggedMessage` builder for `lopi-context`'s integration tests.
//!
//! `tests/common/mod.rs` (not `tests/common.rs`) is the standard Rust idiom
//! for code shared across integration-test binaries — each `tests/*.rs` file
//! compiles as its own crate, so this is the one place the literal
//! `TaggedMessage { .. }` fixture is written; every test file's own
//! narrower helper (fixed role, fixed tokens, ...) delegates to this.

#![allow(dead_code)] // not every test file uses every field combination

use lopi_context::{ContentBlock, Phase, PinPolicy, Role, TaggedMessage};
use uuid::Uuid;

/// Build a `TaggedMessage` with every field explicit.
pub fn make_msg(role: Role, text: &str, phase: Phase, pin: PinPolicy, tokens: usize) -> TaggedMessage {
    TaggedMessage {
        id: Uuid::new_v4(),
        role,
        content: vec![ContentBlock::Text(text.to_string())],
        tokens,
        pin,
        phase,
        evict_after: None,
        tool_pair_id: None,
        is_conclusion: false,
    }
}
