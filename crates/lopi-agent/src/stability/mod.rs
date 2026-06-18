//! Layer 5 Patch Stability Harness.
//!
//! Before committing to an implementation, generate `N` independent plan
//! samples for the same task prompt and measure pairwise Jaccard variance.
//! High variance means the model output is non-deterministic for this task
//! class — a signal that human review is required before proceeding.
//!
//! # Variance thresholds (defaults)
//! - `variance ≤ 0.15` → **Stable**: proceed, use consensus plan.
//! - `0.15 < variance ≤ 0.35` → **Warning**: proceed with a diagnostic log.
//! - `variance > 0.35` → **Unstable**: block the run, require human review.
//!
//! # Stability ledger
//! Every assessment is persisted to the `stability_ledger` SQLite table via
//! `lopi-memory`. Over time this builds an empirical dataset of:
//!   (task_type, model, variance_score, verdict)
//!
//! That dataset quantifies which task categories are safe to self-ship and
//! which always need a human gate — the research artifact from the Layer 5
//! design conversation.
//!
//! # Integration
//! Wire via `AgentRunner::with_stability_gate(config)`. When set, `run()`
//! calls `StabilityHarness::assess()` before the first implementation attempt.
//! `Unstable` verdicts abort the run with a `TaskStatus::Failed` reason string
//! that includes the variance score for the ledger.

pub mod semantic;
pub mod similarity;

pub use semantic::flag_out_of_scope;
pub use similarity::{jaccard, variance_and_consensus};

use crate::api_client::{AnthropicClient, LOPI_SYSTEM_PROMPT};
use anyhow::Result;
use lopi_core::Task;
use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
use std::sync::Arc;

/// Configuration for the Layer 5 stability gate.
#[derive(Debug, Clone)]
pub struct StabilityConfig {
    /// Number of independent plan samples to generate (default 5).
    pub n_samples: usize,
    /// Variance ≤ this → `Stable` (default 0.15).
    pub stable_threshold: f32,
    /// Variance ≤ this → `Warning`; above → `Unstable` (default 0.35).
    pub warning_threshold: f32,
    /// Model used for plan generation — should match the planning model.
    pub model: String,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            n_samples: 5,
            stable_threshold: 0.15,
            warning_threshold: 0.35,
            model: crate::claude::MODEL_SONNET.to_string(),
        }
    }
}

/// Outcome of a stability assessment.
#[derive(Debug)]
pub enum StabilityVerdict {
    /// Variance ≤ `stable_threshold` — proceed. `consensus_plan` is the plan
    /// most representative of the N samples (highest mean similarity to peers).
    Stable {
        /// The consensus plan text.
        consensus_plan: String,
        /// Variance score: 1 − mean_pairwise_jaccard ∈ [0, 1].
        variance_score: f32,
        /// Number of plan samples actually collected.
        n_samples: usize,
    },
    /// Variance in `(stable_threshold, warning_threshold]` — proceed with a
    /// diagnostic warning logged to the event bus.
    Warning {
        /// The consensus plan text.
        consensus_plan: String,
        /// Variance score.
        variance_score: f32,
        /// Number of plan samples actually collected.
        n_samples: usize,
    },
    /// Variance > `warning_threshold` — block the run and require human review.
    Unstable {
        /// Variance score.
        variance_score: f32,
        /// Number of plan samples actually collected.
        n_samples: usize,
    },
}

impl StabilityVerdict {
    /// Short verdict label for the stability ledger and log output.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable { .. } => "stable",
            Self::Warning { .. } => "warning",
            Self::Unstable { .. } => "unstable",
        }
    }

    /// Variance score embedded in this verdict.
    #[must_use]
    pub fn variance_score(&self) -> f32 {
        match self {
            Self::Stable { variance_score, .. }
            | Self::Warning { variance_score, .. }
            | Self::Unstable { variance_score, .. } => *variance_score,
        }
    }

    /// Consensus plan text if available (Stable and Warning only).
    #[must_use]
    pub fn consensus_plan(&self) -> Option<&str> {
        match self {
            Self::Stable { consensus_plan, .. } | Self::Warning { consensus_plan, .. } => {
                Some(consensus_plan.as_str())
            }
            Self::Unstable { .. } => None,
        }
    }
}

/// The Layer 5 Patch Stability Harness.
///
/// Generates `config.n_samples` independent plan proposals for the same task
/// and measures their pairwise Jaccard variance to detect model instability.
pub struct StabilityHarness {
    client: Arc<AnthropicClient>,
    limiter: Option<Arc<AnthropicLimiter>>,
    breaker: Option<Arc<CircuitBreaker>>,
    /// Configuration controlling sample count and variance thresholds.
    pub config: StabilityConfig,
}

impl StabilityHarness {
    /// Create a new harness with the given API client and configuration.
    #[must_use]
    pub fn new(
        client: Arc<AnthropicClient>,
        limiter: Option<Arc<AnthropicLimiter>>,
        breaker: Option<Arc<CircuitBreaker>>,
        config: StabilityConfig,
    ) -> Self {
        Self {
            client,
            limiter,
            breaker,
            config,
        }
    }

    /// Generate `config.n_samples` plan proposals for `task` and return a
    /// `StabilityVerdict` based on pairwise variance.
    ///
    /// Partial samples (> 1 plan collected despite some failures) still produce
    /// a valid verdict. Only returns `Err` when every API call fails or the
    /// circuit breaker is already open before the first attempt.
    ///
    /// # Errors
    /// Returns an error when zero plans are collected (all calls failed).
    pub async fn assess(&self, task: &Task) -> Result<StabilityVerdict> {
        let prompt = build_stability_prompt(task);
        let mut plans = self.collect_samples(&prompt).await;

        if plans.is_empty() {
            anyhow::bail!(
                "stability harness: all {} API calls failed — no samples collected",
                self.config.n_samples
            );
        }

        // Single sample: trivially stable (no variance to compute).
        if plans.len() == 1 {
            let mut plans = plans;
            return Ok(StabilityVerdict::Stable {
                consensus_plan: plans.remove(0),
                variance_score: 0.0,
                n_samples: 1,
            });
        }

        let (variance_score, consensus_idx) = variance_and_consensus(&plans);
        let n_samples = plans.len();
        let consensus_plan = plans.swap_remove(consensus_idx);

        Ok(if variance_score <= self.config.stable_threshold {
            StabilityVerdict::Stable {
                consensus_plan,
                variance_score,
                n_samples,
            }
        } else if variance_score <= self.config.warning_threshold {
            StabilityVerdict::Warning {
                consensus_plan,
                variance_score,
                n_samples,
            }
        } else {
            StabilityVerdict::Unstable {
                variance_score,
                n_samples,
            }
        })
    }

    /// Collect up to `config.n_samples` plan texts, stopping early if the
    /// circuit breaker opens. Returns however many plans were successfully
    /// fetched (possibly fewer than `n_samples` on partial failure).
    async fn collect_samples(&self, prompt: &str) -> Vec<String> {
        let mut plans: Vec<String> = Vec::with_capacity(self.config.n_samples);
        for i in 0..self.config.n_samples {
            if self.breaker_is_open(i).await {
                break;
            }
            if let Some(l) = &self.limiter {
                l.acquire_request(4000.0).await;
            }
            match self
                .client
                .stream_plan(&self.config.model, LOPI_SYSTEM_PROMPT, prompt, |_| {})
                .await
            {
                Ok((text, usage)) => {
                    self.record_success(usage.estimated_cost(&self.config.model))
                        .await;
                    tracing::debug!(
                        sample = i,
                        chars = text.len(),
                        "stability: plan sample collected"
                    );
                    plans.push(text);
                }
                Err(e) => {
                    self.record_failure().await;
                    tracing::warn!(sample = i, error = %e, "stability: plan sample failed");
                }
            }
        }
        plans
    }

    /// Returns `true` if the circuit breaker is configured and currently open.
    async fn breaker_is_open(&self, sample_idx: usize) -> bool {
        if let Some(b) = &self.breaker {
            if b.check().await.is_err() {
                tracing::warn!(
                    sample = sample_idx,
                    "stability: circuit breaker open — stopping early"
                );
                return true;
            }
        }
        false
    }

    async fn record_success(&self, cost: f64) {
        if let Some(b) = &self.breaker {
            b.record_success().await;
            b.record_cost(cost).await;
        }
    }

    async fn record_failure(&self) {
        if let Some(b) = &self.breaker {
            b.record_failure().await;
        }
    }
}

/// Build the planning prompt for the stability harness.
///
/// Uses the same format as `runner::api_plan::build_user_prompt` so the
/// variance measurement is against the real planning prompt the agent would use.
/// Kept as a standalone function to avoid coupling to the private api_plan module.
pub(crate) fn build_stability_prompt(task: &Task) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(4);
    parts.push(format!("# Task\n{}", task.goal));

    if !task.constraints.is_empty() {
        parts.push(format!(
            "\n# Constraints\n- {}",
            task.constraints.join("\n- ")
        ));
    }
    if !task.allowed_dirs.is_empty() {
        parts.push(format!(
            "\n# Allowed dirs\n- {}",
            task.allowed_dirs.join("\n- ")
        ));
    }
    if !task.forbidden_dirs.is_empty() {
        parts.push(format!(
            "\n# Forbidden dirs\n- {}",
            task.forbidden_dirs.join("\n- ")
        ));
    }
    parts.push(
        "\nProduce a concise step-by-step plan to complete this task. \
         Each step should be a single edit or shell command."
            .to_string(),
    );
    parts.join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopi_core::{Priority, Task, TaskId, TaskSource};

    fn make_task(goal: &str) -> Task {
        Task {
            id: TaskId::new(),
            goal: goal.into(),
            constraints: vec!["must compile".into()],
            allowed_dirs: vec!["src/".into()],
            forbidden_dirs: vec![".github/".into()],
            priority: Priority::Normal,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            source: TaskSource::Cli,
            repo_path: None,
            output_schema: None,
            tools: Vec::new(),
            required_capabilities: Vec::new(),
            rubric: None,
            topology: None,
        }
    }

    #[test]
    fn build_stability_prompt_contains_goal() {
        let task = make_task("add retry logic to the HTTP client");
        let prompt = build_stability_prompt(&task);
        assert!(prompt.contains("add retry logic"));
        assert!(prompt.contains("# Task"));
        assert!(prompt.contains("# Constraints"));
        assert!(prompt.contains("# Allowed dirs"));
        assert!(prompt.contains("step-by-step plan"));
    }

    #[test]
    fn build_stability_prompt_is_deterministic() {
        let task = make_task("fix the parser bug");
        assert_eq!(build_stability_prompt(&task), build_stability_prompt(&task));
    }

    #[test]
    fn build_stability_prompt_omits_empty_sections() {
        let mut task = make_task("g");
        task.constraints = vec![];
        task.forbidden_dirs = vec![];
        let prompt = build_stability_prompt(&task);
        assert!(!prompt.contains("# Constraints"));
        assert!(!prompt.contains("# Forbidden dirs"));
    }

    #[test]
    fn stability_verdict_str() {
        let v = StabilityVerdict::Stable {
            consensus_plan: "plan".into(),
            variance_score: 0.1,
            n_samples: 5,
        };
        assert_eq!(v.as_str(), "stable");
        assert!((v.variance_score() - 0.1).abs() < f32::EPSILON);
        assert_eq!(v.consensus_plan(), Some("plan"));
    }

    #[test]
    fn unstable_verdict_has_no_plan() {
        let v = StabilityVerdict::Unstable {
            variance_score: 0.8,
            n_samples: 5,
        };
        assert_eq!(v.as_str(), "unstable");
        assert!(v.consensus_plan().is_none());
    }

    #[test]
    fn warning_verdict_str() {
        let v = StabilityVerdict::Warning {
            consensus_plan: "plan".into(),
            variance_score: 0.25,
            n_samples: 4,
        };
        assert_eq!(v.as_str(), "warning");
        assert_eq!(v.consensus_plan(), Some("plan"));
    }

    #[test]
    fn config_default_thresholds() {
        let cfg = StabilityConfig::default();
        assert_eq!(cfg.n_samples, 5);
        assert!((cfg.stable_threshold - 0.15).abs() < f32::EPSILON);
        assert!((cfg.warning_threshold - 0.35).abs() < f32::EPSILON);
    }
}
