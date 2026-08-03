//! Sprint P0 (review-pipeline plan, section 4) — the cost circuit breaker's pure decision
//! logic. Per-task and per-day token ceilings, checked *before* the next call is made, not
//! reactively after the fact.
//!
//! This module is deliberately I/O-free: it takes token counts as plain arguments and
//! returns a decision, so it is unit-testable with a stubbed counter (no `MemoryStore`,
//! no clock, no async) per the plan's explicit verify requirement ("structural proof only
//! — show the enforcement point and a unit test with a stubbed counter"). Wiring live
//! counters into `lopi-agent`'s actual call sites (`claude_spawn.rs`, `api_client.rs`) is
//! tracked as a work order — see `LEDGER.md`'s Cost-Circuit-Breaker-1 entry — not shipped
//! in this sprint, because both call sites currently hold no config/DB handle at all
//! (`ClaudeCode` in `claude.rs` is a pure CLI-argument builder), so wiring a live daily
//! counter is a cross-cutting change to that builder's construction chain, not a
//! same-file patch.
//!
//! Explicitly not this module's job, per the plan ("must not degrade silently, retry, or
//! fall back to a cheaper model"): on [`CeilingExceeded`], the caller must hard-stop the
//! task and surface it to a human. No retry, no silent truncation, no model downgrade.

use std::fmt;

/// Which ceiling was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CeilingScope {
    /// Total tokens for one task, summed across every retry attempt.
    PerTask,
    /// Total tokens across all tasks in one UTC day.
    PerDay,
}

impl fmt::Display for CeilingScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CeilingScope::PerTask => write!(f, "per-task"),
            CeilingScope::PerDay => write!(f, "per-day"),
        }
    }
}

/// A ceiling was exceeded. The caller must hard-stop, not retry or degrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CeilingExceeded {
    pub scope: CeilingScope,
    pub limit_tokens: u64,
    pub spent_tokens: u64,
}

impl fmt::Display for CeilingExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} token ceiling exceeded: {} spent, limit {}",
            self.scope, self.spent_tokens, self.limit_tokens
        )
    }
}

impl std::error::Error for CeilingExceeded {}

/// Stateless ceiling check. Constructed from [`crate::economics_config::EconomicsConfig`]'s
/// `per_task_token_ceiling`/`daily_token_ceiling` (both `None` = that ceiling is unset,
/// matching every other opt-in field in that config).
#[derive(Debug, Clone, Copy, Default)]
pub struct CostCircuitBreaker {
    per_task_ceiling: Option<u64>,
    daily_ceiling: Option<u64>,
}

impl CostCircuitBreaker {
    #[must_use]
    pub fn new(per_task_ceiling: Option<u64>, daily_ceiling: Option<u64>) -> Self {
        Self {
            per_task_ceiling,
            daily_ceiling,
        }
    }

    /// Check both ceilings against the running totals *before* the next call is made.
    /// `task_tokens_so_far`/`day_tokens_so_far` are the caller's own running counters
    /// (live wiring: `UsageAccrual`'s per-session total for the task scope, and
    /// `MemoryStore::daily_token_totals` for the day scope) — this function does not
    /// fetch them itself. Per-day is checked first: a task that would also blow the
    /// per-task ceiling is still reported as the day ceiling if the day is already over,
    /// since that is the more urgent stop condition for the human reading the error.
    ///
    /// # Errors
    /// Returns [`CeilingExceeded`] naming whichever ceiling is exceeded. Never retries,
    /// never truncates, never substitutes a cheaper model — the caller must hard-stop.
    pub fn check(
        &self,
        task_tokens_so_far: u64,
        day_tokens_so_far: u64,
    ) -> Result<(), CeilingExceeded> {
        if let Some(limit) = self.daily_ceiling {
            if day_tokens_so_far >= limit {
                return Err(CeilingExceeded {
                    scope: CeilingScope::PerDay,
                    limit_tokens: limit,
                    spent_tokens: day_tokens_so_far,
                });
            }
        }
        if let Some(limit) = self.per_task_ceiling {
            if task_tokens_so_far >= limit {
                return Err(CeilingExceeded {
                    scope: CeilingScope::PerTask,
                    limit_tokens: limit,
                    spent_tokens: task_tokens_so_far,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn both_ceilings_unset_never_trips() {
        let b = CostCircuitBreaker::new(None, None);
        assert!(b.check(u64::MAX, u64::MAX).is_ok());
    }

    #[test]
    fn per_task_ceiling_trips_at_the_limit() {
        let b = CostCircuitBreaker::new(Some(1000), None);
        assert!(b.check(999, 0).is_ok());
        let err = b.check(1000, 0).unwrap_err();
        assert_eq!(err.scope, CeilingScope::PerTask);
        assert_eq!(err.limit_tokens, 1000);
        assert_eq!(err.spent_tokens, 1000);
    }

    #[test]
    fn daily_ceiling_trips_at_the_limit() {
        let b = CostCircuitBreaker::new(None, Some(50_000));
        assert!(b.check(0, 49_999).is_ok());
        let err = b.check(0, 50_000).unwrap_err();
        assert_eq!(err.scope, CeilingScope::PerDay);
    }

    #[test]
    fn daily_ceiling_takes_priority_when_both_exceeded() {
        // A stubbed counter standing in for a live UsageAccrual/daily_token_totals read:
        // both the task and the day are already over their ceilings.
        let b = CostCircuitBreaker::new(Some(1000), Some(50_000));
        let err = b.check(5000, 60_000).unwrap_err();
        assert_eq!(
            err.scope,
            CeilingScope::PerDay,
            "day ceiling reported first, per doc comment"
        );
    }

    #[test]
    fn per_task_ceiling_alone_trips_when_day_is_fine() {
        let b = CostCircuitBreaker::new(Some(1000), Some(50_000));
        let err = b.check(1000, 10).unwrap_err();
        assert_eq!(err.scope, CeilingScope::PerTask);
    }

    #[test]
    fn ceiling_exceeded_never_suggests_a_fallback() {
        // Regression guard for the plan's explicit constraint: the error type carries no
        // "retry" or "downgrade" variant to reach for. This test exists so a future
        // change that adds one is a visible, reviewed diff to this match arm, not a
        // silent capability creep.
        let err = CeilingExceeded {
            scope: CeilingScope::PerTask,
            limit_tokens: 1,
            spent_tokens: 1,
        };
        match err.scope {
            CeilingScope::PerTask | CeilingScope::PerDay => {}
        }
    }
}
