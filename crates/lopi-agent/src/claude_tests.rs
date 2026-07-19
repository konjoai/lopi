#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
//! `ClaudeCode` builder/wrapper tests — split out of `claude.rs` purely to
//! keep that file under the 500-line CI file-size gate; no behavioral
//! difference from being inline. `select_model`/`compress_errors` tests
//! moved with their functions to `claude_model.rs`/`claude_support.rs`.

use super::*;
use lopi_core::Task;

/// `with_allowed_tools`/`with_disallowed_tools` back `--allowedTools`/
/// `--disallowedTools` — verified live (outside this test, since it needs
/// a real `claude -p` call) that these are genuinely enforced even
/// alongside the unconditional `--dangerously-skip-permissions`, not
/// silently bypassed by it. This locks in the builder plumbing itself.
#[test]
fn with_allowed_tools_sets_the_field() {
    let c = ClaudeCode::new(".").with_allowed_tools(vec!["Bash".to_string()]);
    assert_eq!(c.allowed_tools, vec!["Bash".to_string()]);
    assert!(c.disallowed_tools.is_empty());
}

#[test]
fn with_disallowed_tools_sets_the_field() {
    let c = ClaudeCode::new(".").with_disallowed_tools(vec!["Workflow".to_string()]);
    assert_eq!(c.disallowed_tools, vec!["Workflow".to_string()]);
    assert!(c.allowed_tools.is_empty());
}

#[test]
fn a_fresh_claude_code_has_no_tool_restrictions() {
    let c = ClaudeCode::new(".");
    assert!(c.allowed_tools.is_empty());
    assert!(c.disallowed_tools.is_empty());
}

/// Part 0 — `ClaudeCode::plan_streaming` (the wrapper `implement_speculative`
/// actually calls) must forward `self.max_budget_usd`/`disallowed_tools` to
/// the real subprocess argv, not just hold them as fields. Before this fix,
/// a `ClaudeCode` built exactly the way `run_loop.rs` builds it for a
/// speculative attempt — with `LoopConfig`'s caps already wired via
/// `with_max_budget_usd`/`with_disallowed_tools` — still spawned this one
/// path uncapped, because the free-function `claude_stream::plan_streaming`
/// it delegated to didn't accept those params at all.
#[tokio::test]
async fn plan_streaming_wrapper_forwards_budget_and_deny_to_subprocess() {
    use std::os::unix::fs::PermissionsExt;
    let script = std::env::temp_dir().join("lopi_claude_wrapper_stub.sh");
    let capture = std::env::temp_dir().join("lopi_claude_wrapper_capture.txt");
    std::fs::remove_file(&capture).ok();
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> {}; done\necho '1. step'\n",
            capture.display()
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let claude = ClaudeCode::new(".")
        .with_cli(script.to_str().unwrap())
        .with_max_budget_usd(1.0)
        .with_disallowed_tools(vec!["Workflow".to_string()]);
    let task = Task::new("wrapper forwarding test");
    let (handle, mut rx) = claude.plan_streaming(&task);
    while rx.recv().await.is_some() {}
    handle.await.unwrap().unwrap();

    let argv = std::fs::read_to_string(&capture).unwrap();
    assert!(argv.contains("--max-budget-usd\n1"), "argv={argv}");
    assert!(argv.contains("--disallowedTools\nWorkflow"), "argv={argv}");

    std::fs::remove_file(&script).ok();
    std::fs::remove_file(&capture).ok();
}

/// Permission-Modes-1 — `run` (backs `implement_step`/`fix`) must emit
/// `--permission-mode <value>` for a configured mode, not the old
/// unconditional `--dangerously-skip-permissions`.
#[tokio::test]
async fn run_forwards_configured_permission_mode_to_the_subprocess_argv() {
    use std::os::unix::fs::PermissionsExt;
    let script = std::env::temp_dir().join("lopi_claude_run_permmode_stub.sh");
    let capture = std::env::temp_dir().join("lopi_claude_run_permmode_capture.txt");
    std::fs::remove_file(&capture).ok();
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> {}; done\n",
            capture.display()
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let claude = ClaudeCode::new(".")
        .with_cli(script.to_str().unwrap())
        .with_permission_mode("acceptEdits");
    let task = Task::new("permission mode forwarding test");
    claude.implement_step(&task, "do the thing").await.unwrap();

    let argv = std::fs::read_to_string(&capture).unwrap();
    assert!(
        argv.contains("--permission-mode\nacceptEdits"),
        "argv={argv}"
    );
    assert!(
        !argv.contains("--dangerously-skip-permissions"),
        "argv={argv}"
    );

    std::fs::remove_file(&script).ok();
    std::fs::remove_file(&capture).ok();
}

/// `run` with no configured permission mode must still reproduce the old
/// unconditional behavior exactly: `--permission-mode bypassPermissions`.
#[tokio::test]
async fn run_falls_back_to_bypass_permissions_when_unset() {
    use std::os::unix::fs::PermissionsExt;
    let script = std::env::temp_dir().join("lopi_claude_run_permmode_default_stub.sh");
    let capture = std::env::temp_dir().join("lopi_claude_run_permmode_default_capture.txt");
    std::fs::remove_file(&capture).ok();
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> {}; done\n",
            capture.display()
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let claude = ClaudeCode::new(".").with_cli(script.to_str().unwrap());
    let task = Task::new("permission mode default test");
    claude.implement_step(&task, "do the thing").await.unwrap();

    let argv = std::fs::read_to_string(&capture).unwrap();
    assert!(
        argv.contains("--permission-mode\nbypassPermissions"),
        "argv={argv}"
    );

    std::fs::remove_file(&script).ok();
    std::fs::remove_file(&capture).ok();
}

/// Permission-Modes-1 — `run_streamed` (backs `plan_streamed`/
/// `implement_streamed`) must emit `--permission-mode <value>` too, not just
/// the one-shot `run` path.
#[tokio::test]
async fn run_streamed_forwards_configured_permission_mode_to_the_subprocess_argv() {
    use std::os::unix::fs::PermissionsExt;
    let script = std::env::temp_dir().join("lopi_claude_streamed_permmode_stub.sh");
    let capture = std::env::temp_dir().join("lopi_claude_streamed_permmode_capture.txt");
    std::fs::remove_file(&capture).ok();
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> {}; done\n",
            capture.display()
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let claude = ClaudeCode::new(".")
        .with_cli(script.to_str().unwrap())
        .with_permission_mode("dontAsk");
    let task = Task::new("streamed permission mode forwarding test");
    claude.plan_streamed(&task, None, |_| true).await.unwrap();

    let argv = std::fs::read_to_string(&capture).unwrap();
    assert!(argv.contains("--permission-mode\ndontAsk"), "argv={argv}");
    assert!(
        !argv.contains("--dangerously-skip-permissions"),
        "argv={argv}"
    );

    std::fs::remove_file(&script).ok();
    std::fs::remove_file(&capture).ok();
}

/// `with_permission_mode` mirrors `with_effort`'s validate-and-drop pattern:
/// an unrecognized value must not wedge the builder, and must leave the
/// field at its prior state rather than storing garbage.
#[test]
fn with_permission_mode_drops_unrecognized_values() {
    let c = ClaudeCode::new(".").with_permission_mode("not-a-real-mode");
    assert_eq!(c.permission_mode, None);
}

#[test]
fn with_permission_mode_accepts_every_headless_safe_value() {
    for mode in ["bypassPermissions", "auto", "acceptEdits", "dontAsk"] {
        let c = ClaudeCode::new(".").with_permission_mode(mode);
        assert_eq!(c.permission_mode.as_deref(), Some(mode));
    }
}
