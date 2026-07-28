//! Knobs for the repo map's token budget and the bounded-read tool's line
//! cap. Kept in this crate (not folded into `lopi_core::LopiConfig`) so
//! `lopi-index` stays independently testable with its own defaults;
//! `lopi-index` does depend on `lopi-core` for the shared `sqlite_pool`
//! connection-setup helper, but that's plumbing, not configuration
//! coupling — no caller constructs an `IndexConfig` from a `LopiConfig`
//! field today.

use serde::{Deserialize, Serialize};

/// Repo-map / index-tool configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexConfig {
    /// Hard token budget for `RepoMap::build`'s output (default `2500`).
    #[serde(default = "default_map_token_budget")]
    pub map_token_budget: u32,
    /// Max lines `lopi_read` returns before eliding with a continuation
    /// marker (default `400`).
    #[serde(default = "default_max_read_lines")]
    pub max_read_lines: u32,
    /// Max results a single `lopi_find`/`lopi_refs` call returns (default `50`).
    #[serde(default = "default_max_results")]
    pub max_results: u32,
    /// Max `lopi_refs` traversal depth (default `3`, hard-capped at `3`
    /// regardless of a larger configured value — see [`Self::refs_depth`]).
    #[serde(default = "default_refs_depth")]
    pub max_refs_depth: u32,
    /// Directory-skeleton depth in the repo map (default `3`).
    #[serde(default = "default_skeleton_depth")]
    pub skeleton_depth: u32,
    /// How many of the most-referenced symbols the repo map lists as an
    /// orientation aid (default `15`).
    #[serde(default = "default_top_symbols")]
    pub top_referenced_symbols: u32,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            map_token_budget: default_map_token_budget(),
            max_read_lines: default_max_read_lines(),
            max_results: default_max_results(),
            max_refs_depth: default_refs_depth(),
            skeleton_depth: default_skeleton_depth(),
            top_referenced_symbols: default_top_symbols(),
        }
    }
}

impl IndexConfig {
    /// `max_refs_depth`, clamped to the brief's hard cap of `3` — a config
    /// value above that would let one `lopi_refs` call fan out into a grep
    /// spiral of its own, which is exactly what this tool exists to replace.
    #[must_use]
    pub fn refs_depth(&self) -> u32 {
        self.max_refs_depth.min(3)
    }
}

fn default_map_token_budget() -> u32 {
    2_500
}
fn default_max_read_lines() -> u32 {
    400
}
fn default_max_results() -> u32 {
    50
}
fn default_refs_depth() -> u32 {
    3
}
fn default_skeleton_depth() -> u32 {
    3
}
fn default_top_symbols() -> u32 {
    15
}

#[cfg(test)]
mod tests {
    use super::IndexConfig;

    #[test]
    fn refs_depth_clamps_above_three() {
        let cfg = IndexConfig {
            max_refs_depth: 99,
            ..IndexConfig::default()
        };
        assert_eq!(cfg.refs_depth(), 3);
    }

    #[test]
    fn refs_depth_passes_through_when_within_cap() {
        let cfg = IndexConfig {
            max_refs_depth: 2,
            ..IndexConfig::default()
        };
        assert_eq!(cfg.refs_depth(), 2);
    }

    #[test]
    fn defaults_match_the_brief() {
        let cfg = IndexConfig::default();
        assert_eq!(cfg.map_token_budget, 2_500);
        assert_eq!(cfg.max_read_lines, 400);
        assert_eq!(cfg.refs_depth(), 3);
    }
}
