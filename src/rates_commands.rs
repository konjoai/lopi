//! `lopi rates --check` — Sprint E, Part 1: print what lopi believes the
//! current per-token prices and pool terms are, with the date they were
//! last set, so a stale rate table is visible instead of silently wrong.

use chrono::Utc;
use lopi_agent::pricing::{self, RatesReport};
use lopi_core::{LopiConfig, Pool};

/// Pure formatter — separated from `show()` so the staleness/pool-shape
/// branches are directly testable against a hand-built `RatesReport`
/// instead of needing to fake wall-clock time.
#[must_use]
pub fn render(report: &RatesReport, pool: Option<&Pool>) -> String {
    let mut out = String::from("💵 lopi rate table\n\n");
    match report.last_updated {
        Some(d) if report.stale => {
            out.push_str(&format!(
                "  ⚠️  rates last set {d} — older than {} days, treat every cost on this table as an ESTIMATE and verify against current Anthropic pricing\n",
                pricing::DEFAULT_MAX_AGE_DAYS
            ));
        }
        Some(d) => out.push_str(&format!("  rates last set: {d}\n")),
        None => out.push_str(
            "  ⚠️  no [meta] last_updated found — treat every cost on this table as an ESTIMATE\n",
        ),
    }
    out.push('\n');
    out.push_str(&format!(
        "  {:<8} {:>10} {:>10} {:>10} {:>10}\n",
        "tier", "input", "output", "cache_rd", "cache_wr"
    ));
    for (tier, rates) in &report.tiers {
        out.push_str(&format!(
            "  {tier:<8} {:>9.2}/M {:>9.2}/M {:>9.2}/M {:>9.2}/M\n",
            rates.input, rates.output, rates.cache_read, rates.cache_write
        ));
    }

    out.push('\n');
    match pool {
        None => {
            out.push_str("  no [economics] pool configured — the economics layer (reservations, degradation ladder, runaway detection) is inactive\n");
        }
        Some(Pool::AgentSdkCredits {
            monthly_allotment,
            resets_on,
        }) => {
            out.push_str("  pool: agent_sdk_credits\n");
            out.push_str(&format!("    monthly allotment: {monthly_allotment}\n"));
            out.push_str(&format!("    resets on:         {resets_on}\n"));
        }
        Some(Pool::ApiKey {
            hard_ceiling,
            period,
        }) => {
            out.push_str("  pool: api_key\n");
            out.push_str(&format!("    hard ceiling: {hard_ceiling} / {period:?}\n"));
        }
        Some(Pool::ExtraUsage { remaining }) => {
            out.push_str("  pool: extra_usage\n");
            out.push_str(&format!("    remaining: {remaining}\n"));
        }
    }
    out
}

/// Compute the live rate table + configured pool report text. Returns the
/// text (rather than printing it directly) so its content — not just that
/// it compiles — is what a test observes; `main.rs` prints the result.
#[must_use]
pub fn show(config: Option<&LopiConfig>) -> String {
    let today = Utc::now().date_naive();
    let report = pricing::describe(today, pricing::DEFAULT_MAX_AGE_DAYS);
    let pool = config.and_then(|c| c.economics.pool.as_ref());
    render(&report, pool)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use lopi_agent::pricing::TierRates;
    use lopi_core::{Money, Period};

    fn sample_tiers() -> Vec<(String, TierRates)> {
        vec![(
            "sonnet".to_string(),
            TierRates {
                input: 3.0,
                output: 15.0,
                cache_read: 0.3,
                cache_write: 3.75,
            },
        )]
    }

    #[test]
    fn render_flags_a_stale_table_with_a_warning() {
        let report = RatesReport {
            tiers: sample_tiers(),
            last_updated: NaiveDate::from_ymd_opt(2020, 1, 1),
            stale: true,
        };
        let out = render(&report, None);
        assert!(out.contains("older than"));
        assert!(out.contains("ESTIMATE"));
    }

    #[test]
    fn render_does_not_warn_when_the_table_is_fresh() {
        let report = RatesReport {
            tiers: sample_tiers(),
            last_updated: NaiveDate::from_ymd_opt(2026, 7, 1),
            stale: false,
        };
        let out = render(&report, None);
        assert!(out.contains("rates last set: 2026-07-01"));
        assert!(!out.contains("older than"));
    }

    #[test]
    fn render_warns_when_no_last_updated_is_present_at_all() {
        let report = RatesReport {
            tiers: sample_tiers(),
            last_updated: None,
            stale: true,
        };
        let out = render(&report, None);
        assert!(out.contains("no [meta] last_updated found"));
    }

    #[test]
    fn render_shows_no_pool_configured_when_none_is_set() {
        let report = RatesReport {
            tiers: sample_tiers(),
            last_updated: NaiveDate::from_ymd_opt(2026, 7, 1),
            stale: false,
        };
        let out = render(&report, None);
        assert!(out.contains("no [economics] pool configured"));
    }

    #[test]
    fn render_shows_configured_agent_sdk_credits_pool() {
        let report = RatesReport {
            tiers: sample_tiers(),
            last_updated: NaiveDate::from_ymd_opt(2026, 7, 1),
            stale: false,
        };
        let pool = Pool::AgentSdkCredits {
            monthly_allotment: Money::from_usd(100.0),
            resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        };
        let out = render(&report, Some(&pool));
        assert!(out.contains("pool: agent_sdk_credits"));
        assert!(out.contains("$100.0000"));
        assert!(out.contains("2026-08-01"));
    }

    #[test]
    fn render_shows_configured_api_key_pool() {
        let report = RatesReport {
            tiers: sample_tiers(),
            last_updated: NaiveDate::from_ymd_opt(2026, 7, 1),
            stale: false,
        };
        let pool = Pool::ApiKey {
            hard_ceiling: Money::from_usd(50.0),
            period: Period::Daily,
        };
        let out = render(&report, Some(&pool));
        assert!(out.contains("pool: api_key"));
        assert!(out.contains("Daily"));
    }

    #[test]
    fn render_shows_configured_extra_usage_pool() {
        let report = RatesReport {
            tiers: sample_tiers(),
            last_updated: NaiveDate::from_ymd_opt(2026, 7, 1),
            stale: false,
        };
        let pool = Pool::ExtraUsage {
            remaining: Money::from_usd(12.5),
        };
        let out = render(&report, Some(&pool));
        assert!(out.contains("pool: extra_usage"));
        assert!(out.contains("$12.5000"));
    }

    #[test]
    fn render_lists_every_tier_row() {
        let report = RatesReport {
            tiers: sample_tiers(),
            last_updated: NaiveDate::from_ymd_opt(2026, 7, 1),
            stale: false,
        };
        let out = render(&report, None);
        assert!(out.contains("sonnet"));
        assert!(out.contains("3.00/M"));
        assert!(out.contains("15.00/M"));
    }

    #[test]
    fn show_returns_the_real_rendered_report() {
        // Exercises the real wall-clock/config path end to end and asserts
        // on the actual returned content, not just that it compiles/runs.
        let out = show(None);
        assert!(out.starts_with("💵 lopi rate table"));
        assert!(out.contains("no [economics] pool configured"));
    }
}
