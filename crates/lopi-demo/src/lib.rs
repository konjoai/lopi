//! Deterministic, seeded synthetic-store generator behind `lopi demo`.
//!
//! Fabricates a complete, self-consistent lopi store — repos, tasks across
//! every status, agent traffic, token counts, a quality trend, patterns,
//! lessons, and at least one honest failure — so someone can see a fully
//! alive dashboard with zero setup. Nothing here reads the real machine:
//! no environment inspection, no git calls, no filesystem scans of real
//! repos. See `docs/adr/0001-demo-mode-and-measurement.md` for the design
//! rationale and `docs/MEASUREMENT.md` for how synthetic data is marked.
//!
//! This is a library crate (not CLI-only code) so both the `lopi` binary
//! (`src/demo_commands.rs`) and integration/snapshot tests can depend on
//! the same generator — see [`generate`].

#![warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod content;
pub mod generator;
mod generator_content;
mod generator_seed;
pub mod scenario;

pub use generator::{
    default_demo_store_path, generate, GeneratedDemo, DEFAULT_DEMO_SEED, DEMO_DB_FILENAME,
};
pub use scenario::replay_events;
