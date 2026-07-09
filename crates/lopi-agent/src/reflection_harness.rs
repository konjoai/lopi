//! A2 §2 — the measured reflect-vs-blind harness (a regression guard).
//!
//! Reflection is only worth its context cost if it *measurably* beats blind
//! retry. This module is the repeatable three-arm comparison that tests exactly
//! that, in the deterministic, fixture-driven tradition of A1's 24-fixture suite
//! and A3's four score sequences.
//!
//! ## What this is — and is NOT
//!
//! This is a **deterministic mechanism simulation**, not a live LLM benchmark.
//! Each fixture is a retryable task with a candidate-fix pool of size `n` and one
//! root-cause fix; the three arms differ only in how they choose fixes across
//! retries:
//!
//! - [`Arm::Blind`] — sample the pool uniformly each attempt (no memory).
//! - [`Arm::WithinRun`] — a failed attempt's critique eliminates the tried
//!   candidate for the rest of the run (today's `constraints` routing —
//!   sampling *without* replacement).
//! - [`Arm::CrossRun`] — a relevance-filtered durable learning, when present and
//!   retrieval hits (probability [`HarnessParams::retrieval_precision`]), points
//!   attempt 1 at the root cause; on a miss it degrades to within-run and pays a
//!   context-bloat penalty ([`HarnessParams::bloat_penalty`]).
//!
//! A simulated lift is evidence the *mechanism* can help **when retrieval is
//! precise** — it is **not** evidence the live feature beats blind retry. The
//! [precision sweep](precision_sweep) deliberately exposes the failure mode the
//! §2 test warns about: below a precision threshold, imprecise injection + bloat
//! makes cross-run *lose* to blind. The live three-arm run on real tasks (scored
//! by A1's executor) is the true ship gate; see `A2-preregistration.md`.

/// The three reflection arms compared by the harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm {
    /// Retry with no critique carried forward.
    Blind,
    /// Today's behavior — critique eliminates the tried candidate this run.
    WithinRun,
    /// A2 — a durable learning is retrieved and injected across runs.
    CrossRun,
}

/// A single retryable fixture task: a candidate-fix pool with one root cause.
#[derive(Debug, Clone, Copy)]
pub struct FixtureTask {
    /// Stable id — also the deterministic RNG seed (paired across arms).
    pub id: u64,
    /// Number of candidate fixes; exactly one is the root cause.
    pub n_candidates: usize,
    /// Index (`0..n_candidates`) of the fix that actually passes.
    pub root_cause: usize,
    /// Whether a durable learning for this task's failure mode exists in memory
    /// (only the cross-run arm can use it).
    pub learning_relevant: bool,
}

/// Pre-registered harness knobs (see `A2-preregistration.md`).
#[derive(Debug, Clone, Copy)]
pub struct HarnessParams {
    /// P(a retrieved learning points at the true root cause). Baseline `0.8`.
    pub retrieval_precision: f32,
    /// P(an imprecise injection wastes the attempt via context bloat). `0.5`.
    pub bloat_penalty: f32,
    /// Attempts allowed per task before it's scored a failure.
    pub max_attempts: usize,
}

impl Default for HarnessParams {
    /// The pre-registered baseline: precision `0.8`, bloat `0.5`, 4 attempts.
    fn default() -> Self {
        Self {
            retrieval_precision: 0.8,
            bloat_penalty: 0.5,
            max_attempts: 4,
        }
    }
}

/// Aggregate outcome for one arm over the fixture set.
#[derive(Debug, Clone, Copy, Default)]
pub struct ArmStats {
    /// Tasks solved within `max_attempts`.
    pub passed: usize,
    /// Total tasks attempted.
    pub total: usize,
    /// Sum of attempts-to-pass across solved tasks (for the mean).
    pub sum_iters: usize,
}

impl ArmStats {
    /// Fraction of tasks solved, in `0..=1`.
    #[must_use]
    pub fn pass_rate(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        self.passed as f32 / self.total as f32
    }

    /// Mean attempts-to-pass across solved tasks (`0.0` if none passed).
    #[must_use]
    pub fn mean_iters_to_pass(&self) -> f32 {
        if self.passed == 0 {
            return 0.0;
        }
        self.sum_iters as f32 / self.passed as f32
    }
}

/// The full three-arm comparison.
#[derive(Debug, Clone, Copy)]
pub struct ThreeArmReport {
    /// Blind-retry arm.
    pub blind: ArmStats,
    /// Within-run reflection arm (today's behavior).
    pub within_run: ArmStats,
    /// Cross-run reflection arm (A2).
    pub cross_run: ArmStats,
}

impl ThreeArmReport {
    /// Cross-run's pass-rate lead over blind, in **percentage points**. This is
    /// the §2 ship-gate metric.
    #[must_use]
    pub fn cross_vs_blind_pp(&self) -> f32 {
        (self.cross_run.pass_rate() - self.blind.pass_rate()) * 100.0
    }

    /// Cross-run's pass-rate lead over **within-run**, in percentage points — the
    /// *marginal* value of durable cross-run injection on top of today's
    /// within-run reflection. This is where the §2 failure mode bites: imprecise
    /// retrieval + context bloat can make this **negative** (injection displaces
    /// a good within-run attempt), even while cross-run still beats blind because
    /// within-run itself does.
    #[must_use]
    pub fn cross_vs_within_pp(&self) -> f32 {
        (self.cross_run.pass_rate() - self.within_run.pass_rate()) * 100.0
    }

    /// Whether cross-run clears the pre-registered ship margin: beats blind by
    /// `margin_pp` percentage points **and** does not raise mean iterations-to-pass.
    #[must_use]
    pub fn clears_margin(&self, margin_pp: f32) -> bool {
        self.cross_vs_blind_pp() >= margin_pp
            && self.cross_run.mean_iters_to_pass() <= self.blind.mean_iters_to_pass()
    }
}

/// Deterministic splitmix64 — reproducible RNG so the harness numbers are
/// identical run-to-run (no `rand`, no wall-clock seed).
struct SplitMix(u64);

impl SplitMix {
    fn seeded(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..bound`. `bound` must be non-zero (callers guarantee it).
    fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    /// A Bernoulli draw at probability `p` (clamped to `0..=1`).
    fn chance(&mut self, p: f32) -> bool {
        let p = p.clamp(0.0, 1.0);
        ((self.next_u64() % 10_000) as f32) / 10_000.0 < p
    }
}

/// Simulate one arm on one task; returns `Some(attempt)` (1-based) at which it
/// passed, or `None` if it exhausted `max_attempts`. The RNG is re-seeded from
/// `task.id` for every arm, so all three face the same draws — a paired run.
fn simulate(task: FixtureTask, arm: Arm, params: HarnessParams) -> Option<usize> {
    let mut rng = SplitMix::seeded(task.id);
    match arm {
        Arm::Blind => sim_blind(task, &mut rng, params.max_attempts),
        Arm::WithinRun => sim_without_replacement(task, &mut rng, 1, remaining_pool(task)),
        Arm::CrossRun => sim_cross_run(task, &mut rng, params),
    }
}

/// The full candidate pool `0..n`.
fn remaining_pool(task: FixtureTask) -> Vec<usize> {
    (0..task.n_candidates).collect()
}

/// Blind retry: sample the whole pool uniformly each attempt (with replacement).
fn sim_blind(task: FixtureTask, rng: &mut SplitMix, max: usize) -> Option<usize> {
    (1..=max).find(|_| rng.below(task.n_candidates) == task.root_cause)
}

/// Within-run reflection: draw *without replacement* from `remaining`, starting
/// at attempt `start` — a failed attempt's critique removes that candidate.
/// Returns the true attempt-to-pass; the `max_attempts` ceiling is applied by
/// [`run_arm`], so a pass beyond the ceiling is scored a failure there.
fn sim_without_replacement(
    task: FixtureTask,
    rng: &mut SplitMix,
    start: usize,
    mut remaining: Vec<usize>,
) -> Option<usize> {
    let mut attempt = start;
    while !remaining.is_empty() {
        let pick = remaining.remove(rng.below(remaining.len()));
        if pick == task.root_cause {
            return Some(attempt);
        }
        attempt += 1;
    }
    None
}

/// Cross-run reflection: attempt 1 uses the injected learning when the task has a
/// relevant one; on a retrieval miss it pays the bloat penalty, then degrades to
/// within-run for the remaining attempts.
fn sim_cross_run(task: FixtureTask, rng: &mut SplitMix, params: HarnessParams) -> Option<usize> {
    if !task.learning_relevant {
        // Nothing to inject — identical to within-run (same seed → paired).
        return sim_without_replacement(task, rng, 1, remaining_pool(task));
    }
    if rng.chance(params.retrieval_precision) {
        // Precise retrieval points attempt 1 straight at the root cause.
        return Some(1);
    }
    // Imprecise injection: attempt 1 is spent on a wrong fix. With bloat it also
    // fails to eliminate anything; otherwise it removes one wrong candidate.
    let mut remaining = remaining_pool(task);
    if !rng.chance(params.bloat_penalty) {
        if let Some(wrong) = first_wrong(&remaining, task.root_cause) {
            remaining.retain(|&c| c != wrong);
        }
    }
    sim_without_replacement(task, rng, 2, remaining)
}

/// First pool member that is not the root cause.
fn first_wrong(pool: &[usize], root_cause: usize) -> Option<usize> {
    pool.iter().copied().find(|&c| c != root_cause)
}

/// Run one arm over `fixtures`, honoring the params' `max_attempts` ceiling.
fn run_arm(fixtures: &[FixtureTask], arm: Arm, params: HarnessParams) -> ArmStats {
    let mut stats = ArmStats::default();
    for &task in fixtures {
        stats.total += 1;
        if let Some(iters) = simulate(task, arm, params) {
            if iters <= params.max_attempts {
                stats.passed += 1;
                stats.sum_iters += iters;
            }
        }
    }
    stats
}

/// Run the full three-arm comparison over `fixtures` at `params`.
#[must_use]
pub fn run_three_arm(fixtures: &[FixtureTask], params: HarnessParams) -> ThreeArmReport {
    ThreeArmReport {
        blind: run_arm(fixtures, Arm::Blind, params),
        within_run: run_arm(fixtures, Arm::WithinRun, params),
        cross_run: run_arm(fixtures, Arm::CrossRun, params),
    }
}

/// Sweep `retrieval_precision` across `points`, returning `(precision, report)`
/// pairs. Exposes the §2 failure mode: below some precision, cross-run loses.
#[must_use]
pub fn precision_sweep(
    fixtures: &[FixtureTask],
    base: HarnessParams,
    points: &[f32],
) -> Vec<(f32, ThreeArmReport)> {
    points
        .iter()
        .map(|&precision| {
            let params = HarnessParams {
                retrieval_precision: precision,
                ..base
            };
            (precision, run_three_arm(fixtures, params))
        })
        .collect()
}

/// The fixed A2 §2 task set: 20 retryable lopi-style tasks that fail on the first
/// blind try. Pools of 4–8 candidates; ~65% carry a relevant durable learning
/// (the pre-registered mix). Stable ids make the whole harness reproducible.
#[must_use]
pub fn reflection_fixtures() -> Vec<FixtureTask> {
    // (id, n_candidates, root_cause, learning_relevant)
    const SPEC: [(u64, usize, usize, bool); 20] = [
        (1, 4, 2, true),
        (2, 5, 0, true),
        (3, 6, 4, false),
        (4, 4, 3, true),
        (5, 7, 1, true),
        (6, 5, 4, false),
        (7, 6, 2, true),
        (8, 8, 5, true),
        (9, 4, 1, false),
        (10, 5, 3, true),
        (11, 6, 0, true),
        (12, 7, 6, true),
        (13, 4, 0, false),
        (14, 5, 2, true),
        (15, 8, 3, true),
        (16, 6, 5, false),
        (17, 4, 2, true),
        (18, 7, 4, true),
        (19, 5, 1, false),
        (20, 6, 3, true),
    ];
    SPEC.iter()
        .map(|&(id, n_candidates, root_cause, learning_relevant)| FixtureTask {
            id,
            n_candidates,
            root_cause,
            learning_relevant,
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn harness_is_reproducible() {
        let fx = reflection_fixtures();
        let a = run_three_arm(&fx, HarnessParams::default());
        let b = run_three_arm(&fx, HarnessParams::default());
        assert_eq!(a.blind.passed, b.blind.passed);
        assert_eq!(a.cross_run.passed, b.cross_run.passed);
        assert_eq!(a.within_run.passed, b.within_run.passed);
    }

    #[test]
    fn within_run_never_worse_than_blind_on_pass_rate() {
        // Sampling without replacement dominates sampling with replacement.
        let fx = reflection_fixtures();
        let r = run_three_arm(&fx, HarnessParams::default());
        assert!(r.within_run.pass_rate() >= r.blind.pass_rate());
    }

    #[test]
    fn perfect_precision_makes_cross_run_beat_blind() {
        let fx = reflection_fixtures();
        let params = HarnessParams {
            retrieval_precision: 1.0,
            ..HarnessParams::default()
        };
        let r = run_three_arm(&fx, params);
        assert!(
            r.cross_vs_blind_pp() > 0.0,
            "with precise retrieval, reflection must lead blind"
        );
    }

    #[test]
    fn imprecise_injection_costs_marginal_value_over_within_run() {
        // The §2 failure mode: with no precision and full context bloat, the
        // durable injection wastes attempt 1, so cross-run's *marginal* value
        // over today's within-run reflection is non-positive. (It can still beat
        // blind — because within-run does — which is exactly why the ship gate
        // must not be read off the cross-vs-blind number alone.) This is the
        // honest negative the harness exists to catch.
        let fx = reflection_fixtures();
        let params = HarnessParams {
            retrieval_precision: 0.0,
            bloat_penalty: 1.0,
            max_attempts: 4,
        };
        let r = run_three_arm(&fx, params);
        assert!(
            r.cross_vs_within_pp() <= 0.0,
            "with no precision and full bloat, injection must not beat within-run"
        );
    }

    #[test]
    fn precise_injection_earns_marginal_value_over_within_run() {
        let fx = reflection_fixtures();
        let params = HarnessParams {
            retrieval_precision: 1.0,
            ..HarnessParams::default()
        };
        let r = run_three_arm(&fx, params);
        assert!(
            r.cross_vs_within_pp() >= 0.0,
            "precise retrieval must not lose to within-run"
        );
    }

    #[test]
    fn precision_sweep_is_monotone_enough_to_show_the_threshold() {
        let fx = reflection_fixtures();
        let sweep = precision_sweep(
            &fx,
            HarnessParams::default(),
            &[0.0, 0.25, 0.5, 0.75, 1.0],
        );
        let low = sweep.first().map(|(_, r)| r.cross_vs_blind_pp()).unwrap_or(0.0);
        let high = sweep.last().map(|(_, r)| r.cross_vs_blind_pp()).unwrap_or(0.0);
        assert!(high > low, "higher precision must widen cross-run's lead");
    }

    #[test]
    fn fixture_set_is_the_pre_registered_size_and_all_retryable() {
        let fx = reflection_fixtures();
        assert_eq!(fx.len(), 20, "the fixed task set is 20 tasks");
        assert!(
            fx.iter().all(|t| t.root_cause < t.n_candidates),
            "every root cause is a valid pool index"
        );
        assert!(
            fx.iter().all(|t| t.n_candidates >= 4),
            "pools ≥ 4 so blind retry has a real chance of failing in 4 attempts"
        );
    }
}
