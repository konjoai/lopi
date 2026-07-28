//! Sprint E, Part 2 — budget-aware admission, opt-in via
//! [`AgentPool::submit_economically`]. Additive: every existing caller of
//! `submit()` is untouched, so this cannot regress a caller that hasn't
//! opted into the economics layer.

use super::AgentPool;
use crate::budget::AdmissionDecline;
use lopi_core::{AgentEvent, Money, Task, TaskId};
use tracing::warn;

/// Outcome of a budget-aware admission attempt.
#[derive(Debug, Clone)]
pub enum AdmissionOutcome {
    /// Task was queued.
    Admitted(TaskId),
    /// An identical goal was already queued.
    Duplicate(TaskId),
    /// Refused — did not fit the active pool's current headroom.
    Declined(AdmissionDecline),
}

impl AgentPool {
    /// Budget-aware task submission. Falls back to plain [`Self::submit`]
    /// when no `[economics]` pool is configured. On success, holds a
    /// reservation against the pool until the task's terminal choke point
    /// in `run_one` reconciles or releases it — see
    /// [`Self::finish_economics_reservation`].
    pub async fn submit_economically(&self, task: Task) -> AdmissionOutcome {
        let Some(econ) = self.economics.clone() else {
            let id = task.id;
            return match self.submit(task).await {
                Some(existing) => AdmissionOutcome::Duplicate(existing),
                None => AdmissionOutcome::Admitted(id),
            };
        };

        if let Some((from, to)) = econ.recheck_ladder().await {
            self.bus.send(AgentEvent::BudgetTier {
                from,
                to,
                remaining_micros: econ.pool.headroom().await.micros(),
                reason: format!(
                    "pool headroom moved past a threshold: {} -> {}",
                    from.as_str(),
                    to.as_str()
                ),
            });
        }

        if !econ.ladder.current().admits_new_tasks() {
            let headroom = econ.pool.headroom().await;
            self.bus.send(AgentEvent::AdmissionDeclined {
                task_id: task.id,
                goal: task.goal.clone(),
                p90_micros: 0,
                headroom_micros: headroom.micros(),
                alternative: None,
            });
            return AdmissionOutcome::Declined(AdmissionDecline {
                p90: Money::ZERO,
                headroom,
                alternative: Some(format!(
                    "budget tier is {} — no new admissions until it recovers",
                    econ.ladder.current().as_str()
                )),
            });
        }

        let repo = task
            .repo_path
            .as_ref()
            .and_then(|p| p.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| self.repo_path().display().to_string());
        let model = task.model.clone().unwrap_or_else(|| "claude-sonnet-5".into());
        let effort = task.effort.clone();

        match econ.try_admit(Some(&repo), &model, effort.as_deref()).await {
            Ok((reservation, amount)) => {
                let task_id = task.id;
                self.economics_reservations.insert(task_id, reservation);
                let id = match self.submit(task).await {
                    Some(existing) => {
                        // Deduped against an already-queued identical goal —
                        // this reservation was never going to be used, release it.
                        econ.release(reservation).await;
                        self.economics_reservations.remove(&task_id);
                        return AdmissionOutcome::Duplicate(existing);
                    }
                    None => task_id,
                };
                let _ = amount;
                AdmissionOutcome::Admitted(id)
            }
            Err(decline) => {
                self.bus.send(AgentEvent::AdmissionDeclined {
                    task_id: task.id,
                    goal: task.goal.clone(),
                    p90_micros: decline.p90.micros(),
                    headroom_micros: decline.headroom.micros(),
                    alternative: decline.alternative.clone(),
                });
                AdmissionOutcome::Declined(decline)
            }
        }
    }

    /// Close out a task's economics reservation, if one is open — the
    /// single terminal choke point every `submit_economically`-admitted
    /// task passes through exactly once, called from `run_one`. `actual`
    /// is `Some(cost)` on a normal completion (reconciled against real
    /// spend) or `None` on an error/cancellation before any cost was
    /// recorded (released with no spend attributed). A task with no open
    /// reservation (never went through `submit_economically`, or the
    /// economics layer is inactive) is a no-op — never an error.
    pub(super) async fn finish_economics_reservation(&self, task_id: TaskId, actual: Option<Money>) {
        let Some((_, reservation)) = self.economics_reservations.remove(&task_id) else {
            return;
        };
        let Some(econ) = &self.economics else {
            warn!(task_id = %task_id, "reservation existed with no economics layer attached — releasing");
            return;
        };
        match actual {
            Some(cost) => econ.reconcile(reservation, cost).await,
            None => econ.release(reservation).await,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::budget::Economics;
    use crate::queue::TaskQueue;
    use chrono::NaiveDate;
    use lopi_core::{EconomicsConfig, EventBus, Pool};
    use lopi_memory::MemoryStore;
    use std::path::PathBuf;

    fn cfg(usd: f64) -> EconomicsConfig {
        EconomicsConfig {
            pool: Some(Pool::AgentSdkCredits {
                monthly_allotment: Money::from_usd(usd),
                resets_on: NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date"),
            }),
            ..EconomicsConfig::default()
        }
    }

    async fn pool_with_economics(usd: f64) -> AgentPool {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let econ = Economics::new(&cfg(usd), store.clone()).expect("pool configured");
        AgentPool::new(4, PathBuf::from("."), TaskQueue::new(), EventBus::new(16))
            .with_economics(econ)
            .with_store(store)
    }

    #[tokio::test]
    async fn submit_economically_admits_when_it_fits() {
        let pool = pool_with_economics(100.0).await;
        let task = Task::new("do a thing");
        let outcome = pool.submit_economically(task).await;
        assert!(matches!(outcome, AdmissionOutcome::Admitted(_)));
    }

    #[tokio::test]
    async fn submit_economically_declines_when_pool_is_too_thin() {
        let pool = pool_with_economics(0.5).await;
        let task = Task::new("do an expensive thing");
        let outcome = pool.submit_economically(task).await;
        assert!(matches!(outcome, AdmissionOutcome::Declined(_)));
    }

    #[tokio::test]
    async fn submit_economically_without_economics_configured_falls_back_to_plain_submit() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let pool = AgentPool::new(4, PathBuf::from("."), TaskQueue::new(), EventBus::new(16))
            .with_store(store);
        let task = Task::new("do a thing without a budget");
        let outcome = pool.submit_economically(task).await;
        assert!(matches!(outcome, AdmissionOutcome::Admitted(_)));
    }

    #[tokio::test]
    async fn finish_reservation_reconciles_actual_cost() {
        let pool = pool_with_economics(100.0).await;
        let task = Task::new("reconcile me");
        let task_id = task.id;
        let outcome = pool.submit_economically(task).await;
        assert!(matches!(outcome, AdmissionOutcome::Admitted(_)));
        let econ = pool.economics.clone().unwrap();
        let headroom_after_reserve = econ.pool.headroom().await;

        pool.finish_economics_reservation(task_id, Some(Money::from_usd(0.5)))
            .await;

        // Reservation released, actual (lower) cost committed instead —
        // headroom should recover the difference between the p90 hold and
        // the real cost.
        let headroom_after_reconcile = econ.pool.headroom().await;
        assert!(headroom_after_reconcile > headroom_after_reserve);
        assert!(pool.economics_reservations.get(&task_id).is_none());
    }

    #[tokio::test]
    async fn finish_reservation_on_unknown_task_is_a_silent_no_op() {
        let pool = pool_with_economics(100.0).await;
        pool.finish_economics_reservation(TaskId::new(), Some(Money::from_usd(1.0)))
            .await;
    }
}
