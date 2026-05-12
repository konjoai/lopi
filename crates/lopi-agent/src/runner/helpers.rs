use super::AgentRunner;
use lopi_core::{AgentEvent, ScoreWeights, TaskStatus};

impl AgentRunner {
    /// Load evolved `ScoreWeights` from the memory store's annotation signal.
    /// Falls back to `ScoreWeights::default()` on any failure or absent store.
    pub(super) async fn load_score_weights(&mut self) {
        if let Some(store) = &self.store {
            match store.compute_weight_adjustments().await {
                Ok(w) => {
                    self.score_weights = w;
                    tracing::debug!("score weights loaded from annotation signal");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "weight computation failed, using defaults");
                    self.score_weights = ScoreWeights::default();
                }
            }
        }
    }

    pub(super) fn status(&self, s: TaskStatus, attempt: u8) {
        let activity = match &s {
            TaskStatus::Planning => 0.45_f32,
            TaskStatus::Implementing => 0.85_f32,
            TaskStatus::Testing => 0.55_f32,
            TaskStatus::Scoring => 0.30_f32,
            TaskStatus::Retrying { .. } => 0.40_f32,
            TaskStatus::Success { .. } | TaskStatus::Failed { .. } | TaskStatus::RolledBack => {
                0.0_f32
            }
            TaskStatus::Queued => 0.10_f32,
        };
        self.emit_turn_metrics(activity);
        self.bus.send(AgentEvent::StatusChanged {
            task_id: self.id(),
            status: s,
            attempt,
        });
    }

    pub(super) fn emit_turn_metrics(&self, activity: f32) {
        let pressure = self.context.token_pressure();
        self.bus.send(AgentEvent::TurnMetrics {
            task_id: self.id(),
            pressure,
            activity,
            tokens_per_sec: 0.0,
            cost_usd: 0.0,
        });
    }

    /// Sprint J-A — run the KCQF quality scanner after a successful task.
    ///
    /// Calls `lopi_kcqf::scan_diff` on the repo root (clippy always; coverage
    /// skipped when no diff files are specified). Each violation is converted to
    /// a `TaskSource::Maintenance` task and stored in `self.maintenance_tasks`
    /// for the pool to drain and re-queue. Best-effort: scan failures are
    /// logged as warnings and never block the success return.
    pub(super) async fn run_kcqf_scan(&mut self) {
        if !self.kcqf_enabled {
            return;
        }
        self.log("🔍 KCQF: scanning for quality violations…");
        // diff_files is empty: coverage scanning (which is per-file) is skipped.
        // Only clippy runs workspace-wide. Passing changed-file paths is a future enhancement.
        match lopi_kcqf::scan_diff(&self.repo_path, &[]).await {
            Ok(violations) if violations.is_empty() => {
                self.log("🔍 KCQF: clean — no violations detected");
            }
            Ok(violations) => {
                let tasks = lopi_kcqf::violations_to_tasks(&violations);
                self.log(format!(
                    "🔍 KCQF: {} violation(s) → {} maintenance task(s) queued",
                    violations.len(),
                    tasks.len()
                ));
                self.maintenance_tasks.extend(tasks);
            }
            Err(e) => {
                self.warn(format!("KCQF scan failed (non-fatal): {e}"));
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopi_core::{ScoreWeights, Task};
    use lopi_memory::MemoryStore;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use tokio::sync::oneshot;

    fn make_runner(store: Option<MemoryStore>) -> AgentRunner {
        let bus = lopi_core::EventBus::new(16);
        let task = Task::new("test goal");
        let (_tx, rx) = oneshot::channel();
        let mut r = AgentRunner::new(
            task,
            PathBuf::from("."),
            bus,
            store,
            rx,
            Arc::new(AtomicUsize::new(0)),
        );
        r.score_weights = ScoreWeights::default();
        r
    }

    #[tokio::test]
    async fn load_score_weights_uses_defaults_without_store() {
        let mut runner = make_runner(None);
        let before = runner.score_weights.clone();
        runner.load_score_weights().await;
        // With no store, weights stay at default
        assert_eq!(
            runner.score_weights.lint_penalty_per_error,
            before.lint_penalty_per_error
        );
    }

    #[tokio::test]
    async fn load_score_weights_with_empty_store_stays_default() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let mut runner = make_runner(Some(store));
        runner.load_score_weights().await;
        let defaults = ScoreWeights::default();
        assert_eq!(
            runner.score_weights.lint_penalty_per_error,
            defaults.lint_penalty_per_error
        );
    }

    #[tokio::test]
    async fn load_lessons_returns_repo_lessons() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .save_lesson(
                "/test/repo",
                "strategy",
                "always run clippy first",
                None,
                0.9,
            )
            .await
            .unwrap();
        let lessons = store.load_lessons("/test/repo", 3).await.unwrap();
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].content, "always run clippy first");
    }

    #[tokio::test]
    async fn load_lessons_empty_for_different_repo() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .save_lesson("/repo-a", "strategy", "use cargo test", None, 0.9)
            .await
            .unwrap();
        let lessons = store.load_lessons("/repo-b", 3).await.unwrap();
        assert!(lessons.is_empty());
    }

    #[test]
    fn kcqf_disabled_by_default() {
        let runner = make_runner(None);
        assert!(!runner.kcqf_enabled);
        assert!(runner.maintenance_tasks.is_empty());
    }

    #[test]
    fn with_kcqf_enables_flag_and_take_drains() {
        let (runner, _bus) = AgentRunner::standalone(Task::new("test"), PathBuf::from("."));
        let mut runner = runner.with_kcqf();
        assert!(runner.kcqf_enabled);
        // Manually populate maintenance_tasks to test drain.
        runner.maintenance_tasks.push(Task::new("maint-1"));
        runner.maintenance_tasks.push(Task::new("maint-2"));
        let drained = runner.take_maintenance_tasks();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].goal, "maint-1");
        assert!(runner.maintenance_tasks.is_empty());
    }
}
