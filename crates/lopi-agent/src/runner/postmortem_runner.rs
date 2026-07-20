//! Failure post-mortem — `AgentRunner` methods.
//!
//! Separated from `run_loop.rs` to stay within the 500-line budget.
//! The lower-level `run_postmortem_quiet()` lives in `postmortem.rs`.

use super::{postmortem, AgentRunner};
use crate::claude::MODEL_HAIKU;

impl AgentRunner {
    /// Run the failure post-mortem if both adaptive retry and a direct-API
    /// client are configured. Best-effort; on any error a warning is logged
    /// and the agent loop continues normally. On success the derived
    /// constraint is persisted as a pattern + a "recovery" lesson.
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

        if let Some(out) = outcome {
            self.persist_postmortem_outcome(&out.constraint).await;
        }
    }

    /// Persist a postmortem-derived constraint as a pattern and a lesson.
    pub(super) async fn persist_postmortem_outcome(&self, constraint: &str) {
        let Some(store) = &self.store else {
            self.log(format!("🧠 post-mortem constraint: {constraint}"));
            return;
        };
        match store
            .insert_postmortem_pattern(&self.task.goal, constraint)
            .await
        {
            Ok(id) => {
                // `id` is always a UUID string today (see
                // `MemoryStore::insert_postmortem_pattern`), but slice by
                // `char_indices` rather than a raw byte index so a future
                // change to a shorter/non-ASCII id can't turn this log line
                // into a panic.
                let short_id = id.get(..8).unwrap_or(id.as_str());
                self.log(format!("🧠 post-mortem pattern saved [{short_id}]"));
                self.log(format!("    constraint: {constraint}"));
                let task_id_str = self.task.id.0.to_string();
                if let Err(e) = store
                    .save_lesson(
                        &self.repo_path.to_string_lossy(),
                        "recovery",
                        constraint,
                        Some(&task_id_str),
                        1.0,
                    )
                    .await
                {
                    self.warn(format!("failed to save post-mortem lesson: {e}"));
                }
            }
            Err(e) => self.warn(format!("post-mortem persist failed: {e}")),
        }
    }
}
