//! `lopi cost` — Sprint E, Part 5: the five unit-economics numbers plus
//! current tier and pool runway. Rebuilds what a Telegram `/cost` command
//! would have returned; Telegram was removed in Sprint S10 (`LEDGER.md`),
//! so this lands on the CLI (and `lopi-remote`'s WhatsApp `cost` command)
//! instead.

use anyhow::Result;
use lopi_core::LopiConfig;
use lopi_orchestrator::budget::{pool::PoolState, report};
use lopi_memory::MemoryStore;

use crate::util::db_path;

pub async fn show(config: Option<&LopiConfig>) -> Result<()> {
    let Some(pool_cfg) = config.and_then(|c| c.economics.pool.clone()) else {
        println!("💵 lopi cost\n");
        println!("  no [economics] pool configured — nothing to report");
        println!("  see `lopi rates --check` for the rate table this would price against");
        return Ok(());
    };

    let store = MemoryStore::open(db_path()).await?;
    // Seeded from the durable ledger, not a clean slate — this is a
    // one-shot CLI invocation with no access to a running `lopi sail`
    // process's live in-memory reservation ledger, so total historical
    // spend is the best available approximation of committed spend.
    let already_spent = store.total_spend_all_time().await?;
    let pool_state = PoolState::seeded(pool_cfg, lopi_core::Money::from_usd(already_spent));
    let econ = report::compute(&store, &pool_state, 7, 7).await?;

    println!("💵 lopi cost\n");
    let cost_or_dash = |m: Option<lopi_core::Money>| m.map_or_else(|| "—".to_string(), |m| m.to_string());
    println!("  cost per merged PR*:   {}", cost_or_dash(econ.cost_per_merged_pr));
    println!("  cost per gate pass:    {}", cost_or_dash(econ.cost_per_gate_pass));
    println!("  cost on retries:       {}", econ.cost_per_retry);
    println!("  cache-attributed save: {}", econ.cache_attributed_saving);
    if econ.pool_runway_days.is_finite() {
        println!("  pool runway:           {:.1} days", econ.pool_runway_days);
    } else {
        println!("  pool runway:           no burn observed yet");
    }
    println!("  headroom:              {}", pool_state.headroom().await);
    println!();
    println!("  * lopi tracks task completion, not GitHub merge state — this is");
    println!("    \"cost per completed task,\" the closest available proxy.");

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {}
}
