//! Textual cross-agent collision detection.
//!
//! `lopi-oracle` answers one question — "do two live worktrees' current tips
//! merge cleanly?" — by shelling out to `git merge-tree --write-tree`, the
//! same read-only, non-mutating invocation timed in the Oracle-Preflight
//! sprint's KT-3 (p95 135ms on `lopi` itself; see `KILL_TEST_REGISTER.md`).
//!
//! **Textual-only, by design, not by omission.** KT-1's retrospective replay
//! caught 14/14 real historical conflicts via this exact method with zero
//! real evidence a semantic-only (non-textual) collision exists in this
//! codebase's history, despite two dedicated search passes. There is no
//! basis to front-load tree-sitter/semantic detection; revisit only once a
//! real textual-oracle miss surfaces one (a broken build after a merge
//! textual didn't flag) — a targeted reason, not a speculative one.
//!
//! **De-duplicated by construction, not as an afterthought.** KT-2 found a
//! naive poll-and-count design re-alerts on the same still-open conflict
//! every cycle — 120 red verdicts/hour off a single collision that never
//! actually changed. [`CollisionOracle`] alerts once per
//! [`ConflictSignature`] (conflicted file set + merge-base) and forgets a
//! signature once it stops appearing, so a poll only ever surfaces genuinely
//! new information: a fresh collision, or a resolved one recurring.
//!
//! **Detection only.** This crate reports; it never blocks. There is no
//! conflict-resolution logic and no hard-deny path — callers surface
//! [`Alert::advisory_text`] as additional planning-prompt context, the same
//! advisory-only pattern this codebase already uses for `allowed_dirs`/
//! `forbidden_dirs` (detect-after-the-fact, never block; see `LEDGER.md`'s
//! PF-1 entry-point audit) and for `lopi-agent`'s own reflection/pattern
//! seeding (`crates/lopi-agent/src/runner/seed.rs`).

#![warn(missing_docs)]

mod signature;
mod snapshot;
mod tracker;

pub use signature::ConflictSignature;
pub use snapshot::{snapshot_pair, Collision, WatchedRef};
pub use tracker::{Alert, CollisionOracle};
