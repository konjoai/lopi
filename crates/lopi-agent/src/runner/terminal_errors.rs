//! Non-retryable error classification shared by the plan and implement error
//! paths in `run_loop.rs`. Split out purely to keep that module under the
//! 500-line CI file-size gate — no behavioral difference from being inline.

use crate::claude::{ERR_BUDGET_HARD_STOP, ERR_CREDIT_EXHAUSTED};

/// If `err_chain` (an error formatted with `{:#}`, carrying its full cause
/// chain) matches a known non-retryable failure, return the
/// `TaskStatus::Failed` reason prefix to use. `None` means the caller should
/// fall back to its normal retry path — the failure might not recur.
///
/// Both matched cases will fail identically on every future attempt (an
/// exhausted-credit account or a session lopi itself killed for crossing its
/// resolved USD cap don't become retryable by trying again), so routing them
/// here instead of the ordinary retry path stops the loop from burning a
/// full plan+implement cycle that can't succeed.
pub(super) fn terminal_failure_reason(err_chain: &str) -> Option<&'static str> {
    if err_chain.contains(ERR_CREDIT_EXHAUSTED) {
        Some("CreditExhausted")
    } else if err_chain.contains(ERR_BUDGET_HARD_STOP) {
        Some("BudgetExceeded")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credit_exhausted_is_terminal() {
        assert_eq!(
            terminal_failure_reason("anthropic credits exhausted: 402"),
            Some("CreditExhausted")
        );
    }

    #[test]
    fn budget_hard_stop_is_terminal() {
        assert_eq!(
            terminal_failure_reason("lopi budget hard-stop"),
            Some("BudgetExceeded")
        );
    }

    #[test]
    fn an_ordinary_error_is_not_terminal() {
        assert_eq!(terminal_failure_reason("connection reset by peer"), None);
    }
}
