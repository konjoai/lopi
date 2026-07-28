//! Sprint E (Finding #10) — the economics layer: predicts spend before
//! committing it, degrades in stages instead of dying, and reports unit
//! economics. See `LEDGER.md`'s Sprint E entry for why this is built fresh
//! here rather than extending `lopi_ratelimit::BudgetGovernor` (unwired
//! dead code — never call it from here).
//!
//! Split per file per the brief's constraint (`budget/{pool,estimate,
//! reserve,ladder,detect,report}.rs`):
//! - [`reserve`] — the single-writer reservation ledger (Part 2).
//! - [`pool`] — the active [`lopi_core::Pool`] + runway (Part 1/5).
//! - [`estimate`] — historical median/p90 cost estimation (Part 2).
//! - [`ladder`] — the degradation ladder + handoff writer (Part 3).
//! - [`detect`] — runaway detectors (Part 4).
//! - [`report`] — unit economics (Part 5).

pub mod detect;
pub mod estimate;
pub mod ladder;
pub mod pool;
pub mod report;
pub mod reserve;
