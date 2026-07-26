//! Sprint F2 Phase 3 — externalized per-model token pricing.
//!
//! Rates used to be hardcoded Rust constants in `ApiUsage::estimated_cost`
//! (`api_client.rs`), so a price change needed a recompile — and the doc
//! comment above them already recorded one 3x-stale rate that had shipped
//! from an Opus 4.1 price left in place after Opus 4.1's own retirement.
//! This table is read from disk at process start instead.
//!
//! This is the **fallback** input, not the primary source: the CLI's own
//! authoritative `total_cost_usd` (surfaced via `ClaudeOutput::cost_usd`)
//! always wins when present. This table backs only the direct-API planning
//! path and the mid-stream `--max-budget-usd` estimate, neither of which the
//! CLI's own reported cost covers.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Per-million-token USD rates for one pricing tier (`opus`/`haiku`/`sonnet`).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TierRates {
    /// Rate per 1M input (prompt) tokens.
    pub input: f64,
    /// Rate per 1M output (completion) tokens.
    pub output: f64,
    /// Rate per 1M tokens served from Anthropic's KV cache.
    pub cache_read: f64,
    /// Rate per 1M tokens written into Anthropic's KV cache this turn.
    pub cache_write: f64,
}

/// A pricing file's shape: a flat map of tier name to rates, matching the
/// `[opus]`/`[haiku]`/`[sonnet]` tables in `pricing.toml`.
#[derive(Debug, Deserialize)]
struct PriceFile {
    #[serde(flatten)]
    tiers: HashMap<String, TierRates>,
}

/// Compiled-in default rates, always available even with no override file
/// on disk. Kept in a real TOML file (not Rust constants) so the sole
/// source of truth for "what does lopi ship by default" and "what can an
/// operator override" is the same file shape.
const DEFAULT_PRICING_TOML: &str = include_str!("../pricing.toml");

fn parse_or_warn(text: &str, source: &str) -> HashMap<String, TierRates> {
    match toml::from_str::<PriceFile>(text) {
        Ok(file) => file.tiers,
        Err(err) => {
            tracing::warn!(source, %err, "pricing file failed to parse — ignoring");
            HashMap::new()
        }
    }
}

/// Operator override locations, repo-level first: `<repo>/.lopi/pricing.toml`
/// then `~/.lopi/pricing.toml`. Either may set only the tiers it wants to
/// change — tiers it omits keep the compiled-in default.
fn override_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from(".lopi/pricing.toml")];
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(home).join(".lopi/pricing.toml"));
    }
    candidates
}

static TABLE: OnceLock<HashMap<String, TierRates>> = OnceLock::new();

/// The effective pricing table: compiled-in defaults with any operator
/// override file's tiers layered on top. Read once and cached — lopi is a
/// long-lived process, so picking up a rate change takes a restart, the
/// same as any other config file it reads at startup.
fn table() -> &'static HashMap<String, TierRates> {
    TABLE.get_or_init(|| {
        let mut rates = parse_or_warn(DEFAULT_PRICING_TOML, "bundled default pricing.toml");
        for path in override_candidates() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let overrides = parse_or_warn(&text, &path.display().to_string());
                if !overrides.is_empty() {
                    tracing::info!(path = %path.display(), tiers = ?overrides.keys().collect::<Vec<_>>(), "loaded pricing override");
                }
                rates.extend(overrides);
            }
        }
        rates
    })
}

/// Resolve the rates for `model` by the same substring match
/// `select_model`/the pre-Phase-3 `estimated_cost` already used:
/// `"opus"` → opus tier, `"haiku"` → haiku tier, else the sonnet tier
/// (lopi's default model family).
#[must_use]
pub fn rates_for(model: &str) -> TierRates {
    let tier = if model.contains("opus") {
        "opus"
    } else if model.contains("haiku") {
        "haiku"
    } else {
        "sonnet"
    };
    // The bundled default always defines all three tiers, so this only
    // falls through if an override file replaced the whole table and
    // dropped the tier being looked up — a hard-coded conservative
    // fallback here is safer than a panic on a cost-estimate path.
    table().get(tier).copied().unwrap_or(TierRates {
        input: 3.00,
        output: 15.0,
        cache_read: 0.30,
        cache_write: 3.75,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_default_covers_all_three_tiers() {
        let defaults = parse_or_warn(DEFAULT_PRICING_TOML, "test");
        for tier in ["opus", "haiku", "sonnet"] {
            assert!(defaults.contains_key(tier), "missing tier: {tier}");
        }
    }

    #[test]
    fn rates_for_matches_model_substrings() {
        let opus = rates_for("claude-opus-5");
        let haiku = rates_for("claude-haiku-4-5");
        let sonnet = rates_for("claude-sonnet-5");
        assert!(opus.input > sonnet.input, "opus should be pricier than sonnet");
        assert!(sonnet.input > haiku.input, "sonnet should be pricier than haiku");
    }

    #[test]
    fn rates_for_defaults_unknown_models_to_sonnet_tier() {
        let unknown = rates_for("some-future-model-id");
        let sonnet = rates_for("claude-sonnet-5");
        assert_eq!(unknown.input, sonnet.input);
    }

    #[test]
    fn malformed_override_file_falls_back_without_panicking() {
        let empty = parse_or_warn("not valid toml [[[", "test");
        assert!(empty.is_empty());
    }

    /// An override file — the mechanism `.lopi/pricing.toml` /
    /// `~/.lopi/pricing.toml` uses — may name only the tiers it wants to
    /// change; this is what `table()` relies on when it `.extend()`s the
    /// compiled-in defaults with a partial override map.
    #[test]
    fn override_file_may_set_only_a_subset_of_tiers() {
        let overrides = parse_or_warn("[sonnet]\ninput = 99.0\noutput = 1.0\ncache_read = 0.1\ncache_write = 0.2\n", "test override");
        assert_eq!(overrides.len(), 1);
        assert!((overrides["sonnet"].input - 99.0).abs() < f64::EPSILON);
    }
}
