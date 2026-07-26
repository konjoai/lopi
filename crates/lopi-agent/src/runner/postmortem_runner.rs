//! Failure post-mortem — `AgentRunner` methods.
//!
//! Separated from `run_loop.rs` to stay within the 500-line budget.
//! The lower-level `run_postmortem_quiet()` lives in `postmortem.rs`.

use super::{postmortem, postmortem_cli, AgentRunner};
use crate::claude::model_haiku;
use tracing::warn;

impl AgentRunner {
    /// Run the failure post-mortem if adaptive retry produced a failure to
    /// reflect on. Best-effort; on any error a warning is logged and the
    /// agent loop continues normally. On success the derived constraint is
    /// persisted as a pattern + a "recovery" lesson.
    ///
    /// Backend selection (Sprint F1 Phase 3) — same rule as
    /// `run_verifier_pass`: an `AnthropicClient` when one is configured, the
    /// `claude` CLI otherwise. Before this sprint this method required a
    /// client and silently no-op'd without one — which, per `with_api` never
    /// being wired in production, meant the post-mortem never ran at all.
    pub(super) async fn run_postmortem_if_configured(&self) {
        let Some(error_log) = self.last_error.as_deref() else {
            return;
        };

        self.log("🧠 running failure post-mortem…");
        let outcome = match self.api_client.as_ref() {
            Some(client) => {
                postmortem::run_postmortem_quiet(
                    client,
                    self.limiter.as_ref(),
                    self.breaker.as_ref(),
                    model_haiku(),
                    &self.task.goal,
                    error_log,
                )
                .await
            }
            None => {
                match postmortem_cli::run_postmortem_cli(
                    &self.repo_path,
                    model_haiku(),
                    &self.task.goal,
                    error_log,
                )
                .await
                {
                    Ok(out) => Some(out),
                    Err(e) => {
                        warn!(error = %e, "post-mortem (cli) failed; no pattern derived");
                        None
                    }
                }
            }
        };

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
