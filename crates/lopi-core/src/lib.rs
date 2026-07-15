//! Core shared types for the lopi agent orchestrator.
//!
//! Exposes `Task`, `AgentRun`, `Score`, `LopiConfig`, and supporting types
//! used across all lopi crates.

/// Eval-Execution-1 (A1) — the goal/acceptance object (cross-cutting seam #1):
/// a tier-tagged, machine-checkable success condition for a loop or a stack.
pub mod acceptance;
/// Agent execution state machine and scoring primitives.
pub mod agent;
/// Token-budget tracking and scope definitions.
pub mod budget;
/// Global and per-repo configuration structures.
pub mod config;
/// Earned-trust auto-promotion state machine (Phase 16.7).
pub mod earned_trust;
/// Eval-Execution-1 (A1) — the one eval-result object (cross-cutting seam #3),
/// shaped for its three consumers (A2 reflection, A3 ratchet, A3/B1 stop).
pub mod eval_outcome;
/// Broadcast event types for TUI, WebSocket, and log panels.
pub mod event;
/// Progress-Gating (A3) — the gain gate: accept an iteration only when it is a
/// genuine gain over best, objective-primary so judge noise cannot lock.
pub mod gain;
/// Loop-engineering configuration: autonomy levels + the `LoopConfig` schema.
pub mod loop_config;
/// Claude model catalog types + the static fallback list `GET /api/models`
/// serves when the live Anthropic call fails. Single source of truth for
/// model IDs, replacing the three independently-drifted copies this repo
/// carried before (`lopi-agent::claude` constants, web's `options.ts`,
/// macOS's `LaunchControls`/`StackConfigTypes`).
pub mod models;
/// Report on Finish (Loop Engineering primitive 6) — the `report` channel
/// name a completed run's summary can be routed to.
pub mod report;
/// Structured output schema validation (JSON Schema subset).
pub mod schema;
/// Constant-time comparison for secrets (bearer tokens, webhook signatures).
pub mod security;
/// Self-prompting loop strategies — how an agent re-prompts itself on retry.
pub mod self_prompt;
/// Progress-Gating (A3) — the specific reason a loop terminated and the
/// precedence among reasons when several trip at once.
pub mod stop_reason;
/// Task definition, status, priority, and source types.
pub mod task;
/// Literal-string prompt templates with named `{hole}` markers, resolved
/// against a variable map at enqueue time (Prompt Templates, Sprint 1).
pub mod template;
/// Customer tier classification.
pub mod tier;
/// Orchestration topology hints (Sprint T).
pub mod topology;

pub use acceptance::{Acceptance, AcceptanceCheck, CheckSpec, EvalTier, MetricGate, Op};
pub use agent::{AgentRun, AgentState, Attempt, Score, ScoreWeights, TurnMetrics};
pub use budget::BudgetScope;
pub use config::{LimitWindow, LopiConfig, MaxxEntry, RepoProfile, ScheduleEntry};
pub use earned_trust::EarnedTrust;
pub use eval_outcome::{CheckResult, EvalOutcome, Verdict};
pub use event::{AgentEvent, EventBus, LogLevel, PlanDecision};
pub use gain::{GainDecision, GainRule, GainSample};
pub use loop_config::{AutonomyLevel, IsolationMode, LoopConfig};
pub use models::{fallback_models, ModelInfo};
pub use report::{ReportChannel, ReportChannelError};
pub use schema::{
    schema_violations_inc, schema_violations_snapshot, validate as validate_schema,
    Violation as SchemaViolation, ViolationKind as SchemaViolationKind,
};
pub use security::constant_time_eq;
pub use self_prompt::SelfPromptStrategy;
pub use stop_reason::StopReason;
pub use task::{Priority, Rubric, Task, TaskId, TaskSource, TaskStatus, VerifierVerdict};
pub use template::{resolve as resolve_template, TemplateError};
pub use tier::CustomerTier;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn task_id_is_unique() {
        let a = TaskId::new();
        let b = TaskId::new();
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn task_new_defaults() {
        let t = Task::new("fix the bug");
        assert_eq!(t.goal, "fix the bug");
        assert_eq!(t.priority, Priority::Normal);
        assert_eq!(t.max_retries, 3);
        assert!(t.allowed_dirs.contains(&"src/".to_string()));
        assert!(t.forbidden_dirs.contains(&".github/".to_string()));
        assert!(matches!(t.source, TaskSource::Cli));
        assert!(t.required_capabilities.is_empty(), "default = no caps");
        assert!(t.report.is_none(), "no report channel by default");
        assert!(!t.verifier_required, "verifier not required by default");
        assert!(
            t.verifier_model.is_none(),
            "no verifier model override by default"
        );
        assert!(
            t.verifier_effort.is_none(),
            "no verifier effort hint by default"
        );
    }

    /// Verifier as Explicit Gate — a `Task` JSON payload predating these
    /// three fields must still deserialize, with the same conservative
    /// defaults `Task::new` produces.
    #[test]
    fn task_deserializes_when_verifier_fields_are_absent() {
        let t = Task::new("legacy payload");
        let mut json = serde_json::to_value(&t).unwrap();
        let obj = json.as_object_mut().unwrap();
        obj.remove("verifier_required");
        obj.remove("verifier_model");
        obj.remove("verifier_effort");
        let back: Task = serde_json::from_value(json).unwrap();
        assert!(!back.verifier_required);
        assert!(back.verifier_model.is_none());
        assert!(back.verifier_effort.is_none());
    }

    #[test]
    fn capabilities_satisfied_by_handles_subset_match() {
        let mut t = Task::new("needs rust + git");
        t.required_capabilities = vec!["rust".into(), "git".into()];
        assert!(t.capabilities_satisfied_by(&["rust".into(), "git".into(), "extra".into()]));
        // Missing one → not satisfied.
        assert!(!t.capabilities_satisfied_by(&["rust".into()]));
        // Empty provided → not satisfied for non-empty requirements.
        assert!(!t.capabilities_satisfied_by(&[]));
        // Empty requirements vacuously satisfied.
        let plain = Task::new("plain task");
        assert!(plain.capabilities_satisfied_by(&[]));
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }

    #[test]
    fn score_passed_requires_full_pass_rate_and_no_lint() {
        let s = Score::new(1.0, 0, 10);
        assert!(s.passed());
        let s2 = Score::new(0.9, 0, 10);
        assert!(!s2.passed());
        let s3 = Score::new(1.0, 1, 10);
        assert!(!s3.passed());
    }

    #[test]
    fn score_weighted_clamps_to_zero() {
        let mut s = Score::new(0.0, 100, 100_000);
        s.errors = vec!["bad".into()];
        let weights = ScoreWeights::default();
        assert!(s.weighted(&weights) >= 0.0);
    }

    #[test]
    fn score_weights_default_matches_legacy() {
        let w = ScoreWeights::default();
        assert_eq!(w.lint_penalty_per_error, 0.05);
        assert_eq!(w.lint_penalty_cap, 0.50);
        assert_eq!(w.diff_penalty_per_kloc, 0.10);
        assert_eq!(w.diff_penalty_cap, 0.30);
    }

    #[test]
    fn score_weighted_with_custom_weights() {
        let s = Score::new(0.9, 5, 2000);
        let default_weights = ScoreWeights::default();
        let relaxed_weights = ScoreWeights {
            lint_penalty_per_error: 0.01,
            lint_penalty_cap: 0.10,
            diff_penalty_per_kloc: 0.02,
            diff_penalty_cap: 0.10,
        };

        let default_score = s.weighted(&default_weights);
        let relaxed_score = s.weighted(&relaxed_weights);
        assert!(
            relaxed_score > default_score,
            "relaxed weights should produce higher scores"
        );
    }

    #[test]
    fn attempt_new_has_pending_outcome() {
        let tid = TaskId::new();
        let a = Attempt::new(tid, 1, "lopi/abc-attempt-1");
        assert_eq!(a.outcome, "pending");
        assert_eq!(a.attempt_num, 1);
        assert_eq!(a.branch, "lopi/abc-attempt-1");
    }

    #[test]
    fn agent_run_starts_idle() {
        let tid = TaskId::new();
        let run = AgentRun::new(tid);
        assert!(matches!(run.state, AgentState::Idle));
        assert!(run.attempts.is_empty());
        assert!(run.finished_at.is_none());
    }

    #[test]
    fn task_id_display() {
        let id = TaskId::new();
        let s = format!("{id}");
        assert_eq!(s.len(), 36);
    }

    #[test]
    fn task_source_serde_round_trip() {
        let s = TaskSource::Telegram {
            chat_id: 12345,
            message_id: 99,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: TaskSource = serde_json::from_str(&json).unwrap();
        match back {
            TaskSource::Telegram {
                chat_id,
                message_id,
            } => {
                assert_eq!(chat_id, 12345);
                assert_eq!(message_id, 99);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn task_source_selfmodify_serde_round_trip() {
        let s = TaskSource::SelfModify {
            approved_by: "config".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: TaskSource = serde_json::from_str(&json).unwrap();
        match back {
            TaskSource::SelfModify { approved_by } => {
                assert_eq!(approved_by, "config");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn task_status_serde_round_trip() {
        let st = TaskStatus::Success {
            branch: "lopi/test-attempt-1".into(),
            pr_url: Some("https://github.com/org/repo/pull/1".into()),
        };
        let json = serde_json::to_string(&st).unwrap();
        let back: TaskStatus = serde_json::from_str(&json).unwrap();
        match back {
            TaskStatus::Success { branch, pr_url } => {
                assert_eq!(branch, "lopi/test-attempt-1");
                assert_eq!(pr_url.unwrap(), "https://github.com/org/repo/pull/1");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn created_at_is_recent() {
        let before = Utc::now();
        let t = Task::new("goal");
        let after = Utc::now();
        assert!(t.created_at >= before);
        assert!(t.created_at <= after);
    }

    #[test]
    fn event_bus_broadcast_and_receive() {
        let bus: EventBus<String> = EventBus::new(16);
        let mut rx = bus.subscribe();
        bus.send("hello".to_string());
        assert_eq!(rx.try_recv().unwrap(), "hello");
    }

    #[test]
    fn agent_event_log_helpers() {
        let tid = TaskId::new();
        let ev = AgentEvent::info(tid, "test message");
        match ev {
            AgentEvent::LogLine { line, .. } => assert_eq!(line, "test message"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn agent_event_serde_round_trip() {
        let tid = TaskId::new();
        let ev = AgentEvent::info(tid, "hello from agent");
        let json = serde_json::to_string(&ev).unwrap();
        let back: AgentEvent = serde_json::from_str(&json).unwrap();
        match back {
            AgentEvent::LogLine { line, .. } => assert_eq!(line, "hello from agent"),
            _ => panic!("wrong variant"),
        }
    }
}
