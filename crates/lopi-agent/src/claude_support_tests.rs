#![allow(clippy::unwrap_used)]
//! `apply_cli_caps`/`build_cli_error`/`compress_errors`/session-continuity
//! tests — split out of `claude_support.rs` purely to keep that file under
//! the 500-line CI file-size gate; no behavioral difference from being
//! inline.

use super::*;
use std::os::unix::process::ExitStatusExt;

fn status(code: i32) -> ExitStatus {
    ExitStatus::from_raw(code << 8)
}

/// Collect the `(key, value)` env overrides set on a `Command`.
fn env_overrides(cmd: &Command) -> Vec<(String, String)> {
    cmd.as_std()
        .get_envs()
        .filter_map(|(k, v)| {
            v.map(|v| {
                (
                    k.to_string_lossy().into_owned(),
                    v.to_string_lossy().into_owned(),
                )
            })
        })
        .collect()
}

#[test]
fn apply_cli_caps_omits_optional_flags_for_none_and_empty() {
    let mut cmd = Command::new("true");
    apply_cli_caps(
        &mut cmd,
        None,
        None,
        None,
        None,
        None,
        &[],
        &[],
        false,
        SessionMode::None,
    );
    let argv: Vec<String> = cmd
        .as_std()
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // `--permission-mode` is never optional — it always falls back to
    // `PermissionMode::default()` (`bypassPermissions`) — everything else
    // stays a true no-op, including the new Sprint F4 session flags.
    assert_eq!(
        argv,
        vec!["--permission-mode", "bypassPermissions"],
        "argv={argv:?}"
    );
    // No model ⇒ no sub-agent pin: sub-agents inherit the CLI default.
    assert!(
        !env_overrides(&cmd)
            .iter()
            .any(|(k, _)| k == "CLAUDE_CODE_SUBAGENT_MODEL"),
        "sub-agent model must not be pinned when no --model is set"
    );
}

/// Sprint F2 Phase 6 — every one of lopi's three worker spawn sites
/// (`ClaudeCode::run`, `ClaudeCode::run_streamed`,
/// `claude_stream::plan_streaming`) calls `apply_cli_caps` with
/// `bare: false` explicitly; this proves that choice at the shared seam
/// so all three inherit it correctly, in the same shape as
/// `apply_cli_caps_includes_every_configured_flag`.
#[test]
fn apply_cli_caps_worker_sessions_never_pass_bare() {
    let mut cmd = Command::new("true");
    apply_cli_caps(
        &mut cmd,
        None,
        None,
        None,
        None,
        None,
        &[],
        &[],
        false,
        SessionMode::None,
    );
    let argv: Vec<String> = cmd
        .as_std()
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(
        !argv.contains(&"--bare".to_string()),
        "worker sessions must load repo context — --bare must be absent, argv={argv:?}"
    );
}

/// Sprint F4 Phase 1 — `SessionMode::New` emits `--session-id <id>`.
#[test]
fn apply_cli_caps_passes_session_id_when_new() {
    let mut cmd = Command::new("true");
    apply_cli_caps(
        &mut cmd,
        None,
        None,
        None,
        None,
        None,
        &[],
        &[],
        false,
        SessionMode::New("62faafd1-ea12-445a-9961-89ed21a151b8"),
    );
    let argv: Vec<String> = cmd
        .as_std()
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(argv.contains(&"--session-id".to_string()), "argv={argv:?}");
    assert!(
        argv.contains(&"62faafd1-ea12-445a-9961-89ed21a151b8".to_string()),
        "argv={argv:?}"
    );
    assert!(!argv.contains(&"--resume".to_string()), "argv={argv:?}");
}

/// Sprint F4 Phase 1 — `SessionMode::Resume` emits `--resume <id>`.
#[test]
fn apply_cli_caps_passes_resume_when_resuming() {
    let mut cmd = Command::new("true");
    apply_cli_caps(
        &mut cmd,
        None,
        None,
        None,
        None,
        None,
        &[],
        &[],
        false,
        SessionMode::Resume("35faaa8b-8553-4b16-a67e-348c1fac42ff"),
    );
    let argv: Vec<String> = cmd
        .as_std()
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(argv.contains(&"--resume".to_string()), "argv={argv:?}");
    assert!(
        argv.contains(&"35faaa8b-8553-4b16-a67e-348c1fac42ff".to_string()),
        "argv={argv:?}"
    );
    assert!(!argv.contains(&"--session-id".to_string()), "argv={argv:?}");
}

#[test]
fn scrub_inherited_anthropic_env_removes_parent_claude_code_session_id() {
    let mut cmd = Command::new("true");
    scrub_inherited_anthropic_env(&mut cmd);
    // `env_remove` records an explicit `(key, None)` override on the
    // `Command` so the child process never sees the key even if it's
    // set in lopi's own environment — checked directly (not via
    // `env_overrides`, which only surfaces `Some` values and would pass
    // trivially whether or not the removal actually happened).
    let removed: Vec<String> = cmd
        .as_std()
        .get_envs()
        .filter(|(_, v)| v.is_none())
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    for var in ["CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION"] {
        assert!(
            removed.iter().any(|k| k == var),
            "{var} must be explicitly removed so a nested claude -p spawn can't silently \
             inherit the parent session's identity; removed={removed:?}"
        );
    }
}

/// Sprint S10, Phase 1 — enumerates exactly what `apply_env_allowlist` adds,
/// so a future addition to `CHILD_ENV_ALLOWLIST` (or a regression back to
/// blind inheritance) shows up as a deliberate diff in this list, not a
/// silent behavior change.
#[test]
fn apply_env_allowlist_sets_only_the_allowlisted_vars_present_in_process_env() {
    // A secret that must never appear on the child, set directly in *this*
    // test process to prove it's excluded despite being present to inherit.
    std::env::set_var("LOPI_KT_S10_1_SECRET", "do-not-leak");

    let mut cmd = Command::new("true");
    apply_env_allowlist(&mut cmd);
    let overrides = env_overrides(&cmd);
    let keys: std::collections::BTreeSet<&str> =
        overrides.iter().map(|(k, _)| k.as_str()).collect();

    // Every key `apply_env_allowlist` ever sets must come from the
    // allowlist itself — nothing else, regardless of this process's env.
    for key in &keys {
        assert!(
            CHILD_ENV_ALLOWLIST.contains(key),
            "apply_env_allowlist set {key}, which is not in CHILD_ENV_ALLOWLIST: {overrides:?}"
        );
    }
    assert!(
        !keys.contains("LOPI_KT_S10_1_SECRET"),
        "a non-allowlisted var must never be set on the child: {overrides:?}"
    );

    std::env::remove_var("LOPI_KT_S10_1_SECRET");
}

/// The rejecting test: an actual spawned child (not just `Command`
/// introspection) must not see a secret that lopi's own process holds.
/// Live rather than mocked — `Command::env_clear`'s effect on inherited
/// variables isn't observable via `Command::get_envs()` at all (it only
/// ever reports explicit overrides), so this is the only way to prove the
/// child process itself doesn't see it.
#[tokio::test]
async fn apply_env_allowlist_child_process_cannot_see_a_non_allowlisted_secret() {
    let original_api_key = std::env::var("ANTHROPIC_API_KEY").ok();
    std::env::set_var("LOPI_KT_S10_1_SECRET", "do-not-leak");
    std::env::set_var("ANTHROPIC_API_KEY", "sk-should-not-leak-either");

    let mut cmd = tokio::process::Command::new("env");
    apply_env_allowlist(&mut cmd);
    let output = cmd.output().await.unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !stdout.contains("LOPI_KT_S10_1_SECRET"),
        "child process env leaked a non-allowlisted var:\n{stdout}"
    );
    assert!(
        !stdout.contains("ANTHROPIC_API_KEY"),
        "child process env leaked ANTHROPIC_API_KEY:\n{stdout}"
    );
    // Sanity: PATH is almost always set in a test environment and IS
    // allowlisted — if this fails too, the allowlist itself is broken, not
    // just over-strict, and the two assertions above would be meaningless.
    if std::env::var("PATH").is_ok() {
        assert!(
            stdout.contains("PATH="),
            "PATH should pass through:\n{stdout}"
        );
    }

    std::env::remove_var("LOPI_KT_S10_1_SECRET");
    match original_api_key {
        Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
        None => std::env::remove_var("ANTHROPIC_API_KEY"),
    }
}

#[test]
fn looks_like_session_establishment_failure_matches_the_live_kt_4_1_signature() {
    // .konjo/killtests/F4/KT-4.1.md: a bad --resume exits non-zero with
    // is_error: true and num_turns: 0, before a single turn runs.
    assert!(looks_like_session_establishment_failure(true, 0));
}

#[test]
fn looks_like_session_establishment_failure_is_false_after_real_work() {
    // A genuine mid-session failure (timeout, tool denial, real bug)
    // after at least one turn ran must NOT be treated as a resume
    // failure — retrying it cold would silently double-spend on a bug
    // that has nothing to do with the session.
    assert!(!looks_like_session_establishment_failure(true, 3));
    assert!(!looks_like_session_establishment_failure(false, 0));
}

/// The other half of the same pin: when a caller (e.g. a future
/// checker/post-mortem spawn site, per F1's handoff) asks for `bare:
/// true`, `--bare` must actually appear, and as the *first* argument —
/// checked here rather than merely "somewhere in argv" so a future
/// refactor can't accidentally place it after a value it would then be
/// mistaken for.
#[test]
fn apply_cli_caps_bare_flag_present_when_requested() {
    let mut cmd = Command::new("true");
    apply_cli_caps(
        &mut cmd,
        None,
        None,
        None,
        None,
        None,
        &[],
        &[],
        true,
        SessionMode::None,
    );
    let argv: Vec<String> = cmd
        .as_std()
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv.first(), Some(&"--bare".to_string()), "argv={argv:?}");
}

#[test]
fn normalize_effort_accepts_cli_levels_case_insensitively() {
    for (raw, want) in [
        ("low", Some("low")),
        ("  Medium ", Some("medium")),
        ("HIGH", Some("high")),
        ("xhigh", Some("xhigh")),
        ("Max", Some("max")),
        ("turbo", None),
        ("", None),
    ] {
        assert_eq!(normalize_effort(raw), want, "raw={raw:?}");
    }
}

#[test]
fn apply_cli_caps_pins_subagent_model_to_the_session_model() {
    let mut cmd = Command::new("true");
    apply_cli_caps(
        &mut cmd,
        Some("haiku"),
        None,
        None,
        None,
        None,
        &[],
        &[],
        false,
        SessionMode::None,
    );
    assert!(
        env_overrides(&cmd)
            .iter()
            .any(|(k, v)| k == "CLAUDE_CODE_SUBAGENT_MODEL" && v == "haiku"),
        "sub-agents must be pinned to the card's model so a Haiku card \
         can't fan out pricier sub-agents"
    );
}

#[test]
fn apply_cli_caps_includes_every_configured_flag() {
    let mut cmd = Command::new("true");
    apply_cli_caps(
        &mut cmd,
        Some("claude-opus-5"),
        Some("high"),
        Some("dontAsk"),
        Some(5),
        Some(2.5),
        &["Bash".to_string()],
        &["Workflow".to_string()],
        false,
        SessionMode::Resume("35faaa8b-8553-4b16-a67e-348c1fac42ff"),
    );
    let argv: Vec<String> = cmd
        .as_std()
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        argv,
        vec![
            "--permission-mode",
            "dontAsk",
            "--resume",
            "35faaa8b-8553-4b16-a67e-348c1fac42ff",
            "--model",
            "claude-opus-5",
            "--effort",
            "high",
            "--max-turns",
            "5",
            "--max-budget-usd",
            "2.5",
            "--allowedTools",
            "Bash",
            "--disallowedTools",
            "Workflow",
        ]
    );
}

#[test]
fn build_cli_error_hard_stops_on_credit_exhaustion() {
    let stdout = r#"{"result":"Your credit balance is too low","api_error_status":402}"#;
    let err = build_cli_error(stdout, "", status(1), Path::new("."), 10);
    assert!(err
        .to_string()
        .contains(crate::claude::ERR_CREDIT_EXHAUSTED));
}

#[test]
fn build_cli_error_surfaces_the_parsed_api_message() {
    let stdout = r#"{"result":"rate limited","api_error_status":429}"#;
    let err = build_cli_error(stdout, "", status(1), Path::new("."), 10);
    let msg = err.to_string();
    assert!(msg.contains("rate limited"));
    assert!(msg.contains("429"));
}

#[test]
fn build_cli_error_falls_back_to_raw_streams_when_unparseable() {
    let err = build_cli_error("not json", "boom", status(1), Path::new("."), 10);
    let msg = err.to_string();
    assert!(msg.contains("boom"));
    assert!(msg.contains("not json"));
}

#[test]
fn compress_errors_removes_backtrace_noise() {
    let errors = vec![
        "error[E0308]: mismatched types\n  at src/main.rs:10\nnote: run with RUST_BACKTRACE=1\nstack backtrace:\n  at src/foo.rs:5".to_string(),
    ];
    let out = compress_errors(&errors);
    assert!(!out.contains("RUST_BACKTRACE"));
    assert!(!out.contains("stack backtrace:"));
    assert!(!out.contains("at src/"));
    assert!(out.contains("mismatched types"));
}

#[test]
fn compress_errors_deduplicates_identical_blocks() {
    let block = "error: cannot borrow as mutable".to_string();
    let errors = vec![block.clone(), block.clone(), block.clone()];
    let out = compress_errors(&errors);
    // Only one copy should survive deduplication
    assert_eq!(out.matches("cannot borrow").count(), 1);
}
