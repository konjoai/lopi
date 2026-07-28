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

/// A pricing file's shape: an `as_of` freshness date plus a flat map of
/// tier name to rates, matching the `[opus]`/`[haiku]`/`[sonnet]` tables in
/// `pricing.toml`.
#[derive(Debug, Deserialize)]
struct PriceFile {
    /// ISO-8601 date (`YYYY-MM-DD`) this table's rates were last verified
    /// current. Absent (e.g. an older or partial operator override file)
    /// is treated as "unknown freshness" — see `table_as_of`.
    as_of: Option<chrono::NaiveDate>,
    #[serde(flatten)]
    tiers: HashMap<String, TierRates>,
}

/// Compiled-in default rates, always available even with no override file
/// on disk. Kept in a real TOML file (not Rust constants) so the sole
/// source of truth for "what does lopi ship by default" and "what can an
/// operator override" is the same file shape.
const DEFAULT_PRICING_TOML: &str = include_str!("../pricing.toml");

/// Parse a pricing TOML string into its tier-rate map and `as_of` date,
/// warning (and falling back to an empty map / unknown date) on a parse
/// failure rather than panicking — a malformed operator override file
/// shouldn't take down a cost-estimate path. Tests exercise this directly
/// (via the `parse_or_warn` test helper below) rather than going through
/// the process-global `OnceLock` in [`table`].
fn parse_price_file(
    text: &str,
    source: &str,
) -> (HashMap<String, TierRates>, Option<chrono::NaiveDate>) {
    match toml::from_str::<PriceFile>(text) {
        Ok(file) => (file.tiers, file.as_of),
        Err(err) => {
            tracing::warn!(source, %err, "pricing file failed to parse — ignoring");
            (HashMap::new(), None)
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

static TABLE: OnceLock<(HashMap<String, TierRates>, Option<chrono::NaiveDate>)> = OnceLock::new();

/// The effective pricing table: compiled-in defaults with any operator
/// override file's tiers layered on top. Read once and cached — lopi is a
/// long-lived process, so picking up a rate change takes a restart, the
/// same as any other config file it reads at startup.
///
/// The effective `as_of` follows the same "override replaces what it sets"
/// rule as the tier rates: the bundled default's `as_of` is used unless an
/// override file also sets its own `as_of`, in which case the override's
/// date wins outright (it isn't merged field-by-field — an override either
/// asserts a fresher date for the whole table or it doesn't).
fn table() -> &'static (HashMap<String, TierRates>, Option<chrono::NaiveDate>) {
    TABLE.get_or_init(|| {
        let (mut rates, mut as_of) =
            parse_price_file(DEFAULT_PRICING_TOML, "bundled default pricing.toml");
        for path in override_candidates() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let (overrides, override_as_of) =
                    parse_price_file(&text, &path.display().to_string());
                if !overrides.is_empty() {
                    tracing::info!(path = %path.display(), tiers = ?overrides.keys().collect::<Vec<_>>(), "loaded pricing override");
                }
                rates.extend(overrides);
                if let Some(date) = override_as_of {
                    as_of = Some(date);
                }
            }
        }
        (rates, as_of)
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
    table().0.get(tier).copied().unwrap_or(TierRates {
        input: 3.00,
        output: 15.0,
        cache_read: 0.30,
        cache_write: 3.75,
    })
}

/// How stale a pricing estimate's basis is allowed to be before lopi
/// degrades the estimate to an explicit warning instead of a confident
/// figure. 90 days — long enough that a routine rate check doesn't nag,
/// short enough that a table nobody has looked at in a quarter doesn't
/// silently keep pricing sessions.
pub const STALENESS_THRESHOLD_DAYS: i64 = 90;

/// The pricing table's effective `as_of` date, if known (a hand-authored
/// bundled/override TOML predating this field entirely has no date to
/// report).
#[must_use]
pub fn table_as_of() -> Option<chrono::NaiveDate> {
    table().1
}

/// Whether the pricing table is older than [`STALENESS_THRESHOLD_DAYS`] as
/// of `today` — or has no known `as_of` at all (treated as stale, since an
/// unknown freshness date is the more conservative assumption for a
/// confident-looking dollar figure). Takes `today` as a parameter rather
/// than calling `chrono::Utc::now()` internally so it's unit-testable
/// without a wall-clock dependency.
#[must_use]
pub fn is_stale(today: chrono::NaiveDate) -> bool {
    is_stale_given(table_as_of(), today)
}

/// A one-line warning suitable for a CLI/TUI status line when [`is_stale`]
/// is true, naming the table's age or its absent `as_of`. `None` when the
/// table isn't stale as of `today`.
#[must_use]
pub fn staleness_warning(today: chrono::NaiveDate) -> Option<String> {
    staleness_warning_given(table_as_of(), today)
}

/// Pure staleness check taking an explicit `as_of` rather than reading the
/// process-global [`table`], so tests can exercise both the "known old
/// date" and "no `as_of` at all" cases without needing a second
/// `OnceLock`-backed table per test binary. [`is_stale`] is a thin wrapper
/// over this using the real table's `as_of`.
fn is_stale_given(as_of: Option<chrono::NaiveDate>, today: chrono::NaiveDate) -> bool {
    match as_of {
        None => true,
        Some(date) => today.signed_duration_since(date).num_days() > STALENESS_THRESHOLD_DAYS,
    }
}

/// Pure warning-message builder taking an explicit `as_of`; see
/// [`is_stale_given`] for why this is split out from [`staleness_warning`].
fn staleness_warning_given(
    as_of: Option<chrono::NaiveDate>,
    today: chrono::NaiveDate,
) -> Option<String> {
    if !is_stale_given(as_of, today) {
        return None;
    }
    Some(match as_of {
        Some(date) => {
            let days = today.signed_duration_since(date).num_days();
            format!(
                "pricing table is stale: {days} days old (as of {date}) — dollar estimates may be inaccurate"
            )
        }
        None => {
            "pricing table is stale: no as_of date recorded — dollar estimates may be inaccurate"
                .to_string()
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Test-only convenience wrapper matching the pre-`as_of` call shape
    /// used throughout this module's tests: parse a pricing TOML string and
    /// return just its tier-rate map, discarding `as_of`.
    fn parse_or_warn(text: &str, source: &str) -> HashMap<String, TierRates> {
        parse_price_file(text, source).0
    }

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
        assert!(empty.is_empty());
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
        assert_eq!(overrides.len(), 1);
        assert!((overrides["sonnet"].input - 99.0).abs() < f64::EPSILON);
    }

    /// The bundled `pricing.toml`'s `as_of` should parse and match the date
    /// recorded in the file's own prose comment ("2026-07 pricing").
    #[test]
    fn bundled_default_has_as_of() {
        let (_, as_of) = parse_price_file(DEFAULT_PRICING_TOML, "test");
        assert_eq!(
            as_of,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap())
        );
    }

    #[test]
    fn is_stale_given_false_within_threshold_true_past_it() {
        let as_of = chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let just_inside = as_of + chrono::Duration::days(STALENESS_THRESHOLD_DAYS);
        let just_outside = as_of + chrono::Duration::days(STALENESS_THRESHOLD_DAYS + 1);
        assert!(!is_stale_given(Some(as_of), just_inside));
        assert!(is_stale_given(Some(as_of), just_outside));
    }

    /// A hand-constructed override-shaped TOML string with no `as_of` key
    /// parses without error (backward compat with a pre-`as_of` override
    /// file) and its missing date is treated as the conservative "stale"
    /// case rather than as an error.
    #[test]
    fn missing_as_of_parses_fine_and_is_treated_as_stale() {
        let (tiers, as_of) = parse_price_file(
            "[sonnet]\ninput = 1.0\noutput = 2.0\ncache_read = 0.1\ncache_write = 0.2\n",
            "test",
        );
        assert_eq!(tiers.len(), 1);
        assert_eq!(as_of, None);

        let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        assert!(is_stale_given(as_of, today));
    }

    #[test]
    fn staleness_warning_none_when_fresh_some_and_mentions_stale_when_old() {
        let as_of = chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let fresh = as_of + chrono::Duration::days(1);
        assert_eq!(staleness_warning_given(Some(as_of), fresh), None);

        let old = as_of + chrono::Duration::days(STALENESS_THRESHOLD_DAYS + 1);
        let warning = staleness_warning_given(Some(as_of), old).expect("should warn when stale");
        assert!(warning.to_lowercase().contains("stale"));

        let unknown_warning =
            staleness_warning_given(None, old).expect("missing as_of should also warn");
        assert!(unknown_warning.to_lowercase().contains("stale"));
    }
}
