//! Progress-Gating (A3) — the loop's live gain gate + termination controls.
//!
//! [`ProgressGate`] carries the per-loop progress state the retry loop threads
//! across attempts: the gain rule, the best sample locked so far, and the
//! consecutive no-progress streak. It reuses A1's evaluation score (via
//! [`GainSample`]) rather than rebuilding scoring, and its stop decisions carry a
//! specific [`StopReason`] rather than a generic halt.
//!
//! The gate is deliberately pure (no I/O) so its accept/reject and stop logic is
//! value-pinned in unit tests; the IO-bearing `record_progress_stop` lives on
//! [`AgentRunner`] and only formats + persists what the gate decides.

use super::run_loop::abort_attempt;
use super::AgentRunner;
use lopi_core::{GainDecision, GainRule, GainSample, StopReason, TaskStatus};
use lopi_git::GitManager;

/// Live progress state for one loop: the gain rule, the best sample so far, and
/// the no-progress streak, plus the two hard caps (`0` disables either).
pub(super) struct ProgressGate {
    rule: GainRule,
    best: Option<GainSample>,
    np_streak: u8,
    no_progress_limit: u8,
    budget_tokens: u64,
}

impl ProgressGate {
    /// Build a gate with the default gain rule and the loop's caps. A
    /// `no_progress_limit`/`budget_tokens` of `0` disables that guard (the
    /// established "0 = disabled" sentinel).
    pub(super) fn new(no_progress_limit: u8, budget_tokens: u64) -> Self {
        Self {
            rule: GainRule::default(),
            best: None,
            np_streak: 0,
            no_progress_limit,
            budget_tokens,
        }
    }

    /// Observe one iteration's sample. On a **gain** the sample locks as the new
    /// best and the no-progress streak resets; on any non-gain (within-noise,
    /// regression, judge-unconfirmed) the prior best is kept and the streak
    /// grows. Returns the decision so the caller can log/route it.
    pub(super) fn observe(&mut self, sample: GainSample) -> GainDecision {
        let decision = self.rule.decide(&sample, self.best.as_ref());
        if decision.is_gain() {
            self.best = Some(sample);
            self.np_streak = 0;
        } else {
            self.np_streak = self.np_streak.saturating_add(1);
        }
        decision
    }

    /// Whether cumulative `tokens_used` has reached the budget cap (when set).
    pub(super) fn budget_exceeded(&self, tokens_used: u64) -> bool {
        self.budget_tokens > 0 && tokens_used >= self.budget_tokens
    }

    /// Whether the no-progress streak has reached the limit (when set).
    pub(super) fn no_progress_tripped(&self) -> bool {
        self.no_progress_limit > 0 && self.np_streak >= self.no_progress_limit
    }

    /// The higher-precedence stop reason that has tripped this iteration, if
    /// any. Budget outranks no-progress ([`StopReason`] precedence), so a loop
    /// that both stalled *and* blew its budget stops as `Budget`.
    pub(super) fn tripped_reason(&self, tokens_used: u64) -> Option<StopReason> {
        let budget = self
            .budget_exceeded(tokens_used)
            .then_some(StopReason::Budget);
        let stall = self.no_progress_tripped().then_some(StopReason::NoProgress);
        match (budget, stall) {
            (Some(a), Some(b)) => Some(a.precede(b)),
            (a, b) => a.or(b),
        }
    }

    /// The current no-progress streak (for logging / diagnostics).
    pub(super) fn streak(&self) -> u8 {
        self.np_streak
    }

    /// The configured no-progress limit (for the stop message).
    pub(super) fn limit(&self) -> u8 {
        self.no_progress_limit
    }

    /// The configured token budget (for the stop message).
    pub(super) fn budget(&self) -> u64 {
        self.budget_tokens
    }
}

impl AgentRunner {
    /// The effective per-loop token budget: a positive `Task::budget_tokens`
    /// wins, else the runner's configured [`task_budget`](Self::task_budget),
    /// else `0` (disabled). Mirrors the "explicit task override wins" precedent.
    pub(super) fn effective_budget_tokens(&self) -> u64 {
        if self.task.budget_tokens > 0 {
            self.task.budget_tokens
        } else {
            self.task_budget().unwrap_or(0)
        }
    }

    /// Top-of-loop budget pre-check (A3): stop before spending more when a
    /// prior attempt's streamed tokens already hit the cap. Returns the terminal
    /// status to bail on, or `None` to proceed. The working tree is clean here,
    /// so this is a pure early-out recorded as [`StopReason::Budget`].
    pub(super) async fn budget_preflight(
        &mut self,
        gate: &ProgressGate,
        git: &GitManager,
        attempt: u8,
    ) -> Option<TaskStatus> {
        let used = self.tokens_used();
        if !gate.budget_exceeded(used) {
            return None;
        }
        let detail = format!("tokens: {used}, budget: {}", gate.budget());
        Some(
            self.record_progress_stop(StopReason::Budget, &detail, git, attempt)
                .await,
        )
    }

    /// Feed this attempt's best objective `weighted` score to the gain gate,
    /// log the decision, and return a terminal status when a termination guard
    /// trips — budget outranks no-progress on precedence. `None` keeps looping.
    ///
    /// A non-gaining iteration is not accepted here: its work is discarded by
    /// the caller's rollback path (A1's finalize rollback) just as before, and
    /// the prior best is kept. This is the gain gate's reject-and-roll-back. (A3)
    pub(super) async fn observe_and_check_stop(
        &mut self,
        gate: &mut ProgressGate,
        weighted: f32,
        git: &GitManager,
        attempt: u8,
    ) -> Option<TaskStatus> {
        let decision = gate.observe(GainSample::objective_only(weighted));
        self.log(format!(
            "📈 gain gate: {} (weighted={weighted:.3})",
            decision.as_str()
        ));
        let reason = gate.tripped_reason(self.tokens_used())?;
        let detail = match reason {
            StopReason::Budget => {
                format!("tokens: {}, budget: {}", self.tokens_used(), gate.budget())
            }
            _ => format!("streak: {}, limit: {}", gate.streak(), gate.limit()),
        };
        Some(
            self.record_progress_stop(reason, &detail, git, attempt)
                .await,
        )
    }

    /// Abort the current attempt (rollback + checkout) and mark it `Retrying` —
    /// the shared cleanup before every `continue` back to the top of the retry
    /// loop. Callers still issue their own `continue` after this returns.
    pub(super) async fn abort_and_mark_retrying(&mut self, git: &GitManager, attempt: u8) {
        abort_attempt(git).await;
        self.status(
            TaskStatus::Retrying {
                attempt: attempt + 1,
            },
            attempt + 1,
        );
    }

    /// Roll back the current attempt and return the terminal [`TaskStatus`] for
    /// a progress-gated stop, tagging the [`StopReason`] into the reason string
    /// (the structured-string-in-`reason` convention `NoProgressStall` and
    /// `TurnLimitExceeded` already use) so it persists on the run and flows
    /// through the DLQ/audit path unchanged.
    pub(super) async fn record_progress_stop(
        &mut self,
        reason: StopReason,
        detail: &str,
        git: &GitManager,
        attempt: u8,
    ) -> TaskStatus {
        self.warn(format!(
            "🛑 stopping — reason={} · {detail}",
            reason.as_str()
        ));
        abort_attempt(git).await;
        let status = TaskStatus::Failed {
            reason: format!("StopReason::{} {{ {detail} }}", reason.as_str()),
        };
        self.status(status.clone(), attempt);
        status
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_locks_baseline_and_resets_streak() {
        let mut gate = ProgressGate::new(3, 0);
        let d = gate.observe(GainSample::objective_only(0.5));
        assert_eq!(d, GainDecision::Gain);
        assert_eq!(gate.streak(), 0);
    }

    #[test]
    fn plateau_grows_streak_and_trips_no_progress_at_k() {
        let mut gate = ProgressGate::new(3, 0);
        gate.observe(GainSample::objective_only(0.5)); // baseline
        assert!(!gate.no_progress_tripped());
        for expected in 1..=3 {
            gate.observe(GainSample::objective_only(0.5)); // no gain
            assert_eq!(gate.streak(), expected);
        }
        assert!(
            gate.no_progress_tripped(),
            "K=3 consecutive stalls must trip"
        );
    }

    #[test]
    fn a_real_gain_resets_the_streak() {
        let mut gate = ProgressGate::new(3, 0);
        gate.observe(GainSample::objective_only(0.5));
        gate.observe(GainSample::objective_only(0.5)); // stall → streak 1
        assert_eq!(gate.streak(), 1);
        let d = gate.observe(GainSample::objective_only(0.9)); // real gain
        assert_eq!(d, GainDecision::Gain);
        assert_eq!(gate.streak(), 0, "a gain must reset the no-progress streak");
    }

    #[test]
    fn a_still_climbing_sequence_never_trips() {
        let mut gate = ProgressGate::new(2, 0);
        for s in [0.30_f32, 0.45, 0.60, 0.80, 0.95] {
            gate.observe(GainSample::objective_only(s));
            assert!(!gate.no_progress_tripped());
        }
    }

    #[test]
    fn budget_trips_only_when_set_and_reached() {
        let disabled = ProgressGate::new(3, 0);
        assert!(!disabled.budget_exceeded(1_000_000));
        let capped = ProgressGate::new(3, 500);
        assert!(!capped.budget_exceeded(499));
        assert!(capped.budget_exceeded(500));
        assert!(capped.budget_exceeded(501));
    }

    #[test]
    fn no_progress_limit_zero_disables_the_stall_guard() {
        let mut gate = ProgressGate::new(0, 0);
        for _ in 0..50 {
            gate.observe(GainSample::objective_only(0.5));
        }
        assert!(!gate.no_progress_tripped());
    }

    #[test]
    fn budget_outranks_no_progress_when_both_trip() {
        let mut gate = ProgressGate::new(1, 100);
        gate.observe(GainSample::objective_only(0.5)); // baseline
        gate.observe(GainSample::objective_only(0.5)); // stall → streak 1, trips
        assert!(gate.no_progress_tripped());
        // Both tripped → budget wins on precedence.
        assert_eq!(gate.tripped_reason(200), Some(StopReason::Budget));
    }

    #[test]
    fn tripped_reason_is_none_when_nothing_fires() {
        let gate = ProgressGate::new(3, 1000);
        assert_eq!(gate.tripped_reason(10), None);
    }

    #[test]
    fn tripped_reason_reports_no_progress_alone() {
        let mut gate = ProgressGate::new(1, 0);
        gate.observe(GainSample::objective_only(0.5));
        gate.observe(GainSample::objective_only(0.5));
        assert_eq!(gate.tripped_reason(0), Some(StopReason::NoProgress));
    }
}
