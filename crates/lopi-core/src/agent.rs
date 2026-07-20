use crate::task::TaskId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Per-turn observability record emitted after each claude invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnMetrics {
    /// Unique identifier for this individual turn.
    pub turn_id: Uuid,
    /// Task this turn belongs to.
    pub task_id: TaskId,
    /// Claude session identifier for the enclosing agent run.
    pub session_id: Uuid,
    /// Model name used for this turn (e.g. `claude-sonnet-4-6`).
    pub model: String,
    /// Attempt number within the parent task run.
    pub attempt_number: u8,
    // Token accounting
    /// Tokens consumed in the input (prompt) portion of this turn.
    pub input_tokens: u32,
    /// Tokens produced in the output (completion) portion of this turn.
    pub output_tokens: u32,
    /// Prompt tokens served from the context cache.
    pub cache_read_input_tokens: u32,
    /// Prompt tokens written into the context cache.
    pub cache_write_input_tokens: u32,
    // Latency
    /// Time-to-first-token in milliseconds.
    pub ttft_ms: u64,
    /// Wall-clock duration of the full turn in milliseconds.
    pub turn_latency_ms: u64,
    /// Combined execution time of all tool calls in this turn, in milliseconds.
    pub tool_execution_ms: u64,
    // Context state
    /// Total tokens currently in the context window.
    pub context_tokens: u32,
    /// Fraction of the context window currently in use (`0.0`–`1.0`).
    pub context_pressure: f32,
    /// Number of messages evicted from context during this turn.
    pub evictions_this_turn: u8,
    // Tool calls
    /// Number of tool calls made during this turn.
    pub tool_calls: u8,
    /// Whether any tool calls in this turn were issued in parallel.
    pub tools_parallel: bool,
    // Cost
    /// Estimated USD cost of this turn based on token counts.
    pub estimated_cost_usd: f64,
    /// Wall-clock time when this turn was recorded.
    pub timestamp: DateTime<Utc>,
}

/// Tunable penalties applied by [`Score::weighted`] to derive a composite quality score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreWeights {
    /// Score penalty subtracted per lint error (default `0.05`).
    #[serde(default = "ScoreWeights::default_lint_penalty_per_error")]
    pub lint_penalty_per_error: f32,
    /// Maximum total lint penalty that can be applied (default `0.50`).
    #[serde(default = "ScoreWeights::default_lint_penalty_cap")]
    pub lint_penalty_cap: f32,
    /// Score penalty per 1 000 diff lines added (default `0.10`).
    #[serde(default = "ScoreWeights::default_diff_penalty_per_kloc")]
    pub diff_penalty_per_kloc: f32,
    /// Maximum total diff-size penalty that can be applied (default `0.30`).
    #[serde(default = "ScoreWeights::default_diff_penalty_cap")]
    pub diff_penalty_cap: f32,
}

impl ScoreWeights {
    fn default_lint_penalty_per_error() -> f32 {
        0.05
    }
    fn default_lint_penalty_cap() -> f32 {
        0.50
    }
    fn default_diff_penalty_per_kloc() -> f32 {
        0.10
    }
    fn default_diff_penalty_cap() -> f32 {
        0.30
    }
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            lint_penalty_per_error: Self::default_lint_penalty_per_error(),
            lint_penalty_cap: Self::default_lint_penalty_cap(),
            diff_penalty_per_kloc: Self::default_diff_penalty_per_kloc(),
            diff_penalty_cap: Self::default_diff_penalty_cap(),
        }
    }
}

/// Quality score produced after a test-and-lint cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    /// Fraction of tests that passed, in the range `[0.0, 1.0]`.
    pub test_pass_rate: f32,
    /// Number of lint errors reported by the linter.
    pub lint_errors: u32,
    /// Total lines changed in the diff.
    pub diff_lines: u32,
    /// Human-readable error messages collected during scoring.
    pub errors: Vec<String>,
}

impl Score {
    /// Construct a new `Score` with an empty error list.
    #[must_use]
    pub fn new(test_pass_rate: f32, lint_errors: u32, diff_lines: u32) -> Self {
        Self {
            test_pass_rate,
            lint_errors,
            diff_lines,
            errors: vec![],
        }
    }

    /// Returns `true` when all tests pass and there are zero lint errors.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.test_pass_rate >= 1.0 && self.lint_errors == 0
    }

    /// Compute a composite quality score in `[0.0, 1.0]` using the given penalty weights.
    #[must_use]
    pub fn weighted(&self, weights: &ScoreWeights) -> f32 {
        // Higher is better. Pass rate dominates; lint errors and oversized diffs penalize.
        // u32→f32 precision loss is intentional: scores are relative metrics, not exact counts.
        #[allow(clippy::cast_precision_loss)]
        let lint_penalty = (self.lint_errors as f32 * weights.lint_penalty_per_error)
            .min(weights.lint_penalty_cap);
        #[allow(clippy::cast_precision_loss)]
        let size_penalty = ((self.diff_lines as f32 / 1000.0) * weights.diff_penalty_per_kloc)
            .min(weights.diff_penalty_cap);
        (self.test_pass_rate - lint_penalty - size_penalty).max(0.0)
    }
}

/// One execution attempt for a task, representing a single branch-and-score cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    /// Unique identifier for this attempt.
    pub id: Uuid,
    /// Task this attempt belongs to.
    pub task_id: TaskId,
    /// Sequential attempt number, starting at 1.
    pub attempt_num: u8,
    /// Git branch name created for this attempt.
    pub branch: String,
    /// Score produced at the end of this attempt, if available.
    pub score: Option<Score>,
    /// Final outcome string (e.g. `"pending"`, `"success"`, `"failed"`).
    pub outcome: String,
    /// Timestamp when this attempt was created.
    pub created_at: DateTime<Utc>,
}

impl Attempt {
    /// Create a new `Attempt` in the `"pending"` state for the given task and branch.
    pub fn new(task_id: TaskId, attempt_num: u8, branch: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            attempt_num,
            branch: branch.into(),
            score: None,
            outcome: "pending".into(),
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_weighted_perfect() {
        let s = Score::new(1.0, 0, 0);
        let weights = ScoreWeights::default();
        assert!(
            (s.weighted(&weights) - 1.0).abs() < 0.001,
            "perfect score should be 1.0"
        );
    }

    #[test]
    fn score_weighted_lint_penalty() {
        // 4 errors × 0.05 = 0.20 penalty
        let s = Score::new(1.0, 4, 0);
        let weights = ScoreWeights::default();
        assert!((s.weighted(&weights) - 0.8).abs() < 0.001);
    }

    #[test]
    fn score_weighted_size_penalty() {
        // 1000 lines → (1000/1000) × 0.1 = 0.10 penalty
        let s = Score::new(1.0, 0, 1000);
        let weights = ScoreWeights::default();
        assert!((s.weighted(&weights) - 0.9).abs() < 0.001);
    }

    #[test]
    fn score_weighted_lint_penalty_caps_at_half() {
        // 20 errors × 0.05 = 1.0, capped at 0.5
        let s = Score::new(1.0, 20, 0);
        let weights = ScoreWeights::default();
        assert!((s.weighted(&weights) - 0.5).abs() < 0.001);
    }

    #[test]
    fn score_weighted_size_penalty_caps_at_0_3() {
        // 5000 lines → (5.0) × 0.1 = 0.5, capped at 0.3
        let s = Score::new(1.0, 0, 5000);
        let weights = ScoreWeights::default();
        assert!((s.weighted(&weights) - 0.7).abs() < 0.001);
    }

    #[test]
    fn score_weighted_combined_penalties() {
        // pass_rate=0.8, lint=2 (penalty 0.10), size=500 (penalty 0.05)
        let s = Score::new(0.8, 2, 500);
        let weights = ScoreWeights::default();
        let expected = 0.8 - 0.10 - 0.05;
        assert!((s.weighted(&weights) - expected).abs() < 0.001);
    }
}
