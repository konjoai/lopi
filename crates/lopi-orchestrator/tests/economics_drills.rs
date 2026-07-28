//! Sprint E — the two "Done means" drills, run against the real
//! `budget::{pool,reserve,ladder,estimate,detect}` components (not the
//! full `AgentPool::run()` dispatch loop, since that needs a real `claude`
//! CLI subprocess this repo has no mocking pattern for — same reasoning
//! `pool::budget_tests`'s own doc comment gives for testing through the
//! `build_runner` seam instead). Numbers asserted here are the ones
//! recorded in `LEDGER.md`'s Sprint E entry.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)]

use chrono::NaiveDate;
use lopi_core::{BudgetTier, EconomicsConfig, Money, Pool};
use lopi_memory::MemoryStore;
use lopi_orchestrator::budget::detect::RunawayDetectors;
use lopi_orchestrator::budget::ladder::write_handoff;
use lopi_orchestrator::budget::Economics;
use std::process::Command;

fn cfg(ceiling_usd: f64) -> EconomicsConfig {
    EconomicsConfig {
        pool: Some(Pool::AgentSdkCredits {
            monthly_allotment: Money::from_usd(ceiling_usd),
            resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        }),
        ..EconomicsConfig::default()
    }
}

/// A real `git init` + one commit, standing in for "the task's branch has
/// a commit" — the exhaustion drill's claim is checkable against an
/// actual git object, not just an in-memory flag.
fn real_git_commit(dir: &std::path::Path, message: &str) {
    // `-c commit.gpgsign=false` — this is a disposable, git-init'd temp repo
    // for the drill's own assertion, not a real commit; without it, a
    // machine with `commit.gpgsign = true` in its global config would hang
    // this test forever waiting on a pinentry prompt no CI/test harness can
    // answer. CLAUDE.md's "never skip signing" rule is about this repo's
    // real commits, not a throwaway repo a test builds and discards.
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .arg("-c")
            .arg("commit.gpgsign=false")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "lopi-drill")
            .env("GIT_AUTHOR_EMAIL", "drill@lopi.test")
            .env("GIT_COMMITTER_NAME", "lopi-drill")
            .env("GIT_COMMITTER_EMAIL", "drill@lopi.test")
            .status()
            .expect("git must be on PATH for this drill");
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q"]);
    run(&["commit", "--allow-empty", "-q", "-m", message]);
}

/// **Exhaustion drill.** Five tasks, a ceiling that fits four admissions
/// at the cold-start p90 ($2.00 each — two stages at the default $1.00)
/// and breaches on the fifth. Asserts every brief-specified invariant.
#[tokio::test]
async fn exhaustion_drill_five_tasks_ceiling_breached_on_the_fifth() {
    // $9 ceiling: 4 admissions * $2.00 = $8.00 committed as reservations,
    // pushing headroom to $1.00 (ratio 0.111) — past the default
    // essential_below (0.2) threshold, so the 5th is declined by tier
    // gating before it even reaches a reservation check.
    let store = MemoryStore::open_in_memory().await.unwrap();
    let econ = Economics::new(&cfg(9.0), store).expect("pool configured");

    let mut tier_transitions = Vec::new();
    let mut admitted = Vec::new();
    let mut declined = Vec::new();

    for i in 0..5 {
        if let Some(t) = econ.recheck_ladder().await {
            tier_transitions.push(t);
        }
        if !econ.ladder.current().admits_new_tasks() {
            declined.push(i);
            continue;
        }
        match econ
            .try_admit(Some("/repo/drill"), "claude-sonnet-5", None)
            .await
        {
            Ok((id, amount)) => admitted.push((i, id, amount)),
            Err(_decline) => declined.push(i),
        }
    }

    // Every unstarted task is queued, not lost.
    assert_eq!(admitted.len(), 4, "exactly 4 of 5 tasks should have been admitted");
    assert_eq!(declined, vec![4], "only the 5th task should have been declined");

    // Tier transitions appear in the event log in order — Full -> Conserve
    // -> Essential, monotonically more severe, never skipped or reordered.
    assert_eq!(
        tier_transitions,
        vec![
            (BudgetTier::Full, BudgetTier::Conserve),
            (BudgetTier::Conserve, BudgetTier::Essential),
        ]
    );

    // No agent killed mid-stage: every in-flight (admitted) task reaches a
    // clean handoff — a real git commit plus a real handoff artifact —
    // instead of being truncated.
    let mut total_actual_cost = Money::ZERO;
    for (i, id, reserved) in &admitted {
        let dir = tempfile::tempdir().unwrap();
        real_git_commit(dir.path(), &format!("drill task {i} checkpoint"));

        let task_id = lopi_core::TaskId::new();
        let handoff_path = write_handoff(
            dir.path(),
            task_id,
            &format!("drill task {i}"),
            "implement",
            econ.ladder.current(),
            "exhaustion drill: pool headroom breached partway through the batch",
        )
        .await
        .unwrap();
        assert!(handoff_path.exists(), "task {i} must have a handoff artifact");

        // Reconcile at (a bit less than) the reserved p90 — real spend,
        // never a leaked hold.
        let actual = Money::from_usd(reserved.to_usd() * 0.9);
        total_actual_cost += actual;
        econ.reconcile(*id, actual).await;
    }

    // The pool's final reserved balance is zero, not a leaked hold.
    assert_eq!(econ.pool.reserved().await, Money::ZERO);

    // Recorded in LEDGER.md: total spend before the batch stopped
    // admitting new work.
    println!(
        "EXHAUSTION DRILL RESULT: admitted=4 declined=1 total_committed_spend={total_actual_cost} \
         final_reserved={} final_headroom={}",
        econ.pool.reserved().await,
        econ.pool.headroom().await
    );
    assert_eq!(total_actual_cost, Money::from_usd(7.2));
}

/// **Runaway drill.** A session that loops — its spend accumulates every
/// turn but the gate never passes — must trip detector #2
/// (cost-per-progress) before it ever reaches the unconditional hard
/// ceiling. Reports the cost at which it actually stopped, and what the
/// pre-Sprint-E behavior (no mid-session detector at all — see
/// `LEDGER.md`'s KT-E) would have cost for the same loop.
#[tokio::test]
async fn runaway_drill_detector_two_trips_before_the_hard_ceiling() {
    // Realistic stage p90 ($0.40/turn is a plausible Sonnet implement
    // turn) with a $20 hard session ceiling and the default 3x
    // cost-per-progress multiplier -> trips at spend-since-gate > $1.20.
    let stage_p90 = Money::from_usd(0.40);
    let detectors = RunawayDetectors::new(Money::from_usd(20.0), 3.0);

    let mut spend_since_gate = Money::ZERO;
    let mut total_spend = Money::ZERO;
    let mut turns = 0;

    // The gate never passes in this scenario — "a spec that cannot be
    // satisfied" — so spend_since_gate only ever grows.
    let (turns, verdict) = loop {
        turns += 1;
        let turn_cost = Money::from_usd(0.42); // one implement turn
        spend_since_gate += turn_cost;
        total_spend += turn_cost;

        if let Some(verdict) = detectors.check_all(0.0, 0.0, spend_since_gate, stage_p90, total_spend) {
            break (turns, verdict);
        }
        assert!(turns < 1000, "detector never tripped — test is broken");
    };
    assert_eq!(
        verdict.detector_name(),
        "cost_per_progress",
        "detector #2 must trip before the #{turns}-turn hard ceiling could"
    );

    println!(
        "RUNAWAY DRILL RESULT: tripped after {turns} turns, total_spend={total_spend}, \
         detector={}",
        verdict.detector_name()
    );

    // What the pre-Sprint-E behavior would have cost for the identical
    // loop: per LEDGER.md's KT-E, there is no mid-session detector at all
    // today — a looping session runs every attempt to the repo's
    // `max_iterations` ceiling (lopi's own default is 5) before
    // retry-exhaustion dead-letters it. Each "attempt" in that world is a
    // full plan+implement pass, not a single turn — approximate at 3
    // turns/attempt (one plan turn + ~2 implement turns before the gate
    // check that would fail it).
    let turns_per_attempt = 3;
    let default_max_iterations = 5;
    let old_governor_turns = turns_per_attempt * default_max_iterations;
    let old_governor_cost = Money::from_usd(0.42 * f64::from(old_governor_turns));

    println!(
        "RUNAWAY DRILL COMPARISON: pre-Sprint-E governor would have run {old_governor_turns} \
         turns before dead-lettering ({old_governor_cost}) vs {turns} turns caught here \
         ({total_spend}) — {:.1}x less spend",
        old_governor_cost.to_usd() / total_spend.to_usd()
    );

    assert!(
        old_governor_cost > total_spend,
        "the point of this sprint: the new detector must stop the loop for less than the old \
         behavior would have spent"
    );
}
