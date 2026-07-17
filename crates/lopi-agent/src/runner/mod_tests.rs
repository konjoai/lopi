//! Unit tests for `mod.rs` — split out to keep that module under the
//! 500-line CI file-size gate. Included via `#[path]` from `mod.rs`, so
//! `super::*` resolves to the runner module's items.
use super::*;
use lopi_core::Task;

/// `with_cli_budget_usd` had never actually been wired from any config
/// path (`self.cli_budget_usd` was always `None`), so `--max-budget-usd`
/// was never passed to `claude -p` regardless of any budget the operator
/// configured — a research-style task that fanned out into several
/// parallel sub-agents ran fully uncapped and reached $25.79 for one
/// session. Locks in the builder itself, since a runaway session is
/// exactly what a regression here would silently reintroduce.
#[test]
fn with_cli_budget_usd_zero_disables_the_cap() {
    let (runner, _bus) = AgentRunner::standalone(Task::new("t"), std::env::temp_dir());
    let runner = runner.with_cli_budget_usd(0.0);
    assert_eq!(runner.cli_budget_usd(), None);
}

#[test]
fn with_cli_budget_usd_negative_also_disables_the_cap() {
    let (runner, _bus) = AgentRunner::standalone(Task::new("t"), std::env::temp_dir());
    let runner = runner.with_cli_budget_usd(-1.0);
    assert_eq!(runner.cli_budget_usd(), None);
}

#[test]
fn with_cli_budget_usd_positive_value_is_set() {
    let (runner, _bus) = AgentRunner::standalone(Task::new("t"), std::env::temp_dir());
    let runner = runner.with_cli_budget_usd(5.0);
    assert_eq!(runner.cli_budget_usd(), Some(5.0));
}

#[test]
fn a_fresh_runner_has_no_cli_budget_cap_until_wired() {
    let (runner, _bus) = AgentRunner::standalone(Task::new("t"), std::env::temp_dir());
    assert_eq!(
        runner.cli_budget_usd(),
        None,
        "the pool is responsible for wiring LoopConfig::max_budget_usd in; the bare runner defaults to uncapped"
    );
}

#[test]
fn a_fresh_runner_has_no_tool_permission_overrides_until_wired() {
    let (runner, _bus) = AgentRunner::standalone(Task::new("t"), std::env::temp_dir());
    assert!(runner.permission_allow.is_empty());
    assert!(runner.permission_deny.is_empty());
}

#[test]
fn with_tool_permissions_sets_both_lists() {
    let (runner, _bus) = AgentRunner::standalone(Task::new("t"), std::env::temp_dir());
    let runner =
        runner.with_tool_permissions(vec!["Bash".to_string()], vec!["Workflow".to_string()]);
    assert_eq!(runner.permission_allow, vec!["Bash".to_string()]);
    assert_eq!(runner.permission_deny, vec!["Workflow".to_string()]);
}
