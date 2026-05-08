use crate::task::TaskId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Per-turn observability record emitted after each claude invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnMetrics {
    pub turn_id: Uuid,
    pub task_id: TaskId,
    pub session_id: Uuid,
    pub model: String,
    pub attempt_number: u8,
    // Token accounting
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_input_tokens: u32,
    pub cache_write_input_tokens: u32,
    // Latency
    pub ttft_ms: u64,
    pub turn_latency_ms: u64,
    pub tool_execution_ms: u64,
    // Context state
    pub context_tokens: u32,
    pub context_pressure: f32,
    pub evictions_this_turn: u8,
    // Tool calls
    pub tool_calls: u8,
    pub tools_parallel: bool,
    // Cost
    pub estimated_cost_usd: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Planning,
    Implementing,
    Testing,
    Scoring,
    Done,
    Errored,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    pub test_pass_rate: f32,
    pub lint_errors: u32,
    pub diff_lines: u32,
    pub errors: Vec<String>,
}

impl Score {
    #[must_use]
    pub fn new(test_pass_rate: f32, lint_errors: u32, diff_lines: u32) -> Self {
        Self {
            test_pass_rate,
            lint_errors,
            diff_lines,
            errors: vec![],
        }
    }

    #[must_use]
    pub fn passed(&self) -> bool {
        self.test_pass_rate >= 1.0 && self.lint_errors == 0
    }

    #[must_use]
    pub fn weighted(&self) -> f32 {
        // Higher is better. Pass rate dominates; lint errors and oversized diffs penalize.
        // u32→f32 precision loss is intentional: scores are relative metrics, not exact counts.
        #[allow(clippy::cast_precision_loss)]
        let lint_penalty = (self.lint_errors as f32 * 0.05).min(0.5);
        #[allow(clippy::cast_precision_loss)]
        let size_penalty = ((self.diff_lines as f32 / 1000.0) * 0.1).min(0.3);
        (self.test_pass_rate - lint_penalty - size_penalty).max(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub id: Uuid,
    pub task_id: TaskId,
    pub attempt_num: u8,
    pub branch: String,
    pub score: Option<Score>,
    pub outcome: String,
    pub created_at: DateTime<Utc>,
}

impl Attempt {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: Uuid,
    pub task_id: TaskId,
    pub state: AgentState,
    pub attempts: Vec<Attempt>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl AgentRun {
    #[must_use]
    pub fn new(task_id: TaskId) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            state: AgentState::Idle,
            attempts: vec![],
            started_at: Utc::now(),
            finished_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_weighted_perfect() {
        let s = Score::new(1.0, 0, 0);
        assert!(
            (s.weighted() - 1.0).abs() < 0.001,
            "perfect score should be 1.0"
        );
    }

    #[test]
    fn score_weighted_lint_penalty() {
        // 4 errors × 0.05 = 0.20 penalty
        let s = Score::new(1.0, 4, 0);
        assert!((s.weighted() - 0.8).abs() < 0.001);
    }

    #[test]
    fn score_weighted_size_penalty() {
        // 1000 lines → (1000/1000) × 0.1 = 0.10 penalty
        let s = Score::new(1.0, 0, 1000);
        assert!((s.weighted() - 0.9).abs() < 0.001);
    }

    #[test]
    fn score_weighted_lint_penalty_caps_at_half() {
        // 20 errors × 0.05 = 1.0, capped at 0.5
        let s = Score::new(1.0, 20, 0);
        assert!((s.weighted() - 0.5).abs() < 0.001);
    }

    #[test]
    fn score_weighted_size_penalty_caps_at_0_3() {
        // 5000 lines → (5.0) × 0.1 = 0.5, capped at 0.3
        let s = Score::new(1.0, 0, 5000);
        assert!((s.weighted() - 0.7).abs() < 0.001);
    }

    #[test]
    fn score_weighted_combined_penalties() {
        // pass_rate=0.8, lint=2 (penalty 0.10), size=500 (penalty 0.05)
        let s = Score::new(0.8, 2, 500);
        let expected = 0.8 - 0.10 - 0.05;
        assert!((s.weighted() - expected).abs() < 0.001);
    }
}
