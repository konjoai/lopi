//! `lopi rates --check` — Sprint E, Part 1: print what lopi believes the
//! current per-token prices and pool terms are, with the date they were
//! last set, so a stale rate table is visible instead of silently wrong.

use chrono::Utc;
use lopi_agent::pricing;
use lopi_core::{LopiConfig, Pool};

pub fn show(config: Option<&LopiConfig>) {
    let today = Utc::now().date_naive();
    let report = pricing::describe(today, pricing::DEFAULT_MAX_AGE_DAYS);

    println!("💵 lopi rate table\n");
    match report.last_updated {
        Some(d) if report.stale => {
            println!("  ⚠️  rates last set {d} — older than {} days, treat every cost on this table as an ESTIMATE and verify against current Anthropic pricing", pricing::DEFAULT_MAX_AGE_DAYS);
        }
        Some(d) => println!("  rates last set: {d}"),
        None => println!(
            "  ⚠️  no [meta] last_updated found — treat every cost on this table as an ESTIMATE"
        ),
    }
    println!();
    println!("  {:<8} {:>10} {:>10} {:>10} {:>10}", "tier", "input", "output", "cache_rd", "cache_wr");
    for (tier, rates) in &report.tiers {
        println!(
            "  {tier:<8} {:>9.2}/M {:>9.2}/M {:>9.2}/M {:>9.2}/M",
            rates.input, rates.output, rates.cache_read, rates.cache_write
        );
    }

    println!();
    match config.and_then(|c| c.economics.pool.as_ref()) {
        None => {
            println!("  no [economics] pool configured — the economics layer (reservations, degradation ladder, runaway detection) is inactive");
        }
        Some(Pool::AgentSdkCredits {
            monthly_allotment,
            resets_on,
        }) => {
            println!("  pool: agent_sdk_credits");
            println!("    monthly allotment: {monthly_allotment}");
            println!("    resets on:         {resets_on}");
        }
        Some(Pool::ApiKey {
            hard_ceiling,
            period,
        }) => {
            println!("  pool: api_key");
            println!("    hard ceiling: {hard_ceiling} / {period:?}");
        }
        Some(Pool::ExtraUsage { remaining }) => {
            println!("  pool: extra_usage");
            println!("    remaining: {remaining}");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {}
}
