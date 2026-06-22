//! Phase 11 — plan-approval gate. Surfaces a proposed plan and blocks until a
//! human decision arrives (or the wait times out).

use super::plan_steps::parse_plan_steps;
use super::AgentRunner;
use lopi_core::{AgentEvent, PlanDecision, TaskStatus};
use lopi_git::GitManager;

/// Whether the plan-approval gate applies to this attempt: only the first
/// attempt (zero-based `attempt == 0`) of a task that opts into approval.
/// Retries refine an already-approved plan, so they are never re-gated.
pub(super) fn plan_gate_applies(attempt: u8, require_plan_approval: bool) -> bool {
    attempt == 0 && require_plan_approval
}

/// Outcome of the Phase 11 plan approval gate.
pub(super) enum PlanGate {
    /// The operator approved the plan — proceed to implementation.
    Approved,
    /// The operator rejected the plan — abandon the task.
    Rejected,
    /// The run was cancelled while awaiting a decision.
    Cancelled,
}

impl AgentRunner {
    /// Phase 11 — apply the plan-approval gate. Only the first attempt of a
    /// gated task is gated; retries refine an already-approved plan. `attempt`
    /// is the zero-based attempt index.
    ///
    /// Returns `Some(terminal_status)` when the operator rejected or cancelled
    /// (the branch is rolled back first) — the caller returns it. `None` means
    /// "approved or not gated; proceed to implementation".
    pub(super) async fn gate_plan(
        &mut self,
        plan: &str,
        attempt: u8,
        git: &GitManager,
    ) -> Option<TaskStatus> {
        if !plan_gate_applies(attempt, self.task.require_plan_approval) {
            return None;
        }
        match self.await_plan_approval(plan, attempt + 1).await {
            PlanGate::Approved => None,
            PlanGate::Rejected => {
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
                let status = TaskStatus::Failed {
                    reason: "Plan rejected by operator".into(),
                };
                self.status(status.clone(), attempt + 1);
                Some(status)
            }
            PlanGate::Cancelled => {
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
                Some(TaskStatus::Failed {
                    reason: "Cancelled".into(),
                })
            }
        }
    }

    /// Emit the proposed plan, mark the task awaiting approval, and block until
    /// a decision arrives (or the wait times out). Ungated runs with no
    /// decision channel auto-approve so a CLI run never stalls.
    pub(super) async fn await_plan_approval(&mut self, plan: &str, attempt: u8) -> PlanGate {
        self.bus.send(AgentEvent::PlanProposed {
            task_id: self.id(),
            attempt,
            steps: parse_plan_steps(plan),
            plan: plan.to_string(),
        });
        self.status(TaskStatus::AwaitingPlanApproval { attempt }, attempt);
        self.log("⏸ awaiting plan approval…");

        let Some(rx) = self.plan_decision_rx.take() else {
            self.log("no approval channel — auto-approving plan");
            return PlanGate::Approved;
        };

        // Cap the wait so a forgotten approval cannot pin an agent slot forever.
        let decided = async {
            tokio::select! {
                d = rx => match d {
                    Ok(PlanDecision::Approve) => PlanGate::Approved,
                    Ok(PlanDecision::Reject) => PlanGate::Rejected,
                    Err(_) => PlanGate::Approved, // sender dropped — fail open
                },
                () = self.cancel_token.cancelled() => PlanGate::Cancelled,
            }
        };
        match tokio::time::timeout(std::time::Duration::from_secs(3600), decided).await {
            Ok(gate) => {
                match gate {
                    PlanGate::Approved => self.log("✅ plan approved — implementing"),
                    PlanGate::Rejected => self.log("❌ plan rejected"),
                    PlanGate::Cancelled => self.log("⛔ cancelled while awaiting approval"),
                }
                gate
            }
            Err(_) => {
                self.warn("plan approval timed out (1h) — rejecting");
                PlanGate::Rejected
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::plan_gate_applies;

    #[test]
    fn gates_only_first_attempt_of_an_opted_in_task() {
        assert!(plan_gate_applies(0, true));
        // Retries are never re-gated.
        assert!(!plan_gate_applies(1, true));
        assert!(!plan_gate_applies(2, true));
        // Tasks that did not opt in are never gated.
        assert!(!plan_gate_applies(0, false));
        assert!(!plan_gate_applies(1, false));
    }
}
