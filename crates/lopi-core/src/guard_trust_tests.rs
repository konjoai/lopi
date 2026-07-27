#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::LoopConfig;

#[tokio::test]
async fn run_guard_command_true_and_false() {
    let dir = std::env::temp_dir();
    assert!(run_guard_command("true", &dir).await.unwrap());
    assert!(!run_guard_command("false", &dir).await.unwrap());
}

#[tokio::test]
async fn run_guard_command_reflects_exit_code() {
    let dir = std::env::temp_dir();
    assert!(run_guard_command("exit 0", &dir).await.unwrap());
    assert!(!run_guard_command("exit 1", &dir).await.unwrap());
}

#[tokio::test]
async fn run_guard_command_runs_in_the_given_cwd() {
    // A command that depends on cwd — proves `current_dir` is actually wired,
    // not just a fixed invocation.
    let dir = std::env::temp_dir();
    let marker = dir.join("lopi_guard_cwd_marker");
    let _ = std::fs::remove_file(&marker);
    std::fs::write(&marker, "x").unwrap();
    assert!(run_guard_command("test -f lopi_guard_cwd_marker", &dir)
        .await
        .unwrap());
    let _ = std::fs::remove_file(&marker);
}

// Sprint S10, Phase 0 — `resolve_guard_command` trust resolution.

#[test]
fn resolve_guard_command_operator_value_always_wins() {
    assert_eq!(
        resolve_guard_command(Some("repo cmd"), Some("operator cmd"), true),
        Some("operator cmd".to_string())
    );
    // Operator override wins even when the source is untrusted — it cannot
    // have arrived via a branch under evaluation.
    assert_eq!(
        resolve_guard_command(Some("repo cmd"), Some("operator cmd"), false),
        Some("operator cmd".to_string())
    );
}

#[test]
fn resolve_guard_command_repo_value_honored_only_when_trusted() {
    assert_eq!(
        resolve_guard_command(Some("repo cmd"), None, true),
        Some("repo cmd".to_string())
    );
}

/// The rejecting test: an untrusted-sourced repo command with no operator
/// override must resolve to `None` — dropped, not queued, not executed.
#[test]
fn resolve_guard_command_refuses_repo_value_when_untrusted() {
    assert_eq!(resolve_guard_command(Some("repo cmd"), None, false), None);
}

#[test]
fn resolve_guard_command_none_stays_none() {
    assert_eq!(resolve_guard_command(None, None, true), None);
    assert_eq!(resolve_guard_command(None, None, false), None);
}

/// KT-S10.0 (BLOCKING) — Sprint S10, Phase 0's kill test. A `.lopi/loop.toml`
/// as if added by a pull request under evaluation sets a `gate` that writes
/// a marker file to disk. A task dispatched against that repo via the
/// webhook path (`TaskSource::Webhook`) must never execute it.
///
/// Mirrors the exact sequence `lopi-orchestrator`'s `run_one` runs against a
/// real repo on disk: [`LoopConfig::load_from_repo`] →
/// [`crate::is_untrusted_source`] → [`resolve_guard_command`] → (only if
/// `Some`) [`run_guard_command`]. Before the Phase 0 fix, this test's
/// pre-fix equivalent (`run_guard_command(cfg.gate, &dir)` with no
/// resolution step) leaves the marker on disk — that was the finding, per
/// the sprint brief: "marker exists" was the expected, severity-confirming
/// result. Post-fix, `resolve_guard_command` refuses the value before it
/// ever reaches `run_guard_command`, and the marker is never created.
#[tokio::test]
async fn kt_s10_0_webhook_sourced_task_cannot_execute_repo_supplied_gate() {
    let dir = std::env::temp_dir().join(format!(
        "lopi_kt_s10_0_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".lopi")).unwrap();
    let marker = dir.join("kt-s10-0-marker");
    let _ = std::fs::remove_file(&marker);
    std::fs::write(
        dir.join(".lopi").join("loop.toml"),
        format!("gate = \"touch {}\"\n", marker.display()),
    )
    .unwrap();

    // Step 1: load the repo's own config, exactly as `run_one` does.
    let cfg = LoopConfig::load_from_repo(&dir).unwrap();
    assert!(
        cfg.gate.is_some(),
        "sanity: the repo config really does carry a gate"
    );

    // Step 2: classify the task's source, exactly as `run_one` does. A
    // GitHub PR/CI event is exactly what `queue_ci_fix`/`handle_pr_review`
    // (`lopi-webhook`) construct.
    let source = crate::TaskSource::Webhook {
        repo: "attacker/repo".into(),
        event: "pull_request".into(),
    };
    let source_trusted = !crate::is_untrusted_source(&source);
    assert!(!source_trusted, "sanity: Webhook is classified untrusted");

    // Step 3: resolve — this is the Phase 0 fix.
    let resolved = resolve_guard_command(cfg.gate.as_deref(), None, source_trusted);
    assert!(
        resolved.is_none(),
        "Phase 0: a repo-supplied gate must be refused for an untrusted task source"
    );

    // Step 4: only a resolved (trusted) value ever reaches the shell-out —
    // this branch is unreachable post-fix, proving the refusal above is
    // actually what stands between the webhook path and code execution.
    if let Some(cmd) = resolved {
        run_guard_command(&cmd, &dir).await.unwrap();
    }
    assert!(
        !marker.exists(),
        "KT-S10.0 FAIL: repo-supplied gate executed for a webhook-sourced task"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_operator_overrides_absent_home_config_is_none() {
    let original = std::env::var("HOME").ok();
    let empty_home = std::env::temp_dir().join("lopi_operator_overrides_absent_test");
    let _ = std::fs::remove_dir_all(&empty_home);
    std::fs::create_dir_all(&empty_home).unwrap();
    std::env::set_var("HOME", &empty_home);

    assert!(LoopConfig::load_operator_overrides().is_none());

    match original {
        Some(h) => std::env::set_var("HOME", h),
        None => std::env::remove_var("HOME"),
    }
    let _ = std::fs::remove_dir_all(&empty_home);
}

#[test]
fn load_operator_overrides_reads_gate_from_home_lopi_dir() {
    let original = std::env::var("HOME").ok();
    let home = std::env::temp_dir().join("lopi_operator_overrides_present_test");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".lopi")).unwrap();
    std::fs::write(
        home.join(".lopi").join("loop.toml"),
        "gate = \"echo operator-gate\"\n",
    )
    .unwrap();
    std::env::set_var("HOME", &home);

    let cfg = LoopConfig::load_operator_overrides().expect("operator config should parse");
    assert_eq!(cfg.gate.as_deref(), Some("echo operator-gate"));

    match original {
        Some(h) => std::env::set_var("HOME", h),
        None => std::env::remove_var("HOME"),
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn load_operator_overrides_malformed_toml_is_none_not_a_panic() {
    let original = std::env::var("HOME").ok();
    let home = std::env::temp_dir().join("lopi_operator_overrides_malformed_test");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".lopi")).unwrap();
    std::fs::write(home.join(".lopi").join("loop.toml"), "not valid toml {{{").unwrap();
    std::env::set_var("HOME", &home);

    assert!(LoopConfig::load_operator_overrides().is_none());

    match original {
        Some(h) => std::env::set_var("HOME", h),
        None => std::env::remove_var("HOME"),
    }
    let _ = std::fs::remove_dir_all(&home);
}
