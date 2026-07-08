//! Guardrail primitives: `gate` (precondition), `until` (exit-condition),
//! and `on_fail` (retry pacing policy). Wired into the retry loop in
//! `run_loop.rs`; kept here so the decision/wait logic is unit-testable
//! without a real git repo or `claude` subprocess.

use super::AgentRunner;
use lopi_core::loop_config::{run_guard_command, OnFail};
use lopi_core::TaskStatus;
use std::time::Duration;

impl AgentRunner {
    /// Guardrail precondition (`gate`): a shell command that must exit `0`
    /// before the loop's first iteration. `None` (no gate configured) always
    /// passes. A command that runs and exits non-zero — or fails to spawn at
    /// all — blocks the loop with a `GateBlocked` failure, the same
    /// structured-string-in-`reason` convention `TurnLimitExceeded` and
    /// `NoProgressStall` already use, so it flows through the existing
    /// `TaskCompleted` event and DLQ paths with no new event/outcome type.
    pub(super) async fn run_gate_preflight(&self) -> Option<TaskStatus> {
        let cmd = self.gate.clone()?;
        let passed = run_guard_command(&cmd, &self.repo_path)
            .await
            .unwrap_or_else(|e| {
                self.warn(format!(
                    "gate command failed to spawn ({e}); treating as blocked"
                ));
                false
            });
        if passed {
            return None;
        }
        self.warn(format!("🚧 gate blocked: `{cmd}` did not exit 0"));
        Some(TaskStatus::Failed {
            reason: format!("GateBlocked {{ cmd: {cmd:?}, task_id: {} }}", self.task.id),
        })
    }

    /// Guardrail exit-condition (`until`): checked after each iteration's
    /// score is computed. `None` (no `until` configured) never signals
    /// done — scoring/`max_iterations` remain the sole stop conditions,
    /// unchanged from before this field existed. A spawn failure is treated
    /// as "not yet done" (never silently assumes success) and warns loudly.
    pub(super) async fn check_until(&self) -> bool {
        let Some(cmd) = self.until.clone() else {
            return false;
        };
        run_guard_command(&cmd, &self.repo_path)
            .await
            .unwrap_or_else(|e| {
                self.warn(format!(
                    "until command failed to spawn ({e}); continuing loop"
                ));
                false
            })
    }

    /// Apply the `on_fail`-policy pause after a failed iteration (see
    /// [`on_fail_wait`]) and log it — the shared tail of the retry loop's
    /// `abort_and_mark_retrying` + backoff sequence.
    pub(super) async fn apply_on_fail_delay(&self, attempt: u8) {
        let wait = on_fail_wait(self.on_fail, attempt);
        if wait.is_zero() {
            self.log(format!(
                "♻️ retry {}/{} (on-fail:continue — no pause)",
                attempt + 1,
                self.task.max_retries
            ));
        } else {
            self.log(format!(
                "♻️ retry {}/{} (backoff {}ms)",
                attempt + 1,
                self.task.max_retries,
                wait.as_millis()
            ));
            tokio::time::sleep(wait).await;
        }
    }
}

/// The pause to apply after a failed iteration, per the `on_fail` policy.
///
/// `Stop` (the default) and `Backoff` both reuse the existing full-jitter
/// backoff ([`super::backoff_secs`]) — `Stop` reproduces today's
/// unconditional pre-existing behavior exactly (required: it is the
/// default, and configs carrying none of these fields must behave
/// identically to before); `Backoff` is the same wait offered as an
/// explicit, named choice. `Continue` skips the pause and retries
/// immediately.
pub(super) fn on_fail_wait(policy: OnFail, attempt: u8) -> Duration {
    match policy {
        OnFail::Continue => Duration::ZERO,
        OnFail::Stop | OnFail::Backoff => super::backoff_secs(attempt, 500),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopi_core::Task;

    fn cwd() -> std::path::PathBuf {
        std::env::temp_dir()
    }

    #[tokio::test]
    async fn gate_unset_never_blocks() {
        let task = Task::new("no gate");
        let (runner, _bus) = AgentRunner::standalone(task, cwd());
        assert!(runner.run_gate_preflight().await.is_none());
    }

    #[tokio::test]
    async fn gate_passing_command_lets_the_loop_start() {
        let task = Task::new("ok gate");
        let (mut runner, _bus) = AgentRunner::standalone(task, cwd());
        runner.gate = Some("true".to_string());
        assert!(runner.run_gate_preflight().await.is_none());
    }

    #[tokio::test]
    async fn gate_failing_command_blocks_with_a_clear_outcome() {
        let task = Task::new("blocked gate");
        let (mut runner, _bus) = AgentRunner::standalone(task, cwd());
        runner.gate = Some("exit 1".to_string());
        let blocked = runner.run_gate_preflight().await;
        assert!(
            matches!(&blocked, Some(TaskStatus::Failed { reason }) if reason.contains("GateBlocked")),
            "expected a GateBlocked Failed status, got {blocked:?}"
        );
    }

    #[tokio::test]
    async fn until_unset_never_reports_done() {
        let task = Task::new("no until");
        let (runner, _bus) = AgentRunner::standalone(task, cwd());
        assert!(!runner.check_until().await);
    }

    #[tokio::test]
    async fn until_true_reports_done_immediately() {
        let task = Task::new("done until");
        let (mut runner, _bus) = AgentRunner::standalone(task, cwd());
        runner.until = Some("true".to_string());
        assert!(runner.check_until().await);
    }

    #[tokio::test]
    async fn until_false_never_reports_done() {
        let task = Task::new("never-done until");
        let (mut runner, _bus) = AgentRunner::standalone(task, cwd());
        runner.until = Some("false".to_string());
        assert!(!runner.check_until().await);
    }

    #[test]
    fn continue_never_waits() {
        for attempt in 0..=6u8 {
            assert_eq!(on_fail_wait(OnFail::Continue, attempt), Duration::ZERO);
        }
    }

    /// `Stop` and `Backoff` must both call the existing `backoff_secs`
    /// helper — proven here by checking every sample from either policy
    /// falls inside `backoff_secs`'s own `[0, ceiling]` contract for that
    /// attempt, and that the wait is actually nonzero at least sometimes.
    /// A hardcoded *second* delay constant would either never vary (fail
    /// the nonzero check) or fall outside the ceiling (fail the bound).
    #[test]
    fn stop_and_backoff_share_the_backoff_secs_distribution() {
        for attempt in 0..=6u8 {
            let ceiling_ms = (500u64 * (1u64 << attempt.min(10))).min(30_000);
            let mut saw_nonzero = false;
            for _ in 0..50 {
                for policy in [OnFail::Stop, OnFail::Backoff] {
                    let wait = on_fail_wait(policy, attempt);
                    assert!(
                        u64::try_from(wait.as_millis()).unwrap_or(u64::MAX) <= ceiling_ms,
                        "on_fail_wait({policy:?}, {attempt}) exceeded backoff_secs's own ceiling"
                    );
                    if wait > Duration::ZERO {
                        saw_nonzero = true;
                    }
                }
            }
            assert!(
                saw_nonzero,
                "backoff must actually wait sometimes at attempt {attempt}"
            );
        }
    }
}
