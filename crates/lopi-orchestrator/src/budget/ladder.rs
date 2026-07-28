//! Sprint E, Part 3 — the degradation ladder + handoff writer.
//!
//! The invariant that matters more than any rung: **no agent is ever
//! killed mid-stage.** Every stop path (`Essential` onward) goes through
//! [`write_handoff`] before a task is denied its next stage — a truncated
//! session leaves a half-implemented feature and an undocumented repo, and
//! the next session pays to rediscover the mess, so a "saving" that
//! truncates work costs more than it saves.

use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::{BudgetTier, LadderThresholds, TaskId};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

/// Tracks the pool's current [`BudgetTier`] and reports genuine
/// transitions (never a same-tier no-op re-check) so callers only emit
/// `AgentEvent::BudgetTier` — and only pay the handoff-writer cost — when
/// the rung actually changed.
pub struct Ladder {
    current: AtomicU8,
}

impl Ladder {
    /// A fresh ladder always starts at `Full` — matches a pool with no
    /// spend yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: AtomicU8::new(tier_to_u8(BudgetTier::Full)),
        }
    }

    /// The tier this ladder is currently on.
    #[must_use]
    pub fn current(&self) -> BudgetTier {
        u8_to_tier(self.current.load(Ordering::SeqCst))
    }

    /// Recompute the tier for `headroom_ratio` against `thresholds` and
    /// atomically swap it in. Returns `Some((from, to))` only on a genuine
    /// change — a re-check that lands on the same tier is a no-op, not a
    /// transition.
    pub fn recheck(
        &self,
        headroom_ratio: f64,
        thresholds: &LadderThresholds,
    ) -> Option<(BudgetTier, BudgetTier)> {
        let new_tier = thresholds.tier_for_ratio(headroom_ratio);
        let old = u8_to_tier(self.current.swap(tier_to_u8(new_tier), Ordering::SeqCst));
        if old == new_tier {
            None
        } else {
            Some((old, new_tier))
        }
    }
}

impl Default for Ladder {
    fn default() -> Self {
        Self::new()
    }
}

const fn tier_to_u8(t: BudgetTier) -> u8 {
    match t {
        BudgetTier::Full => 0,
        BudgetTier::Conserve => 1,
        BudgetTier::Essential => 2,
        BudgetTier::Drain => 3,
        BudgetTier::Halt => 4,
    }
}

const fn u8_to_tier(v: u8) -> BudgetTier {
    match v {
        0 => BudgetTier::Full,
        1 => BudgetTier::Conserve,
        2 => BudgetTier::Essential,
        3 => BudgetTier::Drain,
        _ => BudgetTier::Halt,
    }
}

/// The effort ladder, low to high — mirrors `Task::effort`'s accepted
/// values (`claude_spawn`'s `--effort` validation).
const EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Resolve the effort this stage should actually run at, given the current
/// tier. Only `Conserve` degrades anything, and only on `implement`/
/// `optimize` — "never on plan, verify, or the adversarial reviewer; those
/// are where reasoning pays for itself and cutting them costs more in
/// retries than it saves." `Essential` and worse don't admit new stage
/// transitions at all (see [`BudgetTier::requires_handoff_checkpoint`]), so
/// this function only ever needs to answer for `Full`/`Conserve`.
///
/// `requested = None` (no explicit override) is left alone — there is no
/// concrete "one level down from unset" to drop to without guessing the
/// model's own default.
#[must_use]
pub fn effective_effort(stage: &str, tier: BudgetTier, requested: Option<&str>) -> Option<String> {
    if tier != BudgetTier::Conserve || !matches!(stage, "implement" | "optimize") {
        return requested.map(str::to_string);
    }
    let level = requested?;
    let idx = EFFORT_LEVELS.iter().position(|&l| l == level)?;
    Some(EFFORT_LEVELS[idx.saturating_sub(1)].to_string())
}

/// Where handoff artifacts for `repo` are written — `<repo>/.lopi/handoffs/`.
#[must_use]
pub fn handoff_dir(repo: &Path) -> PathBuf {
    repo.join(".lopi").join("handoffs")
}

/// Write a real handoff artifact for `task_id` stopping at a budget
/// checkpoint. Every stop path from `Essential` on must call this before
/// denying a task its next stage — see the module doc comment's
/// no-agent-killed-mid-stage invariant. Returns the artifact's path.
///
/// # Errors
/// Returns `Err` if the handoff directory can't be created or the file
/// can't be written — a failed handoff write must never be silently
/// swallowed, since it's the one piece of state that lets a later session
/// pick this task back up.
pub async fn write_handoff(
    repo: &Path,
    task_id: TaskId,
    goal: &str,
    stage: &str,
    tier: BudgetTier,
    reason: &str,
) -> Result<PathBuf> {
    let dir = handoff_dir(repo);
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("creating handoff dir {}", dir.display()))?;
    let path = dir.join(format!("{task_id}.md"));
    let content = format!(
        "# Handoff — {goal}\n\n\
         - task_id: {task_id}\n\
         - stage: {stage}\n\
         - budget_tier: {}\n\
         - reason: {reason}\n\
         - written_at: {}\n\n\
         This task stopped at a budget-driven checkpoint, not mid-stage — the \
         current stage ran to completion and its work is committed. Resume by \
         re-queuing the same goal once the pool has headroom again; prior \
         progress lives on the task's git branch/worktree.\n",
        tier.as_str(),
        Utc::now().to_rfc3339(),
    );
    tokio::fs::write(&path, content)
        .await
        .with_context(|| format!("writing handoff artifact {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lopi_core::LadderThresholds;

    #[test]
    fn fresh_ladder_starts_at_full() {
        assert_eq!(Ladder::new().current(), BudgetTier::Full);
    }

    #[test]
    fn recheck_reports_none_on_same_tier() {
        let ladder = Ladder::new();
        let thresholds = LadderThresholds::default();
        assert_eq!(ladder.recheck(1.0, &thresholds), None);
        assert_eq!(ladder.current(), BudgetTier::Full);
    }

    #[test]
    fn recheck_reports_a_genuine_transition() {
        let ladder = Ladder::new();
        let thresholds = LadderThresholds::default();
        let transition = ladder.recheck(0.3, &thresholds);
        assert_eq!(transition, Some((BudgetTier::Full, BudgetTier::Conserve)));
        assert_eq!(ladder.current(), BudgetTier::Conserve);
    }

    #[test]
    fn recheck_can_recover_upward_when_headroom_returns() {
        let ladder = Ladder::new();
        let thresholds = LadderThresholds::default();
        ladder.recheck(0.05, &thresholds);
        assert_eq!(ladder.current(), BudgetTier::Drain);
        let transition = ladder.recheck(1.0, &thresholds);
        assert_eq!(transition, Some((BudgetTier::Drain, BudgetTier::Full)));
    }

    #[test]
    fn effective_effort_only_degrades_conserve_on_implement_and_optimize() {
        assert_eq!(
            effective_effort("implement", BudgetTier::Full, Some("high")),
            Some("high".to_string())
        );
        assert_eq!(
            effective_effort("implement", BudgetTier::Conserve, Some("high")),
            Some("medium".to_string())
        );
        assert_eq!(
            effective_effort("optimize", BudgetTier::Conserve, Some("medium")),
            Some("low".to_string())
        );
        assert_eq!(
            effective_effort("optimize", BudgetTier::Conserve, Some("low")),
            Some("low".to_string()),
            "must floor at low, never go out of range"
        );
    }

    #[test]
    fn effective_effort_never_touches_plan_or_verify() {
        assert_eq!(
            effective_effort("plan", BudgetTier::Conserve, Some("high")),
            Some("high".to_string()),
            "plan must never be downgraded, even in Conserve"
        );
        assert_eq!(
            effective_effort("verify", BudgetTier::Conserve, Some("high")),
            Some("high".to_string()),
            "verify must never be downgraded, even in Conserve"
        );
    }

    #[test]
    fn effective_effort_leaves_unset_effort_alone() {
        assert_eq!(
            effective_effort("implement", BudgetTier::Conserve, None),
            None
        );
    }

    #[test]
    fn effective_effort_only_conserve_degrades_never_essential_or_worse() {
        // Essential/Drain/Halt don't admit new stage transitions at all
        // (BudgetTier::requires_handoff_checkpoint), so effort resolution
        // for those tiers is moot — but must still not silently degrade if
        // ever called, to fail safe rather than fail confusing.
        assert_eq!(
            effective_effort("implement", BudgetTier::Essential, Some("high")),
            Some("high".to_string())
        );
    }

    #[tokio::test]
    async fn write_handoff_produces_a_real_artifact_with_expected_fields() {
        let dir = tempfile::tempdir().unwrap();
        let task_id = TaskId::new();
        let path = write_handoff(
            dir.path(),
            task_id,
            "fix the thing",
            "implement",
            BudgetTier::Essential,
            "pool headroom 15% <= 20% essential threshold",
        )
        .await
        .unwrap();
        assert!(path.exists(), "handoff artifact must exist on disk");
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("fix the thing"));
        assert!(content.contains(&task_id.to_string()));
        assert!(content.contains("essential"));
        assert!(content.contains("implement"));
    }

    /// The brief: "every rung needs a test that asserts a clean handoff
    /// artifact exists afterwards" — every rung from `Essential` on
    /// (`requires_handoff_checkpoint`) must produce a real file.
    #[tokio::test]
    async fn every_handoff_requiring_rung_writes_a_clean_artifact() {
        for tier in [BudgetTier::Essential, BudgetTier::Drain, BudgetTier::Halt] {
            assert!(tier.requires_handoff_checkpoint());
            let dir = tempfile::tempdir().unwrap();
            let task_id = TaskId::new();
            let path = write_handoff(dir.path(), task_id, "goal", "implement", tier, "test")
                .await
                .unwrap();
            assert!(path.exists(), "{tier:?} must produce a handoff artifact");
        }
    }
}
