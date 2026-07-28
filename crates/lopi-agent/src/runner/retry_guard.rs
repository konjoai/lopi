//! Verification gate — duplicate-retry-prompt guard (Finding #1).
//!
//! "Never re-send an identical prompt. If attempt N's prompt hashes equal to
//! attempt N-1's, that is a bug." When the evidence-forwarding mechanism
//! (Sprint H adaptive retry, `self.last_error`) fails to produce anything
//! new — the escalation ladder is pinned, or the same failure reason keeps
//! getting emitted verbatim — a retry burns a whole attempt re-asking the
//! same question with the same context, which is very likely to fail the
//! same way again. This does not abort that retry (killing the loop on a
//! first repeat would be too aggressive for a legitimately intermittent
//! failure); it makes the repetition visible instead of silent.

use super::AgentRunner;

impl AgentRunner {
    /// Compare this iteration's retry evidence (`self.last_error`, already
    /// set by the previous iteration's failure path — see `test_phase.rs`,
    /// `secrets_gate.rs`, `finalize.rs`) against what the *previous*
    /// iteration itself saw, and warn on an exact repeat. Called once at the
    /// top of each attempt, before that evidence is used to build the
    /// planning prompt (`self.last_error` does not change again until this
    /// same iteration's own failure path runs, so the value read here is
    /// exactly what this attempt's prompt will carry).
    pub(super) fn check_duplicate_retry_prompt(&mut self, attempt: u8) {
        if is_duplicate_prompt(
            self.prev_loop_top_error.as_deref(),
            self.last_error.as_deref(),
        ) {
            self.warn(format!(
                "⚠ attempt {} retry evidence is byte-identical to the previous attempt's — \
                 the adaptive-retry/escalation mechanism produced nothing new; this retry is \
                 likely to fail the same way",
                attempt + 1
            ));
        }
        self.prev_loop_top_error = self.last_error.clone();
    }
}

/// Pure comparison: `true` only when both are `Some` and equal. Two `None`s
/// (no evidence yet on either side, e.g. the first two attempts of a loop
/// with adaptive retry off) are not a repeat — they're the absence of
/// evidence, not identical evidence.
fn is_duplicate_prompt(prev: Option<&str>, current: Option<&str>) -> bool {
    matches!((prev, current), (Some(p), Some(c)) if p == c)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lopi_core::Task;

    #[test]
    fn two_nones_are_not_a_duplicate() {
        assert!(!is_duplicate_prompt(None, None));
    }

    #[test]
    fn first_ever_evidence_is_not_a_duplicate() {
        assert!(!is_duplicate_prompt(None, Some("first failure")));
    }

    #[test]
    fn identical_evidence_is_a_duplicate() {
        assert!(is_duplicate_prompt(Some("same error"), Some("same error")));
    }

    #[test]
    fn differing_evidence_is_not_a_duplicate() {
        assert!(!is_duplicate_prompt(
            Some("cargo test failed: foo"),
            Some("cargo test failed: bar")
        ));
    }

    #[test]
    fn evidence_that_stops_appearing_is_not_flagged() {
        // Attempt N had evidence, attempt N+1 has none (e.g. a different,
        // non-adaptive failure path) — not the "resent verbatim" bug.
        assert!(!is_duplicate_prompt(Some("prior failure"), None));
    }

    #[tokio::test]
    async fn runner_warns_on_a_repeated_prompt_and_updates_state() {
        let task = Task::new("fix the flaky test");
        let (mut runner, bus) = AgentRunner::standalone(task, std::path::PathBuf::from("."));
        let mut rx = bus.subscribe();

        runner.last_error = Some("cargo test failed: it_works".into());
        runner.check_duplicate_retry_prompt(0);
        assert_eq!(
            runner.prev_loop_top_error.as_deref(),
            Some("cargo test failed: it_works")
        );
        // First sighting — no warning yet.
        assert!(rx.try_recv().is_err());

        // Next attempt sees the exact same evidence — the bug this guards.
        runner.check_duplicate_retry_prompt(1);
        let mut warned = false;
        while let Ok(ev) = rx.try_recv() {
            if let lopi_core::AgentEvent::LogLine { level, line, .. } = ev {
                if matches!(level, lopi_core::LogLevel::Warn) && line.contains("byte-identical") {
                    warned = true;
                }
            }
        }
        assert!(warned, "must warn on an exact repeat");
    }

    #[tokio::test]
    async fn runner_stays_quiet_when_evidence_changes() {
        let task = Task::new("fix the flaky test");
        let (mut runner, bus) = AgentRunner::standalone(task, std::path::PathBuf::from("."));
        let mut rx = bus.subscribe();

        runner.last_error = Some("first failure".into());
        runner.check_duplicate_retry_prompt(0);
        runner.last_error = Some("a different, more specific failure".into());
        runner.check_duplicate_retry_prompt(1);

        let mut warned = false;
        while let Ok(ev) = rx.try_recv() {
            if let lopi_core::AgentEvent::LogLine { level, line, .. } = ev {
                if matches!(level, lopi_core::LogLevel::Warn) && line.contains("byte-identical") {
                    warned = true;
                }
            }
        }
        assert!(!warned, "evidence changed — must not warn");
    }
}
