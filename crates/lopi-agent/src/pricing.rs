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

/// A pricing file's shape: an optional `[meta]` table plus a flat map of
/// tier name to rates, matching the `[opus]`/`[haiku]`/`[sonnet]` tables in
/// `pricing.toml`. `meta` is a named field (not part of the flattened map),
/// so it never gets mistaken for a fourth pricing tier.
#[derive(Debug, Deserialize)]
struct PriceFile {
    #[serde(default)]
    meta: Option<PriceMeta>,
    #[serde(flatten)]
    tiers: HashMap<String, TierRates>,
}

/// `[meta]` table — Sprint E, Part 1: "provide a `lopi rates --check`
/// command that prints what lopi believes the current per-token prices...
/// are, with the date they were last set, so a stale rate table is visible
/// rather than silently wrong."
#[derive(Debug, Clone, Copy, Deserialize)]
struct PriceMeta {
    last_updated: chrono::NaiveDate,
}

/// Compiled-in default rates, always available even with no override file
/// on disk. Kept in a real TOML file (not Rust constants) so the sole
/// source of truth for "what does lopi ship by default" and "what can an
/// operator override" is the same file shape.
const DEFAULT_PRICING_TOML: &str = include_str!("../pricing.toml");

fn parse_or_warn(text: &str, source: &str) -> PriceFile {
    match toml::from_str::<PriceFile>(text) {
        Ok(file) => file,
        Err(err) => {
            tracing::warn!(source, %err, "pricing file failed to parse — ignoring");
            PriceFile {
                meta: None,
                tiers: HashMap::new(),
            }
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

/// The effective, cached table: tiers plus the `last_updated` date of
/// whichever file last set it (an override's own `[meta]` wins over the
/// bundled default's, since the operator presumably set it when they
/// edited their override).
struct EffectiveTable {
    tiers: HashMap<String, TierRates>,
    last_updated: Option<chrono::NaiveDate>,
}

static TABLE: OnceLock<EffectiveTable> = OnceLock::new();

/// The effective pricing table: compiled-in defaults with any operator
/// override file's tiers layered on top. Read once and cached — lopi is a
/// long-lived process, so picking up a rate change takes a restart, the
/// same as any other config file it reads at startup.
fn table() -> &'static EffectiveTable {
    TABLE.get_or_init(|| {
        let base = parse_or_warn(DEFAULT_PRICING_TOML, "bundled default pricing.toml");
        let mut rates = base.tiers;
        let mut last_updated = base.meta.map(|m| m.last_updated);
        for path in override_candidates() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let overrides = parse_or_warn(&text, &path.display().to_string());
                if !overrides.tiers.is_empty() {
                    tracing::info!(path = %path.display(), tiers = ?overrides.tiers.keys().collect::<Vec<_>>(), "loaded pricing override");
                }
                rates.extend(overrides.tiers);
                if let Some(meta) = overrides.meta {
                    last_updated = Some(meta.last_updated);
                }
            }
        }
        EffectiveTable { tiers: rates, last_updated }
    })
}

/// Sprint E, Part 1 — `lopi rates --check`'s payload: the resolved rate for
/// every tier, when the table backing them was last set, and whether that
/// date has aged past `max_age_days` (a stale table should be *visible*,
/// never a silent wrong number).
#[derive(Debug, Clone)]
pub struct RatesReport {
    /// Resolved rates for `opus`/`haiku`/`sonnet`, in stable sorted order.
    pub tiers: Vec<(String, TierRates)>,
    /// The date whichever file set the active table's rates was last
    /// edited. `None` if neither the bundled default nor any override
    /// declared a `[meta]` table (a pre-Sprint-E override file, e.g.).
    pub last_updated: Option<chrono::NaiveDate>,
    /// `true` once `last_updated` is more than `max_age_days` in the past,
    /// or entirely absent — both cases mean "don't trust this number
    /// without checking," which is exactly what a stale/undated table is.
    pub stale: bool,
}

/// Default staleness horizon — 90 days. Anthropic pricing has moved on a
/// timescale of single-digit months historically (see `pricing.toml`'s own
/// doc comment on the Opus 4.1 staleness incident), so a table older than a
/// quarter is treated as needing a human look, not silently trusted.
pub const DEFAULT_MAX_AGE_DAYS: i64 = 90;

/// Build a [`RatesReport`] for `lopi rates --check` (or any other cost
/// surface that wants to label its numbers estimate-vs-stale). Pure over
/// the cached [`table`] — takes `today` as a parameter rather than calling
/// `Utc::now()` internally so it's unit-testable without wall-clock
/// dependence.
#[must_use]
pub fn describe(today: chrono::NaiveDate, max_age_days: i64) -> RatesReport {
    let t = table();
    let mut tiers: Vec<(String, TierRates)> =
        t.tiers.iter().map(|(k, v)| (k.clone(), *v)).collect();
    tiers.sort_by(|a, b| a.0.cmp(&b.0));
    let stale = match t.last_updated {
        Some(d) => (today - d).num_days() > max_age_days,
        None => true,
    };
    RatesReport {
        tiers,
        last_updated: t.last_updated,
        stale,
    }
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
    table().tiers.get(tier).copied().unwrap_or(TierRates {
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
            assert!(defaults.tiers.contains_key(tier), "missing tier: {tier}");
        }
    }

    #[test]
    fn bundled_default_declares_last_updated() {
        let defaults = parse_or_warn(DEFAULT_PRICING_TOML, "test");
        assert!(
            defaults.meta.is_some(),
            "shipped pricing.toml must declare [meta] last_updated"
        );
    }

    #[test]
    fn rates_for_matches_model_substrings() {
        let opus = rates_for("claude-opus-5");
        let haiku = rates_for("claude-haiku-4-5");
        let sonnet = rates_for("claude-sonnet-5");
        assert!(
            opus.input > sonnet.input,
            "opus should be pricier than sonnet"
        );
        assert!(
            sonnet.input > haiku.input,
            "sonnet should be pricier than haiku"
        );
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
        assert!(empty.tiers.is_empty());
        assert!(empty.meta.is_none());
    }

    /// An override file — the mechanism `.lopi/pricing.toml` /
    /// `~/.lopi/pricing.toml` uses — may name only the tiers it wants to
    /// change; this is what `table()` relies on when it `.extend()`s the
    /// compiled-in defaults with a partial override map.
    #[test]
    fn override_file_may_set_only_a_subset_of_tiers() {
        let overrides = parse_or_warn(
            "[sonnet]\ninput = 99.0\noutput = 1.0\ncache_read = 0.1\ncache_write = 0.2\n",
            "test override",
        );
        assert_eq!(overrides.tiers.len(), 1);
        assert!((overrides.tiers["sonnet"].input - 99.0).abs() < f64::EPSILON);
    }

    #[test]
    fn override_file_meta_is_parsed_independently_of_tiers() {
        let file = parse_or_warn(
            "[meta]\nlast_updated = \"2026-01-15\"\n[sonnet]\ninput = 1.0\noutput = 1.0\ncache_read = 1.0\ncache_write = 1.0\n",
            "test override",
        );
        assert_eq!(
            file.meta.map(|m| m.last_updated),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 15)
        );
        // `meta` must not leak into the flattened tier map as a bogus tier.
        assert!(!file.tiers.contains_key("meta"));
    }

    #[test]
    fn describe_reports_shipped_last_updated_and_is_not_stale_relative_to_itself() {
        let report = describe(
            chrono::NaiveDate::from_ymd_opt(2026, 7, 2).expect("valid date"),
            DEFAULT_MAX_AGE_DAYS,
        );
        assert_eq!(
            report.last_updated,
            chrono::NaiveDate::from_ymd_opt(2026, 7, 1)
        );
        assert!(!report.stale, "one day old must not be flagged stale");
        assert_eq!(report.tiers.len(), 3);
        // Sorted order, per the doc comment on `RatesReport::tiers`.
        let names: Vec<&str> = report.tiers.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, vec!["haiku", "opus", "sonnet"]);
    }

    #[test]
    fn describe_flags_a_table_older_than_max_age_as_stale() {
        let far_future = chrono::NaiveDate::from_ymd_opt(2030, 1, 1).expect("valid date");
        let report = describe(far_future, DEFAULT_MAX_AGE_DAYS);
        assert!(report.stale, "a multi-year-old table must be flagged stale");
    }
}
