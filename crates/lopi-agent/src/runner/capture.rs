//! Success-side constraint capture (Constraint-Capture-2) — the counterpart
//! to `postmortem_runner`'s failure-side capture.
//!
//! `MemoryStore::mine_patterns` runs after every completed task regardless
//! of outcome and always updates `avg_attempts`/`success_rate`, but a
//! `successful_constraints` value only means anything when the run that
//! produced it was actually a clean success. `success_constraint` distils
//! this run's final plan into the same bounded, single-line shape
//! `reflection::summarize_attempt` already produces for a rejected
//! attempt's learning capture — reused here rather than duplicated, since
//! both are "first non-empty line of `last_plan`, truncated."

use super::reflection::{summarize_attempt, ATTEMPTED_SUMMARY_CAP};
use super::AgentRunner;

impl AgentRunner {
    /// Derive a short constraint string from this run's final plan, for
    /// `MemoryStore::mine_patterns` to persist on a clean success.
    ///
    /// `None` when no plan was ever generated (e.g. `dry_run`, or the run
    /// never reached planning) or the plan has no non-empty first line —
    /// callers should treat `None` as "nothing to record," not an error.
    #[must_use]
    pub fn success_constraint(&self) -> Option<String> {
        let summary = summarize_attempt(self.last_plan.as_deref(), ATTEMPTED_SUMMARY_CAP);
        (!summary.is_empty()).then_some(summary)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::AgentRunner;
    use lopi_core::{AgentEvent, EventBus, Task};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    fn runner_with_plan(plan: Option<&str>) -> AgentRunner {
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let (_tx, rx) = tokio::sync::oneshot::channel();
        let task = Task::new("refactor the auth middleware");
        let mut runner = AgentRunner::new(
            task,
            PathBuf::from("/repo"),
            bus,
            None,
            rx,
            Arc::new(AtomicUsize::new(0)),
        );
        runner.last_plan = plan.map(str::to_string);
        runner
    }

    #[test]
    fn success_constraint_takes_first_line_of_last_plan() {
        let runner = runner_with_plan(Some(
            "Extract the token refresh into its own fn\nthen wire callers",
        ));
        assert_eq!(
            runner.success_constraint().as_deref(),
            Some("Extract the token refresh into its own fn")
        );
    }

    #[test]
    fn success_constraint_none_when_no_plan_was_ever_generated() {
        let runner = runner_with_plan(None);
        assert_eq!(runner.success_constraint(), None);
    }

    #[test]
    fn success_constraint_none_when_plan_is_blank() {
        let runner = runner_with_plan(Some("   \n  \n"));
        assert_eq!(runner.success_constraint(), None);
    }
}
