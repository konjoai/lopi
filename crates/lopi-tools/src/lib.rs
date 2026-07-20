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
//! Persistence is strict by default, with one deliberate exception:
//! [`ToolRegistry::register`] and [`ToolRegistry::clear`] apply the mutation
//! in memory first, then flush to disk and return that flush's `Result` — a
//! failed flush is reported to the caller as the call failing, even though
//! the in-memory map already changed. [`ToolRegistry::deregister`] is the
//! one best-effort operation: it always applies in memory and returns
//! whether an entry was removed as a plain `bool`, only logging via
//! `tracing::warn` if the subsequent flush fails, on the theory that
//! removing a tool should never be blocked by a transient disk error.
//! Callers that need a guaranteed-current on-disk snapshot after any
//! mutation should call [`ToolRegistry::save_to_disk`] explicitly and check
//! the `Result`.

pub mod registry;

pub use registry::{
    default_registry_path, RegistryError, ToolRegistry, ToolRegistrySnapshot, ToolSpec,
};
