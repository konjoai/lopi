//! `lopi-tools` — durable tool registry.
//!
//! Agents declare which external tools they are allowed to call by name; the
//! registry holds each tool's spec (`name`, `description`, JSON-Schema
//! parameters, timeout, retry budget). The registry persists every write to
//! a JSON file so registrations survive `lopi sail` restarts.
//!
//! Tier 2 (data) under `.konjo/arch.toml` — depends on no other lopi crate.
//! The agent runner (tier 4) gates calls against [`ToolRegistry::get`] before
//! invoking anything, so an unregistered tool name is rejected at the
//! registry boundary, not the tool implementation.
//!
//! Persistence is best-effort: a write that fails the disk flush still
//! updates the in-memory map and logs via `tracing::warn` — the next
//! successful save reconciles state. Callers that need stronger guarantees
//! should call [`ToolRegistry::save_to_disk`] explicitly and check the
//! `Result`.

pub mod registry;

pub use registry::{
    default_registry_path, RegistryError, ToolRegistry, ToolRegistrySnapshot, ToolSpec,
};
