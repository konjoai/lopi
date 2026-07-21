//! Successor tasks — Sprint Successor-1: the data model for an
//! **agent-authored, cross-task-boundary** follow-up, as distinct from
//! [`crate::self_prompt`]'s framework-authored, same-task retry reframing.
//!
//! A [`Successor`] is a proposal: "after this task finishes, run this next
//! goal." It carries no execution machinery of its own — this sprint stops at
//! the data model plus the containment gates in
//! [`crate::task::derive_successor_task`]; parsing a `Successor` out of an
//! agent's own final output is explicitly out of scope (Sprint Successor-2).

use crate::autonomy::AutonomyLevel;
use crate::task::{Task, TaskSource};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Upper bound (in bytes) on [`Successor::goal`]'s length. A goal beyond this
/// is rejected by [`Successor::validate`] rather than silently truncated —
/// truncating a goal string could quietly change what the successor task
/// actually does.
pub const MAX_GOAL_LEN: usize = 2_000;

/// Default depth cap passed to [`derive_successor_task`] by the finalize-path
/// wiring (`lopi-agent`'s `AgentRunner::derive_and_stash_successor`). A
/// per-repo `.lopi/loop.toml` ceiling is a natural Sprint Successor-2/3
/// extension once chains actually run unattended for a while — out of scope
/// this sprint, whose own gate (`Task::successor_enabled`, defaulting
/// `false`) already stops any chain from starting without one hop's worth of
/// explicit opt-in.
pub const DEFAULT_MAX_CHAIN_DEPTH: u8 = 3;

/// An agent-authored proposal for a follow-up task, to run after the current
/// one reaches a terminal state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct Successor {
    /// Natural-language goal for the successor task.
    pub goal: String,
    /// Which terminal outcome of the parent task should spawn this successor.
    pub when: SuccessorCondition,
    /// Why the agent proposed this follow-up — surfaced to a human reviewer,
    /// never executed or interpreted.
    pub rationale: String,
    /// Directories the successor is allowed to touch, proposed by the agent.
    /// [`crate::task::derive_successor_task`] intersects this against the
    /// parent's own `allowed_dirs` (when the parent's is non-empty) — a
    /// successor can never propose its way into a directory its parent
    /// itself could not reach.
    #[serde(default)]
    pub allowed_dirs: Vec<String>,
}

/// Which of the parent task's terminal outcomes should spawn a [`Successor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuccessorCondition {
    /// Spawn only when the parent task reaches `TaskStatus::Success`.
    OnSuccess,
    /// Spawn only when the parent task reaches a failing terminal state.
    OnFailure,
    /// Spawn regardless of the parent task's terminal outcome.
    Always,
}

impl SuccessorCondition {
    /// Parse a serialized condition (`"on_success"`, `"success"`, …).
    /// Case-insensitive; accepts the canonical snake_case name and a couple
    /// of shorthand aliases, mirroring
    /// [`SelfPromptStrategy::parse`](crate::self_prompt::SelfPromptStrategy::parse).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "on_success" | "onsuccess" | "success" => Some(Self::OnSuccess),
            "on_failure" | "onfailure" | "failure" | "fail" => Some(Self::OnFailure),
            "always" => Some(Self::Always),
            _ => None,
        }
    }
}

/// Why a [`Successor`] failed validation — a named, explained rejection,
/// never a silent drop. Mirrors [`crate::report::ReportChannelError`]'s
/// pattern.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SuccessorError {
    /// The goal is empty (or whitespace-only) — there is nothing for the
    /// successor task to do.
    #[error("successor goal is empty")]
    EmptyGoal,
    /// The goal exceeds [`MAX_GOAL_LEN`] bytes.
    #[error("successor goal is {len} bytes, exceeding the {max}-byte limit")]
    GoalTooLong {
        /// Actual byte length of the offending goal.
        len: usize,
        /// The limit that was exceeded ([`MAX_GOAL_LEN`]).
        max: usize,
    },
}

impl Successor {
    /// Validate this successor's goal: non-empty and within
    /// [`MAX_GOAL_LEN`]. Does not touch `when`/`rationale`/`allowed_dirs` —
    /// those have no invalid representation at this layer.
    ///
    /// # Errors
    /// Returns [`SuccessorError::EmptyGoal`] for an empty/whitespace-only
    /// goal, or [`SuccessorError::GoalTooLong`] when it exceeds
    /// [`MAX_GOAL_LEN`] bytes.
    pub fn validate(&self) -> Result<(), SuccessorError> {
        if self.goal.trim().is_empty() {
            return Err(SuccessorError::EmptyGoal);
        }
        if self.goal.len() > MAX_GOAL_LEN {
            return Err(SuccessorError::GoalTooLong {
                len: self.goal.len(),
                max: MAX_GOAL_LEN,
            });
        }
        Ok(())
    }
}

// ── Containment gates (Sprint Successor-1, Phase 2) ─────────────────────────
//
// `derive_successor_task` is the one place a task can spawn another task
// this sprint. It lives here (lopi-core), not in `lopi-orchestrator::
// task_build`, because `lopi-agent`'s finalize path is the caller — and
// `lopi-orchestrator` already depends on `lopi-agent`, not the other way
// around, so putting the containment logic in `lopi-orchestrator` would
// require a dependency cycle to call it from `finalize.rs`. Every crate that
// needs it already depends on `lopi-core`.

/// Gate 2 — autonomy ceiling: a successor's autonomy level is never
/// higher-ranked than its parent's, regardless of what was requested for it.
/// A successor may narrow the parent's trust, never widen it.
#[must_use]
pub fn clamp_autonomy_to_parent(
    parent_level: AutonomyLevel,
    requested_level: AutonomyLevel,
) -> AutonomyLevel {
    AutonomyLevel::from_rank(parent_level.rank().min(requested_level.rank()))
}

/// Gate 4 — whether `source` is untrusted input: a webhook-triggered CI
/// event or an inbound Telegram message, as opposed to a human at the
/// CLI/API or an already-approved self-modification. A chain seeded by
/// untrusted input must never self-extend past one hop without a human
/// looking at the plan first.
#[must_use]
pub fn is_untrusted_source(source: &TaskSource) -> bool {
    matches!(source, TaskSource::Webhook { .. } | TaskSource::Telegram { .. })
}

/// Union two directory lists, preserving `a`'s order and skipping any of
/// `b`'s entries already present. Small lists (goal-scoped directory
/// allowlists, never large) — `O(n*m)` `Vec::contains` is the right call
/// over pulling in a `HashSet` for a handful of path strings.
fn union_dirs(a: &[String], b: &[String]) -> Vec<String> {
    let mut out = a.to_vec();
    for d in b {
        if !out.contains(d) {
            out.push(d.clone());
        }
    }
    out
}

/// Intersect two directory lists, preserving `a`'s order.
fn intersect_dirs(a: &[String], b: &[String]) -> Vec<String> {
    a.iter().filter(|d| b.contains(d)).cloned().collect()
}

/// Why [`derive_successor_task`] refused to produce a child task outright.
///
/// Only the depth cap (gate 1) and an invalid proposed goal can actually
/// *reject* a derivation. The other three containment gates — autonomy
/// ceiling, directory inheritance, and the untrusted-source lockdown — are
/// infallible deterministic transforms (narrowing a rank, unioning/
/// intersecting string lists, forcing two booleans): there is no input for
/// which they cannot produce a value, so modeling them as fallible would add
/// an error variant no caller could ever construct — dead code the
/// quality gate would (rightly) flag. Each still has its own dedicated test,
/// both as a pure helper above and as end-to-end coverage on
/// `derive_successor_task` below.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SuccessorRejection {
    /// Gate 1 — depth cap: `parent.chain_depth + 1` would exceed `max_depth`.
    #[error(
        "successor would extend the chain to depth {next_depth}, exceeding the max depth of {max_depth}"
    )]
    DepthExceeded {
        /// The depth the successor would have been created at.
        next_depth: u8,
        /// The configured ceiling that would have been exceeded.
        max_depth: u8,
    },
    /// The proposed successor's own goal failed [`Successor::validate`].
    #[error("successor rejected: {0}")]
    InvalidGoal(#[from] SuccessorError),
}

/// Derive a successor [`Task`] from `parent`, enforcing every containment
/// gate load-bearing enough to matter once tasks can spawn tasks:
///
/// 1. **Depth cap** — refuses with [`SuccessorRejection::DepthExceeded`]
///    once `parent.chain_depth + 1 > max_depth`.
/// 2. **Autonomy ceiling** — [`clamp_autonomy_to_parent`]: the child's
///    `autonomy_level` is never higher-ranked than the parent's.
/// 3. **Directory inheritance** — `forbidden_dirs` is the union of the
///    parent's and a fresh task's own defaults (never fewer restrictions
///    than either); `allowed_dirs` is the intersection of the parent's
///    `allowed_dirs` and `s.allowed_dirs` when the parent's is non-empty (an
///    empty parent `allowed_dirs` means "no restriction stated," so the
///    successor's own proposal stands unmodified — matching how
///    `allowed_dirs`/`forbidden_dirs` are already treated everywhere else:
///    empty is "unset," not "empty allowlist").
/// 4. **Untrusted-source gate** — when `parent.source` is untrusted
///    ([`is_untrusted_source`]), the child gets `require_plan_approval =
///    true` unconditionally and `successor_enabled` forced to `false`, so a
///    chain seeded by unsupervised input can extend at most one more hop.
///
/// The child always carries `parent_task = Some(parent.id)`, `chain_depth =
/// parent.chain_depth + 1`, and `source =
/// TaskSource::SelfAuthored { parent: parent.id }`.
///
/// # Errors
/// See [`SuccessorRejection`].
pub fn derive_successor_task(
    parent: &Task,
    s: &Successor,
    max_depth: u8,
) -> Result<Task, SuccessorRejection> {
    s.validate()?;

    let next_depth = parent.chain_depth.saturating_add(1);
    if next_depth > max_depth {
        tracing::warn!(
            parent_task = %parent.id,
            next_depth,
            max_depth,
            "successor rejected: would exceed the configured max chain depth"
        );
        return Err(SuccessorRejection::DepthExceeded {
            next_depth,
            max_depth,
        });
    }

    let mut child = Task::new(s.goal.clone());

    // Gate 2 — autonomy ceiling.
    child.autonomy_level = clamp_autonomy_to_parent(parent.autonomy_level, child.autonomy_level);

    // Gate 3 — directory inheritance.
    child.forbidden_dirs = union_dirs(&parent.forbidden_dirs, &child.forbidden_dirs);
    child.allowed_dirs = if parent.allowed_dirs.is_empty() {
        s.allowed_dirs.clone()
    } else {
        intersect_dirs(&parent.allowed_dirs, &s.allowed_dirs)
    };

    // Gate 4 — untrusted-source lockdown.
    if is_untrusted_source(&parent.source) {
        child.require_plan_approval = true;
        child.successor_enabled = false;
    }

    child.parent_task = Some(parent.id);
    child.chain_depth = next_depth;
    child.source = TaskSource::SelfAuthored { parent: parent.id };

    Ok(child)
}

#[cfg(test)]
#[path = "successor_tests.rs"]
mod tests;
