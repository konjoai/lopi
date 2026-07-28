//! Pure, IO-free deterministic task-plan builder shared by
//! [`crate::generator::generate`] and [`crate::scenario::replay_events`], so
//! the persisted store and the TUI's event replay always describe the same
//! task list for a given seed. Nothing in this module touches the network,
//! filesystem, or a clock — every field here is a function of `seed` alone.

use lopi_core::{TaskId, TaskSource};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use uuid::Uuid;

use crate::content;

/// One task's deterministic shape for a given seed — no timestamps, no I/O.
/// [`crate::generator::generate`] persists these with real/offset
/// timestamps; [`crate::scenario::replay_events`] turns them into a
/// synthetic `AgentEvent` sequence instead.
pub(crate) struct TaskPlan {
    /// Deterministic task id for this seed.
    pub id: TaskId,
    /// Goal text, copied from `content::GOALS`.
    pub goal: String,
    /// Index into `content::REPOS`.
    pub repo_index: usize,
    /// Where this task originated.
    pub source: TaskSource,
    /// Target lifecycle outcome.
    pub outcome: TaskOutcome,
}

/// Target lifecycle outcome for a [`TaskPlan`] — mirrors the six
/// `tasks.status` values the real system ever persists to the durable
/// store (see `docs/adr/0001-demo-mode-and-measurement.md` point 3: the
/// fine-grained `Planning`/`Implementing`/`Testing`/`Scoring`/`Retrying`/
/// `AwaitingPlanApproval` states are event-only, never written to
/// `tasks.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskOutcome {
    /// Waiting in the queue, not yet started.
    Queued,
    /// Currently executing.
    Running,
    /// Completed successfully.
    Success,
    /// Failed after exhausting retries.
    Failed,
    /// Rolled back after a failure.
    RolledBack,
    /// Stopped on a pre-PR rebase conflict.
    Conflict,
}

impl TaskOutcome {
    /// Canonical `tasks.status` string for this outcome.
    pub(crate) fn db_status(self) -> &'static str {
        match self {
            TaskOutcome::Queued => "queued",
            TaskOutcome::Running => "running",
            TaskOutcome::Success => "success",
            TaskOutcome::Failed => "failed",
            TaskOutcome::RolledBack => "rolled_back",
            TaskOutcome::Conflict => "conflict",
        }
    }

    /// Whether this outcome represents a terminal state (anything but
    /// `Queued`/`Running`).
    pub(crate) fn is_terminal(self) -> bool {
        !matches!(self, TaskOutcome::Queued | TaskOutcome::Running)
    }
}

/// Fixed distribution of outcomes across the demo's ~22 tasks — covers
/// every [`TaskOutcome`] variant at least once and reads as "enough that
/// the list scrolls", per the sprint spec.
fn outcome_distribution() -> Vec<TaskOutcome> {
    let mut outcomes = Vec::with_capacity(22);
    outcomes.extend(std::iter::repeat_n(TaskOutcome::Queued, 3));
    outcomes.extend(std::iter::repeat_n(TaskOutcome::Running, 4));
    outcomes.extend(std::iter::repeat_n(TaskOutcome::Success, 8));
    outcomes.extend(std::iter::repeat_n(TaskOutcome::Failed, 4));
    outcomes.extend(std::iter::repeat_n(TaskOutcome::RolledBack, 1));
    outcomes.extend(std::iter::repeat_n(TaskOutcome::Conflict, 2));
    outcomes
}

/// Deterministically pick a [`TaskSource`] for one plan. Never picks
/// `TaskSource::Telegram` — that transport was removed (ADR point 1) and
/// using it here would misleadingly suggest it's still live.
fn pick_source(rng: &mut StdRng, repo_path: &str) -> TaskSource {
    match rng.gen_range(0..3) {
        0 => TaskSource::Cli,
        1 => TaskSource::Webhook {
            repo: repo_path.to_string(),
            event: "check_run".to_string(),
        },
        _ => TaskSource::Api,
    }
}

/// Build the deterministic task-plan list for `seed` — pure, no I/O, no
/// timestamps. The same `seed` always returns byte-identical plans (same
/// goals, repo assignments, sources, ids, and outcome distribution).
pub(crate) fn build_task_plans(seed: u64) -> Vec<TaskPlan> {
    let mut rng = StdRng::seed_from_u64(seed);

    let mut goal_indices: Vec<usize> = (0..content::GOALS.len()).collect();
    goal_indices.shuffle(&mut rng);

    let mut outcomes = outcome_distribution();
    outcomes.shuffle(&mut rng);

    outcomes
        .into_iter()
        .enumerate()
        .map(|(i, outcome)| {
            let goal_idx = goal_indices[i % goal_indices.len()];
            let template = &content::GOALS[goal_idx];
            let repo_path = content::REPOS[template.repo_index].path;
            let id = TaskId(Uuid::from_bytes(rng.gen()));
            let source = pick_source(&mut rng, repo_path);
            TaskPlan {
                id,
                goal: template.goal.to_string(),
                repo_index: template.repo_index,
                source,
                outcome,
            }
        })
        .collect()
}
