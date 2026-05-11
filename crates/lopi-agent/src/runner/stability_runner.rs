//! Layer 5 patch stability pre-flight — `AgentRunner` methods.
//!
//! Separated from `run_loop.rs` to keep that file within the 500-line budget.
//! All methods are `impl AgentRunner`; the stability harness lives in
//! `crate::stability`.

use super::AgentRunner;
use crate::stability::StabilityVerdict;
use lopi_core::TaskStatus;
use lopi_memory::StabilityRecord;

impl AgentRunner {
    /// Generate N plan samples, measure pairwise Jaccard variance, persist
    /// the ledger entry, and return `Some(TaskStatus::Failed)` if the gate
    /// blocks this run. Returns `None` when the gate passes (Stable or
    /// Warning) or when no harness is configured.
    pub(super) async fn run_stability_preflight(&self) -> Option<TaskStatus> {
        let harness = self.stability_harness.as_ref()?;

        self.log(format!(
            "🔬 stability gate: generating {} plan samples…",
            harness.config.n_samples
        ));

        let verdict = match harness.assess(&self.task).await {
            Ok(v) => v,
            Err(e) => {
                self.warn(format!("stability harness failed ({e}); proceeding without gate"));
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

        self.log(format!("🔬 stability: {verdict_str} (variance={variance:.3}, samples={n})"));

        if matches!(verdict, StabilityVerdict::Warning { .. }) {
            self.warn(format!(
                "⚠️  patch stability warning — variance={variance:.3} exceeds stable threshold; \
                 proceeding but flagging for review"
            ));
        }

        self.save_stability_ledger_entry(&harness.config.model, verdict_str, n, variance, &verdict)
            .await;

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

    /// Write a stability assessment to the persistent ledger (best-effort).
    pub(super) async fn save_stability_ledger_entry(
        &self,
        model: &str,
        verdict_str: &str,
        n_samples: usize,
        variance_score: f32,
        verdict: &StabilityVerdict,
    ) {
        let Some(store) = &self.store else { return };
        let rec = StabilityRecord {
            task_goal: &self.task.goal,
            model,
            n_samples,
            variance_score,
            verdict: verdict_str,
            semantic_flags: &[],
            accepted: !matches!(verdict, StabilityVerdict::Unstable { .. }),
        };
        if let Err(e) = store.save_stability_entry(rec).await {
            self.warn(format!("stability ledger write failed: {e}"));
        }
    }
}
