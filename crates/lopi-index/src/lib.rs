//! `lopi-index` — symbol-indexed repo navigation.
//!
//! Parses a repo with tree-sitter into a per-repo `SQLite` database
//! (`symbols`/`refs`), reindexes incrementally off `git diff`, and exposes
//! two read paths built on top of it: [`map::RepoMap`] (a deterministic,
//! token-budgeted orientation document meant for a planning prompt) and
//! [`query`]'s bounded find/read/refs/composite-query operations (meant for
//! on-demand tool calls, never eagerly injected).
//!
//! See `LEDGER.md`'s Finding #4 entry for the scope decisions this crate
//! makes against the brief it was built from — most importantly, that this
//! codebase has no raw-file-content-injection site to "rip out" (Part 4 of
//! the brief), and that Sprint C's `PrefixBuilder`/cached-prefix
//! infrastructure this crate was meant to depend on was never built.

#![warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod config;
mod hash;
pub mod map;
pub mod parse;
pub mod query;
pub mod reindex;
mod store;
pub mod types;

pub use config::IndexConfig;
pub use hash::hash_bytes;
pub use map::RepoMap;
pub use store::{IndexStore, RefDirection, RefHit, SymbolFilter, INDEX_DB_REL_PATH};
pub use types::{IndexDelta, Language, NewRef, NewSymbol, Ref, Symbol, SymbolKind};
