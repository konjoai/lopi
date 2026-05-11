use super::{postmortem, AgentRunner};
use crate::claude::MODEL_HAIKU;
use crate::stability::StabilityVerdict;
use lopi_core::{AgentEvent, ScoreWeights, TaskStatus};
use lopi_memory::StabilityRecord;

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

    /// Sprint H — run the failure post-mortem if both adaptive retry and a
    /// direct-API client are configured. Best-effort; on any error we log
    /// a warning and continue. The derived constraint is persisted to the
    /// patterns table with `derived_from_postmortem = 1`.
    pub(super) async fn run_postmortem_if_configured(&self) {
        let Some(client) = self.api_client.as_ref() else {
            return;
        };
        let Some(error_log) = self.last_error.as_deref() else {
            return;
        };

        self.log("🧠 running failure post-mortem…");
        let outcome = postmortem::run_postmortem_quiet(
            client,
            self.limiter.as_ref(),
            self.breaker.as_ref(),
            MODEL_HAIKU,
            &self.task.goal,
            error_log,
        )
        .await;

        let Some(outcome) = outcome else {
            return;
        };

        if let Some(store) = &self.store {
            match store
                .insert_postmortem_pattern(&self.task.goal, &outcome.constraint)
                .await
            {
                Ok(id) => {
                    self.log(format!("🧠 post-mortem pattern saved [{}]", &id[..8]));
                    self.log(format!("    constraint: {}", outcome.constraint));
                    self.maybe_propose_self_modify(store).await;
                }
                Err(e) => {
                    self.warn(format!("post-mortem persist failed: {e}"));
                }
            }
        } else {
            self.log(format!("🧠 post-mortem constraint: {}", outcome.constraint));
        }
    }

    /// Layer 5 stability pre-flight. Generates N plans, measures variance,
    /// persists the ledger entry, and returns `Some(TaskStatus::Failed)` if
    /// the harness blocks this run. Returns `None` when the gate passes
    /// (Stable or Warning) or when no harness is configured.
    pub(super) async fn run_stability_preflight(&self) -> Option<TaskStatus> {
        let harness = self.stability_harness.as_ref()?;

        self.log(format!(
            "🔬 stability gate: generating {} plan samples…",
            harness.config.n_samples
        ));

        let verdict = match harness.assess(&self.task).await {
            Ok(v) => v,
            Err(e) => {
                self.warn(format!(
                    "stability harness failed ({e}); proceeding without gate"
                ));
                return None;
            }
        };

        let verdict_str = verdict.as_str();
        let variance = verdict.variance_score();
        let n = match &verdict {
            StabilityVerdict::Stable { n_samples, .. }
            | StabilityVerdict::Warning { n_samples, .. }
            | StabilityVerdict::Unstable { n_samples, .. } => *n_samples,
        };

        self.log(format!(
            "🔬 stability: {verdict_str} (variance={variance:.3}, samples={n})"
        ));

        if matches!(verdict, StabilityVerdict::Warning { .. }) {
            self.warn(format!(
                "⚠️  patch stability warning — variance={variance:.3} exceeds stable threshold; \
                 proceeding but flagging for review"
            ));
        }

        if let Some(store) = &self.store {
            let rec = StabilityRecord {
                task_goal: &self.task.goal,
                model: &harness.config.model,
                n_samples: n,
                variance_score: variance,
                verdict: verdict_str,
                semantic_flags: &[],
                accepted: !matches!(verdict, StabilityVerdict::Unstable { .. }),
            };
            if let Err(e) = store.save_stability_entry(rec).await {
                self.warn(format!("stability ledger write failed: {e}"));
            }
        }

        if matches!(verdict, StabilityVerdict::Unstable { .. }) {
            let reason = format!(
                "StabilityGateBlocked: variance={variance:.3} (>{:.2}) with {n} samples — \
                 human review required before proceeding",
                harness.config.warning_threshold
            );
            self.log(format!("🚫 {reason}"));
            Some(TaskStatus::Failed { reason })
        } else {
            None
        }
    }

    /// Emit `SelfModifyProposed` if recent post-mortem count crosses the threshold.
    /// Best-effort — any error is logged and ignored.
    async fn maybe_propose_self_modify(&self, store: &lopi_memory::MemoryStore) {
        const THRESHOLD: i64 = 3;
        const WINDOW_HOURS: i64 = 24;

        match store.recent_postmortem_count(WINDOW_HOURS).await {
            Ok(count) if count >= THRESHOLD => {
                let goal = format!(
                    "Self-improve: {count} failure patterns detected in the last {WINDOW_HOURS}h"
                );
                self.bus
                    .send(AgentEvent::SelfModifyProposed { goal: goal.clone() });
                self.log(format!("🔔 SelfModifyProposed emitted ({count} patterns)"));
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "recent_postmortem_count failed");
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
}
