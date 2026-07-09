//! A2 (reflection) — durable, rollback-safe learning capture.
//!
//! Extends the existing within-run critique routing
//! ([`eval_runner`](super::eval_runner) appends `EvalOutcome.critique` to
//! `task.constraints`) into a **cross-run** learning: the same critique, plus a
//! short summary of what was attempted and why it was rejected, is distilled and
//! persisted to [`MemoryStore::save_learning`](lopi_memory::MemoryStore::save_learning).
//!
//! The capture call is placed **before** A3's rollback discards the attempt (see
//! [`finalize`](super::finalize) and [`run_loop`](super::run_loop)) so a
//! gain-gate-rejected attempt still yields its lesson — you learned what does
//! *not* work. The write lands in SQLite, which git rollback never touches, so
//! the learning outlives the rolled-back working tree.
//!
//! Everything here is gated on [`reflect_cross_run`](super::AgentRunner) and is
//! best-effort: a capture failure warns (never silently), and never blocks the
//! retry it precedes.

use super::AgentRunner;

/// Longest `attempted` summary persisted with a learning. Keeps the durable row
/// bounded — the plan can be large, but the learning only needs a gist.
const ATTEMPTED_SUMMARY_CAP: usize = 280;

impl AgentRunner {
    /// Capture a durable learning from a rejected/rolled-back attempt.
    ///
    /// No-op unless cross-run reflection is enabled and a store is wired. The
    /// `critique` is the evaluator's flattened gaps/fix-hints (same payload the
    /// within-run path routes into `constraints`); `outcome` is the reject reason
    /// (`eval_rejected`, `non_gaining`, …). Best-effort — an empty critique or a
    /// write error warns and returns without disturbing the loop.
    pub(super) async fn capture_learning(&self, critique: &[String], outcome: &str) {
        if !self.reflect_cross_run {
            return;
        }
        let Some(store) = &self.store else {
            return;
        };
        let critique_text = critique.join("\n");
        if critique_text.trim().is_empty() {
            return;
        }
        let attempted = summarize_attempt(self.last_plan.as_deref(), ATTEMPTED_SUMMARY_CAP);
        let task_id = self.task.id.0.to_string();
        if let Err(e) = store
            .save_learning(
                &self.repo_path.to_string_lossy(),
                &self.task.goal,
                &critique_text,
                &attempted,
                outcome,
                Some(&task_id),
            )
            .await
        {
            self.warn(format!("reflection: failed to capture learning: {e}"));
        } else {
            self.log(format!(
                "🪞 captured a durable learning ({outcome}) — {} critique item(s)",
                critique.len()
            ));
        }
    }
}

/// Distil a plan into a bounded `attempted` summary: the first non-empty line,
/// truncated to `cap` chars. Pure so its wording is unit-testable without a
/// runner. An absent/blank plan yields an empty summary (the critique still
/// carries the signal).
fn summarize_attempt(plan: Option<&str>, cap: usize) -> String {
    let first_line = plan
        .and_then(|p| p.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("");
    if first_line.chars().count() <= cap {
        return first_line.to_string();
    }
    first_line.chars().take(cap).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::AgentRunner;
    use super::summarize_attempt;
    use lopi_core::{AgentEvent, EventBus, Task};
    use lopi_memory::MemoryStore;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    fn runner_with_store(store: MemoryStore, on: bool) -> AgentRunner {
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let (_tx, rx) = tokio::sync::oneshot::channel();
        let task = Task::new("fix the flaky auth timeout test");
        AgentRunner::new(
            task,
            PathBuf::from("/repo"),
            bus,
            Some(store),
            rx,
            Arc::new(AtomicUsize::new(0)),
        )
        .with_cross_run_reflection(on)
    }

    #[tokio::test]
    async fn capture_persists_a_learning_that_outlives_rollback() {
        // Phase 1 verify: a gain-gate-rejected attempt still yields a persisted,
        // retrievable learning. The write lands in SQLite, which the subsequent
        // git rollback never touches.
        let store = MemoryStore::open_in_memory().await.unwrap();
        let runner = runner_with_store(store.clone(), true);
        runner
            .capture_learning(
                &["auth token TTL was set to zero".to_string()],
                "eval_rejected",
            )
            .await;
        // ... the attempt is rolled back here (git-only) ...
        let hits = store
            .find_relevant_learnings("/repo", "auth timeout token expiry", 3)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1, "the rejected attempt's lesson survives");
        assert_eq!(hits[0].outcome, "eval_rejected");
        assert_eq!(hits[0].critique, "auth token TTL was set to zero");
    }

    #[tokio::test]
    async fn capture_is_a_noop_when_reflection_is_off() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let runner = runner_with_store(store.clone(), false);
        runner
            .capture_learning(&["some critique".to_string()], "non_gaining")
            .await;
        assert!(
            store.load_learnings("/repo", 10).await.unwrap().is_empty(),
            "off-by-default must capture nothing"
        );
    }

    #[tokio::test]
    async fn injection_is_relevant_bounded_and_labeled() {
        // Phase 2 verify: retrieval returns relevant learnings for a matching
        // task, near-nothing for unrelated ones, and injection respects the cap.
        let store = MemoryStore::open_in_memory().await.unwrap();
        // Four relevant (auth/timeout) + one unrelated (rendering).
        for i in 0..4 {
            store
                .save_learning(
                    "/repo",
                    "resolve the auth timeout token bug",
                    &format!("auth failure mode {i}"),
                    "",
                    "eval_rejected",
                    None,
                )
                .await
                .unwrap();
        }
        store
            .save_learning(
                "/repo",
                "speed up image rendering pipeline",
                "image cache too small",
                "",
                "non_gaining",
                None,
            )
            .await
            .unwrap();

        let runner = runner_with_store(store, true);
        let injected = runner.seed_reflection_learnings().await;
        assert!(injected.len() <= 3, "the injection cap is honoured");
        assert!(
            !injected.is_empty(),
            "a matching goal retrieves its learnings"
        );
        assert!(
            injected.iter().all(|c| c.starts_with("Past learning")),
            "each learning is labeled as a prior failure"
        );
        assert!(
            injected
                .iter()
                .all(|c| !c.contains("image cache too small")),
            "the unrelated rendering learning must not be injected"
        );
    }

    #[tokio::test]
    async fn injection_is_empty_when_reflection_is_off() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .save_learning(
                "/repo",
                "fix the flaky auth timeout test",
                "x",
                "",
                "eval_rejected",
                None,
            )
            .await
            .unwrap();
        let runner = runner_with_store(store, false);
        assert!(
            runner.seed_reflection_learnings().await.is_empty(),
            "off-by-default injects nothing"
        );
    }

    #[tokio::test]
    async fn capture_skips_an_empty_critique() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let runner = runner_with_store(store.clone(), true);
        runner.capture_learning(&[], "eval_rejected").await;
        runner
            .capture_learning(&["   ".to_string()], "eval_rejected")
            .await;
        assert!(store.load_learnings("/repo", 10).await.unwrap().is_empty());
    }

    #[test]
    fn summary_takes_first_non_empty_line() {
        let plan = "\n\n  Refactor the auth module  \nthen add tests\n";
        assert_eq!(
            summarize_attempt(Some(plan), 280),
            "Refactor the auth module"
        );
    }

    #[test]
    fn summary_truncates_to_cap() {
        let long = "x".repeat(500);
        let out = summarize_attempt(Some(&long), 280);
        assert_eq!(out.chars().count(), 280);
    }

    #[test]
    fn summary_empty_for_no_plan() {
        assert_eq!(summarize_attempt(None, 280), "");
        assert_eq!(summarize_attempt(Some("   \n  "), 280), "");
    }
}
