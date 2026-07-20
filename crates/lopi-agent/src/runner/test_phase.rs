//! Testing → scoring → finalize-or-retry phase, split out of `run_loop.rs`
//! purely to keep that module under the 500-line CI file-size gate; no
//! behavioral difference from being inline — pure code motion.

use super::progress::ProgressGate;
use super::run_loop::abort_attempt;
use super::{schema_gate, AgentRunner};
use crate::claude::ClaudeCode;
use crate::scorer::Scorer;
use anyhow::Result;
use lopi_context::Phase;
use lopi_core::{AgentEvent, Attempt, Score, TaskStatus};
use lopi_git::GitManager;
use tracing::Instrument as _;

/// What the caller (`run_loop.rs`'s attempt loop) should do once this phase
/// returns.
pub(super) enum TestPhaseOutcome {
    /// The task is finished — the caller returns this status from `run()`.
    Terminal(TaskStatus),
    /// Nothing terminal happened this attempt — the caller loops.
    Continue,
}

/// Outcome of [`AgentRunner::attempt_inplace_fix`].
enum FixOutcome {
    /// The fix lifted the score to a pass and finalize succeeded.
    Finalized(TaskStatus),
    /// The fix lifted the score to a pass, but finalize rejected it (the
    /// verifier bounced it back to `Retrying`) — the caller just loops.
    Continue,
    /// Still failing after the fix (or the diff scope broke) — carries the
    /// best weighted score seen so far, for the stall guard below.
    StillFailing(f32),
}

impl AgentRunner {
    /// Score the just-implemented attempt, finalize on a pass (or `until`),
    /// try an in-place fix on a fail and rescore, and — if still failing —
    /// run the gain gate/reflection/retry-delay before signalling the caller
    /// to retry.
    ///
    /// `finalize_on_pass`/`attempt_inplace_fix` are already split out below
    /// to shrink this (was 36, now 29 against CI's 25 threshold) — the
    /// remaining shape is an inherently sequential score → persist →
    /// finalize-or-fix → retry-prep pipeline, the same reason `run()` in
    /// `run_loop.rs` carries `#[allow(clippy::too_many_lines)]` rather than
    /// fragmenting further into cross-called micro-steps that would cost
    /// more in indirection than they'd save in per-function complexity.
    #[allow(clippy::too_many_arguments, clippy::cognitive_complexity)]
    pub(super) async fn run_test_phase(
        &mut self,
        scorer: &Scorer,
        claude: &ClaudeCode,
        git: &GitManager,
        gate: &mut ProgressGate,
        branch: &str,
        attempt: u8,
    ) -> Result<TestPhaseOutcome> {
        if let Err(e) = git
            .check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs)
            .await
        {
            self.warn(format!("diff scope violation: {e}"));
            self.status(TaskStatus::RolledBack, attempt + 1);
            abort_attempt(git).await;
            return Ok(TestPhaseOutcome::Continue);
        }

        self.status(TaskStatus::Testing, attempt + 1);
        self.context.transition_phase(Phase::Testing);
        tracing::info!(
            pressure = self.context.token_pressure(),
            "context at testing"
        );
        // No narration line here — it added nothing beyond restating the
        // `TaskStatus::Testing` transition just above, and (worse) rendered
        // indistinguishably from real Claude output in the transcript. The
        // real, useful signal is the score line right below, once the
        // scorer actually has a result.
        // OTel GenAI-aligned span: score phase.
        let score_span = tracing::info_span!(
            "lopi.agent.score",
            task_id = %self.id(),
            attempt = attempt + 1,
        );
        let score = scorer.score().instrument(score_span).await?;

        // P1.4 — Optional structured-output schema validation (see
        // `schema_gate.rs`). On any violation the agent stashes the
        // messages as `last_error` (so the next planning prompt sees them
        // via adaptive retry) and rolls into the next attempt.
        if let Some(ref schema) = self.task.output_schema {
            if let Some((count, summary)) = schema_gate::violation_summary(schema, &score) {
                self.warn(format!(
                    "📐 output_schema validation failed ({count} issue(s)):\n{summary}"
                ));
                if self.adaptive_retry {
                    self.last_error = Some(format!(
                        "Attempt {} output failed schema validation:\n{summary}",
                        attempt + 1
                    ));
                }
                self.abort_and_mark_retrying(git, attempt).await;
                return Ok(TestPhaseOutcome::Continue);
            }
        }

        self.bus.send(AgentEvent::ScoreUpdated {
            task_id: self.id(),
            test_pass_rate: score.test_pass_rate,
            lint_errors: score.lint_errors,
            diff_lines: score.diff_lines,
        });
        let weighted = score.weighted(&self.score_weights);
        self.log(format!(
            "● score: pass={:.0}% lint={} diff={}L (weighted={:.3})",
            score.test_pass_rate * 100.0,
            score.lint_errors,
            score.diff_lines,
            weighted
        ));
        // Best weighted score seen this attempt — updated if an in-place
        // fix lifts it. Drives the no-progress stall guard below.
        let mut attempt_weighted = weighted;

        // Persist attempt.
        if let Some(store) = &self.store {
            let mut a = Attempt::new(self.id(), attempt + 1, branch);
            a.score = Some(score.clone());
            a.outcome = if score.passed() {
                "success".into()
            } else {
                "retry".into()
            };
            store.save_attempt(&a).await.ok();
        }

        // Guardrails — `until`: an independent exit-condition checked every
        // iteration. A pass ends the loop early as a success regardless of
        // the iteration's own test score; `None` configured leaves
        // `score.passed()` as the sole condition, unchanged from before
        // this field existed.
        let until_satisfied = self.check_until().await;
        if score.passed() || until_satisfied {
            return Ok(self
                .finalize_on_pass(branch, git, &score, until_satisfied, attempt)
                .await);
        }

        match self
            .attempt_inplace_fix(
                scorer,
                claude,
                git,
                branch,
                &score,
                attempt_weighted,
                attempt,
            )
            .await?
        {
            FixOutcome::Finalized(status) => return Ok(TestPhaseOutcome::Terminal(status)),
            FixOutcome::Continue => return Ok(TestPhaseOutcome::Continue),
            FixOutcome::StillFailing(best) => attempt_weighted = best,
        }

        // Sprint H — adaptive retry: stash the score's error list so the
        // next attempt's planning prompt can include it. Only stored when
        // adaptive_retry is enabled to avoid pointless work.
        if self.adaptive_retry {
            let base_failure = format!(
                "Attempt {} failed:\n  test_pass_rate: {:.0}%\n  lint_errors: {}\n  diff_lines: {}\n  errors: {}",
                attempt + 1,
                score.test_pass_rate * 100.0,
                score.lint_errors,
                score.diff_lines,
                if score.errors.is_empty() { "(none captured)".into() } else { score.errors.join("\n  - ") }
            );
            // Phase 16.4/16.5 — reframe the raw failure per the
            // self-prompting strategy. `Direct` returns it unchanged
            // (legacy behaviour); richer strategies prepend a Reflexion /
            // Self-Refine / Plan-Then-Act preamble. With escalation
            // enabled the strategy climbs one S-rung per failed attempt
            // (see `effective_strategy`).
            let strategy = self.effective_strategy(attempt + 1);
            self.last_error = Some(strategy.frame(&base_failure, attempt + 1));
        }

        // Gain gate + termination (A3) — feed this attempt's best objective
        // score to the gate (a gain locks best + resets the streak; a
        // non-gain keeps the prior best and grows it) and stop with a
        // specific `StopReason` when budget or no-progress trips. The
        // rejected (non-gaining) iteration's work is discarded by
        // `abort_and_mark_retrying` below — A1's rollback path, unchanged.
        if let Some(status) = self
            .observe_and_check_stop(gate, attempt_weighted, git, attempt + 1)
            .await
        {
            return Ok(TestPhaseOutcome::Terminal(status));
        }

        // A2 (reflection) — capture the durable learning from this
        // non-gaining attempt *before* `abort_and_mark_retrying` rolls it
        // back. The heuristic score's errors are the critique. No-op
        // unless cross-run reflection is enabled.
        self.capture_learning(&score.errors, "non_gaining").await;
        self.abort_and_mark_retrying(git, attempt).await;
        self.apply_on_fail_delay(attempt).await;
        Ok(TestPhaseOutcome::Continue)
    }

    /// The `score.passed() || until_satisfied` finalize path: forces the
    /// verifier on for L3/L4, commits, rebases onto the advanced default,
    /// then opens (or skips) the PR. `None` from `finalize` ⇒ verifier
    /// rejected (already rolled back, marked `Retrying`) — the caller just
    /// loops. Split out of `run_test_phase` purely to keep that function's
    /// cognitive complexity under CI's gate — pure code motion.
    async fn finalize_on_pass(
        &mut self,
        branch: &str,
        git: &GitManager,
        score: &Score,
        until_satisfied: bool,
        attempt: u8,
    ) -> TestPhaseOutcome {
        if until_satisfied && !score.passed() {
            self.log("● until condition met — concluding the loop early");
        }
        let Some(status) = self
            .finalize(branch, git, score, until_satisfied, attempt + 1)
            .await
        else {
            return TestPhaseOutcome::Continue;
        };
        // Goal-met (A3) — the highest-precedence terminal: the loop
        // satisfied its acceptance/until goal and finalized.
        self.log(format!(
            "● stop reason: {}",
            lopi_core::StopReason::GoalMet.as_str()
        ));
        TestPhaseOutcome::Terminal(self.conclude_finalized(status, score, attempt + 1))
    }

    /// In-place fix attempt on a failing score: ask Claude to fix the
    /// reported errors, rescore, and finalize if that lifted the score to a
    /// pass. Split out of `run_test_phase` purely to keep that function's
    /// cognitive complexity under CI's gate — pure code motion.
    ///
    /// `🔧` is reserved by the frontend for its structured tool_call dedup
    /// (`reduceLogLine` drops any log line starting with it, on the
    /// assumption it's the redundant plain twin of a `ToolCall` event) —
    /// the log line below isn't one, so it was being silently dropped
    /// before `●` replaced the wrench.
    #[allow(clippy::too_many_arguments)]
    async fn attempt_inplace_fix(
        &mut self,
        scorer: &Scorer,
        claude: &ClaudeCode,
        git: &GitManager,
        branch: &str,
        score: &Score,
        attempt_weighted: f32,
        attempt: u8,
    ) -> Result<FixOutcome> {
        self.log(format!("● fixing {} error(s)…", score.errors.len()));
        if let Err(e) = claude.fix(&self.task, &score.errors).await {
            self.warn(format!("fix failed: {e}"));
        }

        if git
            .check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs)
            .await
            .is_err()
        {
            return Ok(FixOutcome::StillFailing(attempt_weighted));
        }

        self.status(TaskStatus::Testing, attempt + 1);
        let fixed_score = scorer.score().await?;
        self.bus.send(AgentEvent::ScoreUpdated {
            task_id: self.id(),
            test_pass_rate: fixed_score.test_pass_rate,
            lint_errors: fixed_score.lint_errors,
            diff_lines: fixed_score.diff_lines,
        });
        let weighted = fixed_score.weighted(&self.score_weights);
        self.log(format!(
            "● fixed score: pass={:.0}% lint={} diff={}L (weighted={:.3})",
            fixed_score.test_pass_rate * 100.0,
            fixed_score.lint_errors,
            fixed_score.diff_lines,
            weighted
        ));
        // The fix lifted (or lowered) the score — track the better of the
        // two for the stall guard.
        let best = attempt_weighted.max(weighted);
        if !fixed_score.passed() {
            return Ok(FixOutcome::StillFailing(best));
        }

        self.log("● fix worked — finalizing…");
        // Same L1–L4 finalize path as the primary success branch. The fix
        // path is never an `until`-driven conclusion, so pass `false`.
        if let Some(status) = self
            .finalize(branch, git, &fixed_score, false, attempt + 1)
            .await
        {
            // Same terminal bookkeeping as `finalize_on_pass` — pins the
            // conclusion message and closes the OTel completion span. This
            // path used to call the bare `self.status(...)` instead, so a
            // task succeeding via in-place fix never got either.
            let status = self.conclude_finalized(status, &fixed_score, attempt + 1);
            return Ok(FixOutcome::Finalized(status));
        }
        Ok(FixOutcome::Continue)
    }
}
