//! A2 §2 — the measured reflect-vs-blind comparison, as a committed regression
//! guard (mirrors A1's `eval_regression.rs`: the kill-test becomes a durable,
//! CI-runnable artifact, not a throwaway).
//!
//! IMPORTANT — this is a **deterministic mechanism simulation**, not a live LLM
//! benchmark. It exercises the real retrieval/dedup/cap logic indirectly and a
//! documented pass/fail model whose knobs are pre-registered in
//! `docs/research/loop-intelligence/A2-preregistration.md`. Its numbers say when
//! the *mechanism* helps (precise retrieval) vs. hurts (imprecise + bloat); they
//! are **not** proof the live feature beats blind retry. Run with
//! `cargo test -p lopi-agent --test reflection_harness -- --nocapture` to see the
//! three-arm table and the precision sweep.

use lopi_agent::reflection_harness::{precision_sweep, reflection_fixtures, run_three_arm};
use lopi_agent::HarnessParams;

/// The pre-registered ship margin (percentage points), from `A2.md` / the
/// pre-registration doc. Cross-run must beat blind by this much to ship
/// on-by-default — a bar the *live* run must clear, not this simulation.
const SHIP_MARGIN_PP: f32 = 15.0;

#[test]
fn three_arm_comparison_reproduces_and_is_reported() {
    let fx = reflection_fixtures();
    let params = HarnessParams::default();
    let report = run_three_arm(&fx, params);

    println!("\n=== A2 §2 — three-arm reflect-vs-blind (mechanism simulation) ===");
    println!(
        "params: retrieval_precision={:.2} bloat_penalty={:.2} max_attempts={}",
        params.retrieval_precision, params.bloat_penalty, params.max_attempts
    );
    for (name, s) in [
        ("blind      ", report.blind),
        ("within_run ", report.within_run),
        ("cross_run  ", report.cross_run),
    ] {
        println!(
            "  {name}: pass {:>2}/{:<2} ({:>5.1}%)  mean iters-to-pass {:.2}",
            s.passed,
            s.total,
            s.pass_rate() * 100.0,
            s.mean_iters_to_pass()
        );
    }
    println!(
        "  cross vs blind:  {:+.1} pp   |   cross vs within: {:+.1} pp",
        report.cross_vs_blind_pp(),
        report.cross_vs_within_pp()
    );
    println!(
        "  ship margin ≥{SHIP_MARGIN_PP:.0}pp cleared (in sim): {}",
        report.clears_margin(SHIP_MARGIN_PP)
    );

    // Reproducibility is the regression-guard property: identical numbers run to
    // run. (No wall-clock/rand seeding.)
    let again = run_three_arm(&fx, params);
    assert_eq!(report.cross_run.passed, again.cross_run.passed);
    assert_eq!(report.blind.passed, again.blind.passed);

    // Structural sanity that must hold for the harness to be meaningful.
    assert!(
        report.within_run.pass_rate() >= report.blind.pass_rate(),
        "within-run reflection must dominate blind retry"
    );
    assert!(
        report.cross_run.pass_rate() >= report.blind.pass_rate(),
        "at the baseline precision cross-run should not trail blind"
    );
}

#[test]
fn precision_sweep_shows_the_threshold() {
    let fx = reflection_fixtures();
    let sweep = precision_sweep(
        &fx,
        HarnessParams::default(),
        &[0.0, 0.2, 0.4, 0.6, 0.8, 1.0],
    );

    println!("\n=== A2 §2 — retrieval-precision sweep (cross-run marginal value) ===");
    println!("  precision | cross_vs_blind_pp | cross_vs_within_pp");
    for (p, r) in &sweep {
        println!(
            "     {p:.2}   |     {:+6.1}       |      {:+6.1}",
            r.cross_vs_blind_pp(),
            r.cross_vs_within_pp()
        );
    }

    // The honest finding the sweep must show: cross-run's marginal value over
    // within-run rises with retrieval precision — reflection earns its context
    // cost only when retrieval is precise, which is why A2 ships it bounded,
    // relevance-filtered, and flagged off pending the live run.
    let lowest = sweep
        .first()
        .map(|(_, r)| r.cross_vs_within_pp())
        .unwrap_or(0.0);
    let highest = sweep
        .last()
        .map(|(_, r)| r.cross_vs_within_pp())
        .unwrap_or(0.0);
    assert!(
        highest > lowest,
        "marginal value must increase with retrieval precision"
    );
}
