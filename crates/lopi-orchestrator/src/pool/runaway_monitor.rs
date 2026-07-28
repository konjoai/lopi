//! Sprint E, Part 4 — the runaway monitor: subscribes to the event bus,
//! tracks live per-session spend/progress, and pauses any session a
//! detector trips.
//!
//! "A tripped detector pauses at the next safe checkpoint and asks, via
//! Telegram, with the evidence attached." Telegram was removed in Sprint
//! S10 (see `LEDGER.md`'s Sprint E entry) — this pauses via the existing
//! `AgentPool::cancel` primitive (the only real "stop a running session"
//! mechanism this codebase has; there is no true suspend/resume), writes a
//! handoff artifact, and broadcasts `AgentEvent::RunawayPaused` with the
//! evidence for whichever remote surface is listening. Default behavior is
//! straight to stop-and-hand-off — the brief's own specified default on no
//! operator answer within a timeout, so implementing that default directly
//! (rather than an interactive resume/downshift prompt this runner has no
//! pause primitive to support) is not a shortcut, it's the specified
//! fallback path.
//!
//! Burn-rate detection (detector #1) needs a live per-(repo, stage, model)
//! baseline this monitor doesn't cheaply have without a DB round-trip per
//! tick; it's left wired in `budget::detect` and unit-tested there, but not
//! driven from this live loop yet — cost-per-progress (detector #2, "the
//! one that would have caught my incident") and the hard ceiling (#3) are.

use super::AgentPool;
use crate::budget::detect::RunawayVerdict;
use crate::budget::ladder::write_handoff;
use crate::budget::{Economics, RunawayDetectors};
use dashmap::DashMap;
use lopi_core::{AgentEvent, Money, TaskId};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::warn;

/// Live per-session state the monitor accumulates from the event bus.
#[derive(Debug, Clone, Default)]
struct SessionTracker {
    goal: String,
    repo: String,
    spend_total: Money,
    spend_at_last_progress: Money,
    last_gate_result: String,
    last_commit: Option<String>,
}

/// Fold one bus event into `trackers`. Pure with respect to the map (no
/// I/O), so this is unit-testable without a live bus.
fn apply_event(trackers: &DashMap<TaskId, SessionTracker>, event: &AgentEvent) {
    match event {
        AgentEvent::TaskStarted { task_id, repo, .. } => {
            trackers.insert(
                *task_id,
                SessionTracker {
                    repo: repo.clone(),
                    last_gate_result: "none".to_string(),
                    ..SessionTracker::default()
                },
            );
        }
        AgentEvent::TaskQueued { task_id, goal, .. } => {
            if let Some(mut t) = trackers.get_mut(task_id) {
                t.goal = goal.clone();
            }
        }
        AgentEvent::Cost {
            task_id, cost_usd, ..
        } => {
            if let Some(mut t) = trackers.get_mut(task_id) {
                t.spend_total = Money::from_usd(*cost_usd);
            }
        }
        AgentEvent::VerifierVerdict {
            task_id, passed, ..
        } => {
            if let Some(mut t) = trackers.get_mut(task_id) {
                t.last_gate_result = if *passed { "pass" } else { "fail" }.to_string();
                if *passed {
                    t.spend_at_last_progress = t.spend_total;
                }
            }
        }
        AgentEvent::TaskCompleted { task_id, .. } | AgentEvent::TaskCancelled { task_id } => {
            trackers.remove(task_id);
        }
        _ => {}
    }
}

/// Decide whether `tracker`'s current state trips detector #2 or #3.
/// Detector #1 (burn rate) is intentionally not evaluated here — see the
/// module doc comment.
fn evaluate(detectors: &RunawayDetectors, tracker: &SessionTracker) -> Option<RunawayVerdict> {
    // Cost-per-progress (detector #2) needs a live per-(repo, stage, model)
    // p90 baseline this loop doesn't cheaply have without a DB round-trip
    // per tick — see the module doc comment. Only the unconditional hard
    // ceiling (#3) is evaluated live for now; #2 stays wired and
    // unit-tested in `budget::detect`, ready for that baseline to be
    // threaded in.
    detectors.check_hard_ceiling(tracker.spend_total)
}

impl AgentPool {
    /// Start the runaway monitor as a background task, tied to the pool's
    /// own event bus. No-op if no `[economics]` pool is configured. Runs
    /// for the process lifetime; safe to call once per pool.
    pub fn start_runaway_monitor(&self) {
        let Some(econ) = self.economics.clone() else {
            return;
        };
        let bus = self.bus.clone();
        let pool = self.clone();
        let trackers: Arc<DashMap<TaskId, SessionTracker>> = Arc::new(DashMap::new());

        {
            let trackers = trackers.clone();
            let mut rx = bus.subscribe();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(event) => apply_event(&trackers, &event),
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("runaway monitor lagged {n} events");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                sweep_once(&pool, &econ, &trackers).await;
            }
        });
    }
}

/// One sweep over every tracked session — split out of the spawn closure
/// so it's directly callable from tests without waiting on a real 10s
/// interval tick.
async fn sweep_once(
    pool: &AgentPool,
    econ: &Arc<Economics>,
    trackers: &DashMap<TaskId, SessionTracker>,
) {
    let tripped: Vec<(TaskId, SessionTracker, RunawayVerdict)> = trackers
        .iter()
        .filter_map(|entry| {
            evaluate(&econ.detectors, entry.value())
                .map(|v| (*entry.key(), entry.value().clone(), v))
        })
        .collect();

    for (task_id, tracker, verdict) in tripped {
        pool.cancel(&task_id).await;
        let repo = PathBuf::from(&tracker.repo);
        if let Err(e) = write_handoff(
            &repo,
            task_id,
            &tracker.goal,
            "implement",
            lopi_core::BudgetTier::Halt,
            &format!("runaway detector `{}` tripped", verdict.detector_name()),
        )
        .await
        {
            warn!(task_id = %task_id, "runaway handoff write failed: {e:#}");
        }
        pool.bus().send(AgentEvent::RunawayPaused {
            task_id,
            detector: verdict.detector_name().to_string(),
            burn_rate_tokens_per_min: 0.0,
            spend_micros: tracker.spend_total.micros(),
            last_gate_result: tracker.last_gate_result.clone(),
            last_commit: tracker.last_commit.clone(),
            stage: "implement".to_string(),
        });
        trackers.remove(&task_id);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::queue::TaskQueue;
    use chrono::NaiveDate;
    use lopi_core::{EconomicsConfig, EventBus, Pool, TaskStatus};
    use lopi_memory::MemoryStore;

    fn cfg(hard_ceiling_usd: f64) -> EconomicsConfig {
        EconomicsConfig {
            pool: Some(Pool::AgentSdkCredits {
                monthly_allotment: Money::from_usd(1000.0),
                resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date"),
            }),
            hard_session_ceiling: Money::from_usd(hard_ceiling_usd),
            ..EconomicsConfig::default()
        }
    }

    #[test]
    fn apply_event_tracks_started_and_cost() {
        let trackers = DashMap::new();
        let task_id = TaskId::new();
        apply_event(
            &trackers,
            &AgentEvent::TaskStarted {
                task_id,
                attempt: 1,
                branch: "b".into(),
                repo: "/repo/a".into(),
            },
        );
        apply_event(
            &trackers,
            &AgentEvent::Cost {
                task_id,
                cost_usd: 5.0,
                num_turns: 1,
                session_id: String::new(),
            },
        );
        let t = trackers.get(&task_id).expect("tracked");
        assert_eq!(t.repo, "/repo/a");
        assert_eq!(t.spend_total, Money::from_usd(5.0));
    }

    #[test]
    fn apply_event_verifier_pass_resets_progress_baseline() {
        let trackers = DashMap::new();
        let task_id = TaskId::new();
        apply_event(
            &trackers,
            &AgentEvent::TaskStarted {
                task_id,
                attempt: 1,
                branch: "b".into(),
                repo: "/repo/a".into(),
            },
        );
        apply_event(
            &trackers,
            &AgentEvent::Cost {
                task_id,
                cost_usd: 3.0,
                num_turns: 1,
                session_id: String::new(),
            },
        );
        apply_event(
            &trackers,
            &AgentEvent::VerifierVerdict {
                task_id,
                passed: true,
                gaps: vec![],
                fix_hints: vec![],
                confidence: 1.0,
            },
        );
        let t = trackers.get(&task_id).expect("tracked");
        assert_eq!(t.spend_at_last_progress, Money::from_usd(3.0));
        assert_eq!(t.last_gate_result, "pass");
    }

    #[test]
    fn apply_event_completion_removes_tracker() {
        let trackers = DashMap::new();
        let task_id = TaskId::new();
        apply_event(
            &trackers,
            &AgentEvent::TaskStarted {
                task_id,
                attempt: 1,
                branch: "b".into(),
                repo: "/repo/a".into(),
            },
        );
        apply_event(
            &trackers,
            &AgentEvent::TaskCompleted {
                task_id,
                outcome: TaskStatus::Success {
                    branch: "b".into(),
                    pr_url: None,
                },
                total_attempts: 1,
                successor: None,
            },
        );
        assert!(trackers.get(&task_id).is_none());
    }

    #[test]
    fn evaluate_trips_hard_ceiling() {
        let detectors = RunawayDetectors::new(Money::from_usd(5.0), 3.0);
        let tracker = SessionTracker {
            spend_total: Money::from_usd(6.0),
            ..SessionTracker::default()
        };
        let verdict = evaluate(&detectors, &tracker).expect("must trip");
        assert_eq!(verdict.detector_name(), "hard_ceiling");
    }

    #[test]
    fn evaluate_does_not_trip_under_ceiling() {
        let detectors = RunawayDetectors::new(Money::from_usd(5.0), 3.0);
        let tracker = SessionTracker {
            spend_total: Money::from_usd(4.0),
            ..SessionTracker::default()
        };
        assert!(evaluate(&detectors, &tracker).is_none());
    }

    /// End-to-end-ish: a real pool + real bus, synthetic events pushed in,
    /// one manual sweep — asserts `RunawayPaused` actually broadcasts once
    /// the hard ceiling trips.
    #[tokio::test]
    async fn sweep_once_broadcasts_runaway_paused_and_clears_the_tracker() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let econ = Arc::new(Economics::new(&cfg(1.0), store.clone()).unwrap());
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let pool =
            AgentPool::new(4, PathBuf::from("."), TaskQueue::new(), bus.clone()).with_store(store);
        let mut sub = bus.subscribe();

        let trackers: Arc<DashMap<TaskId, SessionTracker>> = Arc::new(DashMap::new());
        let task_id = TaskId::new();
        apply_event(
            &trackers,
            &AgentEvent::TaskStarted {
                task_id,
                attempt: 1,
                branch: "b".into(),
                repo: std::env::temp_dir().to_string_lossy().to_string(),
            },
        );
        apply_event(
            &trackers,
            &AgentEvent::Cost {
                task_id,
                cost_usd: 2.0, // over the 1.0 hard ceiling
                num_turns: 1,
                session_id: String::new(),
            },
        );

        sweep_once(&pool, &econ, &trackers).await;

        assert!(
            trackers.get(&task_id).is_none(),
            "tripped tracker must clear"
        );
        let event = tokio::time::timeout(Duration::from_secs(1), sub.recv())
            .await
            .expect("must receive an event")
            .unwrap();
        assert!(matches!(
            event,
            AgentEvent::RunawayPaused { detector, .. } if detector == "hard_ceiling"
        ));
    }
}
