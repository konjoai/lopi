//! `lopi cost` — Sprint E, Part 5: the five unit-economics numbers plus
//! current tier and pool runway. Rebuilds what a Telegram `/cost` command
//! would have returned; Telegram was removed in Sprint S10 (`LEDGER.md`),
//! so this lands on the CLI (and `lopi-remote`'s WhatsApp `cost` command)
//! instead.

use anyhow::Result;
use lopi_core::{LopiConfig, Money};
use lopi_memory::MemoryStore;
use lopi_orchestrator::budget::report::UnitEconomics;
use lopi_orchestrator::budget::{pool::PoolState, report};

use crate::util::db_path;

/// Message shown when `[economics]` has no pool configured.
#[must_use]
pub fn render_not_configured() -> String {
    "💵 lopi cost\n\n  no [economics] pool configured — nothing to report\n  see `lopi rates --check` for the rate table this would price against\n".to_string()
}

/// Pure formatter for the five unit-economics numbers — separated from
/// `show()` so the exact text is directly assertable without a live store.
#[must_use]
pub fn render(econ: &UnitEconomics, headroom: Money) -> String {
    let cost_or_dash = |m: Option<Money>| m.map_or_else(|| "—".to_string(), |m| m.to_string());
    let mut out = String::from("💵 lopi cost\n\n");
    out.push_str(&format!(
        "  cost per merged PR*:   {}\n",
        cost_or_dash(econ.cost_per_merged_pr)
    ));
    out.push_str(&format!(
        "  cost per gate pass:    {}\n",
        cost_or_dash(econ.cost_per_gate_pass)
    ));
    out.push_str(&format!(
        "  cost on retries:       {}\n",
        econ.cost_per_retry
    ));
    out.push_str(&format!(
        "  cache-attributed save: {}\n",
        econ.cache_attributed_saving
    ));
    if econ.pool_runway_days.is_finite() {
        out.push_str(&format!(
            "  pool runway:           {:.1} days\n",
            econ.pool_runway_days
        ));
    } else {
        out.push_str("  pool runway:           no burn observed yet\n");
    }
    out.push_str(&format!("  headroom:              {headroom}\n"));
    out.push('\n');
    out.push_str("  * lopi tracks task completion, not GitHub merge state — this is\n");
    out.push_str("    \"cost per completed task,\" the closest available proxy.\n");
    out
}

/// Compute the cost report text (rather than printing it directly), so its
/// content — not just that it compiles — is what a test observes; the CLI
/// entry point prints the result.
///
/// # Errors
/// Returns `Err` if the store can't be opened or a report query fails.
pub async fn show(config: Option<&LopiConfig>) -> Result<String> {
    let Some(pool_cfg) = config.and_then(|c| c.economics.pool.clone()) else {
        return Ok(render_not_configured());
    };

    let store = MemoryStore::open(db_path()).await?;
    // Seeded from the durable ledger, not a clean slate — this is a
    // one-shot CLI invocation with no access to a running `lopi sail`
    // process's live in-memory reservation ledger, so total historical
    // spend is the best available approximation of committed spend.
    let already_spent = store.total_spend_all_time().await?;
    let pool_state = PoolState::seeded(pool_cfg, Money::from_usd(already_spent));
    let econ = report::compute(&store, &pool_state, 7, 7).await?;
    let headroom = pool_state.headroom().await;

    Ok(render(&econ, headroom))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_econ() -> UnitEconomics {
        UnitEconomics {
            cost_per_merged_pr: Some(Money::from_usd(6.0)),
            cost_per_gate_pass: Some(Money::from_usd(3.5)),
            cost_per_retry: Money::from_usd(1.2),
            cache_attributed_saving: Money::from_usd(0.8),
            pool_runway_days: 42.5,
        }
    }

    #[test]
    fn render_not_configured_names_the_reason_and_the_alternative_command() {
        let out = render_not_configured();
        assert!(out.contains("no [economics] pool configured"));
        assert!(out.contains("lopi rates --check"));
    }

    #[test]
    fn render_prints_all_five_numbers_and_headroom() {
        let out = render(&sample_econ(), Money::from_usd(88.0));
        assert!(out.contains("$6.0000"));
        assert!(out.contains("$3.5000"));
        assert!(out.contains("$1.2000"));
        assert!(out.contains("$0.8000"));
        assert!(out.contains("42.5 days"));
        assert!(out.contains("$88.0000"));
    }

    #[test]
    fn render_shows_a_dash_when_no_merged_pr_or_gate_pass_data_exists() {
        let econ = UnitEconomics {
            cost_per_merged_pr: None,
            cost_per_gate_pass: None,
            ..sample_econ()
        };
        let out = render(&econ, Money::ZERO);
        assert!(out.contains("cost per merged PR*:   —"));
        assert!(out.contains("cost per gate pass:    —"));
    }

    #[test]
    fn render_shows_no_burn_observed_when_runway_is_infinite() {
        let econ = UnitEconomics {
            pool_runway_days: f64::INFINITY,
            ..sample_econ()
        };
        let out = render(&econ, Money::ZERO);
        assert!(out.contains("no burn observed yet"));
        assert!(!out.contains("days"));
    }

    #[tokio::test]
    async fn show_returns_the_not_configured_message_when_no_pool_is_set() {
        // No config -> no pool -> the early-return branch, exercised
        // end-to-end (no store I/O reachable on this path) and asserted on
        // the actual returned content.
        let out = show(None).await.unwrap();
        assert!(out.contains("no [economics] pool configured"));
    }
}
