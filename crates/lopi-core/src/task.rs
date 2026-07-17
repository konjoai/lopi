use crate::deliverable::Deliverable;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for a task — a newtype over `Uuid`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TaskId(pub Uuid);

impl TaskId {
    /// Generate a new random task ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Current lifecycle status of a [`Task`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// Task is waiting in the queue for an available agent.
    Queued,
    /// Agent is generating an implementation plan.
    Planning,
    /// Plan generated; paused awaiting human approval before implementation
    /// (Phase 11 — plan approval gate). The proposed plan rides on the
    /// accompanying [`crate::AgentEvent::PlanProposed`] event.
    AwaitingPlanApproval {
        /// Attempt whose plan is pending approval.
        attempt: u8,
    },
    /// Agent is applying code changes.
    Implementing,
    /// Agent is running the test suite.
    Testing,
    /// Agent is evaluating test and lint results.
    Scoring,
    /// Task is being retried after a previous failed attempt.
    Retrying {
        /// Attempt number of the upcoming retry.
        attempt: u8,
    },
    /// Task completed successfully and a branch (and optionally PR) was created.
    Success {
        /// Branch containing the successful changes.
        branch: String,
        /// URL of the opened pull request, if auto-PR is enabled.
        pr_url: Option<String>,
    },
    /// Task failed after exhausting all retry attempts.
    Failed {
        /// Human-readable description of why the task failed.
        reason: String,
    },
    /// Changes were rolled back after a failure.
    RolledBack,
    /// A pre-PR rebase onto the advanced default branch hit conflicts, so the
    /// task stopped rather than force an unsafe merge. Carries the conflicting
    /// paths so an operator — or a follow-up task — knows exactly what collided.
    Conflict {
        /// Repository-relative paths that conflicted during the rebase.
        paths: Vec<String>,
    },
}

impl TaskStatus {
    /// Canonical, machine-readable status string for durable persistence and
    /// the JSON API / WebSocket snapshot.
    ///
    /// This is the single lifecycle vocabulary that the store, the REST API,
    /// and the web client's status bucketing all agree on. Unlike a
    /// human-facing display label, it never embeds a reason, emoji, attempt
    /// count, or branch — persisting one of those (as the CLI/REPL paths once
    /// did via a display formatter) produced compound values like
    /// `"failed ❌ Cancelled"` that no consumer could bucket. Every write to
    /// the `tasks.status` column must go through this so a fresh page load,
    /// which only has the snapshot to read, buckets terminal tasks correctly.
    #[must_use]
    pub fn db_status(&self) -> &'static str {
        match self {
            TaskStatus::Queued => "queued",
            TaskStatus::Planning => "planning",
            TaskStatus::AwaitingPlanApproval { .. } => "awaiting_plan_approval",
            TaskStatus::Implementing => "implementing",
            TaskStatus::Testing => "testing",
            TaskStatus::Scoring => "scoring",
            TaskStatus::Retrying { .. } => "retrying",
            TaskStatus::Success { .. } => "success",
            TaskStatus::Failed { .. } => "failed",
            TaskStatus::RolledBack => "rolled_back",
            TaskStatus::Conflict { .. } => "conflict",
        }
    }
}

/// Scheduling priority for a [`Task`] in the agent queue.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Lowest priority — processed only when no higher-priority tasks are queued.
    Low = 0,
    /// Default priority for tasks submitted without an explicit override.
    Normal = 1,
    /// Elevated priority — processed before `Normal` tasks.
    High = 2,
    /// Highest priority — pre-empts all other queued tasks.
    Critical = 3,
}

/// Rubric used by the Konjo Verifier to grade an agent's output.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Rubric {
    /// Display name for this rubric (e.g. `"refactor_safety"`).
    pub name: String,
    /// Ordered criteria the verifier checks. Each entry is an imperative statement
    /// such as `"All existing tests still pass"`.
    pub criteria: Vec<String>,
}

impl Rubric {
    /// Parse a rubric from TOML source — the on-disk format used by
    /// `.konjo/rubrics/*.toml`.
    ///
    /// This is IO-free; callers read the file (e.g. via `tokio::fs`) and pass
    /// the contents here so the parse stays off any async-blocking path.
    ///
    /// # Errors
    ///
    /// Returns `Err` when the TOML is malformed or is missing the `name` /
    /// `criteria` fields.
    pub fn from_toml_str(s: &str) -> anyhow::Result<Self> {
        toml::from_str(s).map_err(Into::into)
    }
}

/// Verdict returned by the Konjo Verifier second-score pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierVerdict {
    /// Whether the agent's output satisfies all rubric criteria.
    pub passed: bool,
    /// Criteria not met — one sentence each explaining the gap.
    pub gaps: Vec<String>,
    /// Imperative fix hints ready for injection into the next retry's planning prompt.
    pub fix_hints: Vec<String>,
    /// Verifier confidence in the verdict, normalised to `[0.0, 1.0]`.
    pub confidence: f64,
}

/// A unit of work submitted to the lopi agent pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for this task.
    pub id: TaskId,
    /// Natural-language description of what the agent must achieve.
    pub goal: String,
    /// Additional hard constraints injected into the planning prompt.
    pub constraints: Vec<String>,
    /// Directories the agent is allowed to modify.
    pub allowed_dirs: Vec<String>,
    /// Directories the agent must never touch.
    pub forbidden_dirs: Vec<String>,
    /// Scheduling priority relative to other queued tasks.
    pub priority: Priority,
    /// Maximum number of retry attempts before the task is marked failed.
    pub max_retries: u8,
    /// Timestamp when the task was created.
    pub created_at: DateTime<Utc>,
    /// Origin that submitted this task.
    pub source: TaskSource,
    /// Override repository path for this task. Pool default is used when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<PathBuf>,
    /// P2 — capabilities the pool's dispatcher must satisfy before this task
    /// can be picked up. Empty (default)
    /// means "any agent can run this". Compared against
    /// `AgentPool::register_capabilities` tags via
    /// [`crate::Task::capabilities_satisfied_by`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<String>,
    /// P1.4 — Optional JSON Schema the agent's structured output must
    /// satisfy. Stored as raw `serde_json::Value` so callers can supply
    /// any schema shape; the validator in `lopi_core::schema` enforces a
    /// pragmatic subset (`type`, `required`, `properties`, `enum`).
    /// Validation failures are counted via `schema_violations_inc` and
    /// trigger an adaptive-retry cycle so the next plan prompt includes
    /// the violation message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// P2 — Allowlist of tool names this task may call. The names are
    /// looked up in `lopi-tools::ToolRegistry` at dispatch time. An empty
    /// vec means "no tools" — the agent stays in pure-CLI/API mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Sprint S — optional rubric for the Konjo Verifier second-score pass.
    /// When set, the verifier grades the diff against these criteria after
    /// the heuristic score passes. Falls back to a workspace default rubric
    /// when `None` and verifier mode is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rubric: Option<Rubric>,
    /// Sprint T — advisory orchestration topology. When `None`, the
    /// orchestrator's classifier proposes one at dispatch time. See
    /// [`crate::topology::TopologyHint`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<crate::topology::TopologyHint>,
    /// Phase 11 — when true, the agent surfaces its plan and pauses for human
    /// approval before implementation begins. Approve/reject arrives via
    /// `POST /api/tasks/:id/plan/{approve,reject}`.
    #[serde(default)]
    pub require_plan_approval: bool,
    /// Phase 16 (Loop Engineering) — trust level governing how far the loop may
    /// act without a human: L1 report-only … L4 auto-merge. Defaults to L2
    /// (draft PR), the conservative level inherited from a schedule or config.
    #[serde(default)]
    pub autonomy_level: crate::loop_config::AutonomyLevel,
    /// Report on Finish (Loop Engineering primitive 6) — channel name a
    /// completed run's summary is routed to (e.g. `"telegram"`), threaded
    /// from [`crate::config::ScheduleEntry::report`] the same way
    /// `autonomy_level` is. `None` (the default) changes nothing: the L1
    /// report-only hook only logs locally, as it always has. Validated via
    /// [`crate::report::ReportChannel::parse`] before use, never trusted
    /// as a raw literal.
    #[serde(default)]
    pub report: Option<String>,
    /// Verifier as Explicit Gate — force the Konjo Verifier second-score pass
    /// for this task, independent of `autonomy_level`. Mirrors
    /// [`crate::loop_config::LoopConfig::verifier_required`]. `false` (the
    /// default) leaves the only forcing mechanism as
    /// `autonomy_level >= VerifiedPr`, unchanged from before this field
    /// existed.
    #[serde(default)]
    pub verifier_required: bool,
    /// Model used for the verifier's grading pass. Mirrors
    /// [`crate::loop_config::LoopConfig::verifier_model`]. `None` (the
    /// default) resolves to a model that differs from the worker's — see
    /// `lopi_agent::verifier::resolve_verifier` — so the checker is never the
    /// same model as the maker.
    #[serde(default)]
    pub verifier_model: Option<String>,
    /// Reasoning-effort hint for the verifier's grading pass. Mirrors
    /// [`crate::loop_config::LoopConfig::verifier_effort`]. `None` (the
    /// default) omits the hint entirely.
    #[serde(default)]
    pub verifier_effort: Option<String>,
    /// Explicit worker-model override (e.g. `"claude-opus-4-7"`). `None` (the
    /// default) leaves model selection to `claude::select_model`'s
    /// complexity/retry heuristic, unchanged from before this field existed.
    /// An explicit value is always honored verbatim, mirroring
    /// `verifier_model`'s "explicit wins over the heuristic" precedent.
    #[serde(default)]
    pub model: Option<String>,
    /// Reasoning-effort level for the worker session (`"low"`/`"medium"`/
    /// `"high"`/`"xhigh"`/`"max"`). Passed to the CLI worker as `--effort`
    /// (validated in `ClaudeCode::with_effort`; an unrecognized value is
    /// dropped). Not injected into the direct-API path's system prompt —
    /// that's `cache_control: ephemeral` and must stay byte-identical to keep
    /// its cache-hit rate. `None` leaves the CLI default.
    #[serde(default)]
    pub effort: Option<String>,
    /// What kind of deliverable this goal is expected to produce, which
    /// governs whether a zero-diff attempt counts as success (intent-aware
    /// success — see [`crate::deliverable::Deliverable`]). `None` (the
    /// default) infers it from the goal text via
    /// [`Task::deliverable_kind`]; set explicitly to override the guess.
    #[serde(default)]
    pub deliverable: Option<Deliverable>,
    /// Per-task override of the hard iteration ceiling, taking precedence
    /// over the repo's `.lopi/loop.toml` [`crate::loop_config::LoopConfig::max_iterations`]
    /// when set. `0` is the infinite-loop sentinel (by design decision, not
    /// an `Option`-based ∞). `None` (the default) leaves the repo config —
    /// or its own default — as the sole ceiling, unchanged from before this
    /// field existed.
    #[serde(default)]
    pub max_iterations: Option<u8>,
    /// Per-task guardrail precondition, taking precedence over the repo's
    /// [`crate::loop_config::LoopConfig::gate`] when set. Mirrors
    /// `max_iterations`'s "explicit wins over the repo default" precedent.
    /// `None` (the default) leaves the repo's own `gate` (if any) as the
    /// sole precondition.
    #[serde(default)]
    pub gate: Option<String>,
    /// Per-task guardrail exit-condition, taking precedence over the repo's
    /// [`crate::loop_config::LoopConfig::until`] when set. `None` (the
    /// default) leaves the repo's own `until` (if any) — or scoring/
    /// `max_iterations` alone — as the sole stop condition.
    #[serde(default)]
    pub until: Option<String>,
    /// Per-task on-fail policy override, taking precedence over the repo's
    /// [`crate::loop_config::LoopConfig::on_fail`] when set. `None` (the
    /// default) defers to the repo's own `on_fail` (itself defaulting to
    /// [`crate::loop_config::OnFail::Stop`]).
    #[serde(default)]
    pub on_fail: Option<crate::loop_config::OnFail>,
    /// Backend-1 — opaque caller-supplied identity for this task, echoed
    /// back verbatim and persisted alongside it. Lets a client durably
    /// associate its own concept of "the thing that requested this task"
    /// (e.g. a loop-stack card id) with the [`TaskId`] the pool actually
    /// assigns, without lopi needing to understand what that concept is.
    /// `None` (the default) changes nothing for every existing caller.
    #[serde(default)]
    pub client_ref: Option<String>,
    /// Eval-Execution-1 (A1) — the machine-checkable success condition the
    /// tiered eval executor scores this loop against
    /// ([`crate::acceptance::Acceptance`]). `None` (the default) means no
    /// explicit goal is set, so the loop falls back to the legacy
    /// `score.passed()` gate — behavior is unchanged for every existing task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<crate::acceptance::Acceptance>,
    /// Eval-Execution-1 (A1) — operator opt-out of the fail-closed verifier.
    /// `false` (the default) is **fail-closed**: a verifier/judge error yields
    /// a not-passing verdict, never a silent pass. Set `true` only to
    /// deliberately restore the old fail-open behavior (proceed on error) for
    /// a low-trust loop where the operator accepts the risk.
    #[serde(default)]
    pub verifier_fail_open: bool,
    /// Progress-Gating (A3) — per-task token budget ceiling, taking precedence
    /// over the repo's [`crate::loop_config::LoopConfig::budget_tokens`] when
    /// non-zero (same "explicit wins over repo default" precedent as
    /// `max_iterations`). `0` (the default) inherits the repo/global budget.
    /// When it resolves to a positive value the loop meters cumulative token
    /// usage against it and stops with [`crate::StopReason::Budget`] on exceed.
    #[serde(default)]
    pub budget_tokens: u64,
    /// Budget & Guardrail Controls Part 3 — per-task override applied on top
    /// of the repo's [`crate::loop_config::LoopConfig::resolved_budget`]
    /// (`lopi run --budget`/`--budget-preset`/`--budget-tokens`, Telegram
    /// `/budget`). `None` (the default) leaves the repo's resolved budget as
    /// the sole source, unchanged from before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_override: Option<crate::budget_preset::BudgetOverride>,
}

/// Where a task originated — used for routing replies and audit logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskSource {
    /// Submitted via the `lopi run` command-line interface.
    Cli,
    /// Submitted by a Telegram bot message.
    Telegram {
        /// Telegram chat that sent the command.
        chat_id: i64,
        /// Message ID of the originating Telegram message.
        message_id: i32,
    },
    /// Injected by the GitHub webhook handler in response to a CI event.
    Webhook {
        /// Repository full name (e.g. `"org/repo"`).
        repo: String,
        /// GitHub event type that triggered the task (e.g. `"check_run"`).
        event: String,
    },
    /// Submitted via the REST API.
    Api,
    /// Approved self-modification task targeting lopi's own codebase.
    SelfModify {
        /// Identity or mechanism that approved the self-modification.
        approved_by: String,
    },
}

impl Task {
    /// Create a new `Task` with sensible defaults and `Normal` priority.
    #[must_use]
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            goal: goal.into(),
            constraints: vec![],
            allowed_dirs: vec!["src/".into(), "tests/".into()],
            forbidden_dirs: vec![".github/".into(), "infra/".into(), "Cargo.toml".into()],
            priority: Priority::Normal,
            max_retries: 3,
            created_at: Utc::now(),
            source: TaskSource::Cli,
            repo_path: None,
            output_schema: None,
            tools: Vec::new(),
            required_capabilities: Vec::new(),
            rubric: None,
            topology: None,
            require_plan_approval: false,
            autonomy_level: crate::loop_config::AutonomyLevel::default(),
            report: None,
            verifier_required: false,
            verifier_model: None,
            verifier_effort: None,
            model: None,
            effort: None,
            deliverable: None,
            max_iterations: None,
            gate: None,
            until: None,
            on_fail: None,
            client_ref: None,
            acceptance: None,
            verifier_fail_open: false,
            budget_tokens: 0,
            budget_override: None,
        }
    }

    /// Create a `Task` whose goal is resolved from a template string against
    /// `vars` at enqueue time (Prompt Templates, Sprint 1) — lopi fills the
    /// holes and Claude only ever sees the resolved literal; this is not a
    /// skill. The plain [`Task::new`] path is untouched and stays the default
    /// for callers with no template semantics.
    ///
    /// # Errors
    /// Returns [`crate::template::TemplateError`] when `template` has a
    /// `{name}` hole with no matching entry in `vars`.
    pub fn from_template(
        template: &str,
        vars: &BTreeMap<String, String>,
    ) -> Result<Self, crate::template::TemplateError> {
        crate::template::resolve(template, vars).map(Self::new)
    }

    /// True when every entry in `required_capabilities` appears in
    /// `provided`. Empty requirements vacuously satisfy.
    ///
    /// Used by `AgentPool::submit` to filter candidate agents before dispatch.
    #[must_use]
    pub fn capabilities_satisfied_by(&self, provided: &[String]) -> bool {
        self.required_capabilities
            .iter()
            .all(|req| provided.iter().any(|p| p == req))
    }

    /// Resolve this task's [`Deliverable`]: the explicit `deliverable` field
    /// when set, otherwise inferred from the goal text. Drives intent-aware
    /// success — whether a zero-diff attempt is a failure to retry or a valid
    /// review-only conclusion.
    #[must_use]
    pub fn deliverable_kind(&self) -> Deliverable {
        self.deliverable
            .unwrap_or_else(|| Deliverable::infer_from_goal(&self.goal))
    }
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
