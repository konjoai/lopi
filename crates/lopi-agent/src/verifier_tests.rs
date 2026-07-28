//! Split out of `verifier.rs` purely to keep that file under the CLAUDE.md
//! file-size budget — pure code motion, same pattern as `test_phase.rs`'s
//! own doc comment.

#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn strip_fences_removes_markdown_wrapper() {
    assert_eq!(
        strip_fences("```json\n{\"passed\":true}\n```"),
        "{\"passed\":true}"
    );
}

#[test]
fn strip_fences_passthrough_for_clean_json() {
    assert_eq!(strip_fences("{\"passed\":false}"), "{\"passed\":false}");
}

#[test]
fn parse_verdict_valid_json() {
    let raw = r#"{"passed":true,"gaps":[],"fix_hints":[],"confidence":0.9}"#;
    let v = parse_verdict(raw).unwrap();
    assert!(v.passed);
    assert!(v.gaps.is_empty());
    assert!((v.confidence - 0.9).abs() < 1e-6);
}

#[test]
fn parse_verdict_failed_with_hints() {
    let raw = r#"{"passed":false,"gaps":["tests do not cover new branch"],"fix_hints":["add test for the else branch"],"confidence":0.8}"#;
    let v = parse_verdict(raw).unwrap();
    assert!(!v.passed);
    assert_eq!(v.gaps.len(), 1);
    assert_eq!(v.fix_hints[0], "add test for the else branch");
}

#[test]
fn parse_verdict_invalid_json_returns_err() {
    assert!(parse_verdict("not json").is_err());
}

fn sample_rubric() -> Rubric {
    Rubric {
        name: "safety".into(),
        criteria: vec!["tests pass".into(), "no scope creep".into()],
    }
}

/// Maker/checker isolation (Pentad M4.1): the isolated prompt must NOT
/// contain the maker's plan, so the checker cannot be anchored to it.
#[test]
fn isolated_prompt_excludes_the_maker_plan() {
    let plan = "MAKER SECRET REASONING: I hacked the test to pass";
    let prompt = build_prompt(
        "fix the bug",
        plan,
        "diff --git a b",
        "ok",
        &sample_rubric(),
        false, // isolated
        &[],
    );
    assert!(!prompt.contains("MAKER SECRET REASONING"), "plan excluded");
    assert!(!prompt.contains("PLAN (excerpt)"), "no plan section header");
    // The artifact + intent + rubric are still present.
    assert!(prompt.contains("GOAL:\nfix the bug"));
    assert!(prompt.contains("DIFF (excerpt):\ndiff --git a b"));
    assert!(prompt.contains("RUBRIC (safety):"));
    assert!(prompt.contains("- no scope creep"));
}

#[test]
fn plan_context_mode_includes_the_plan() {
    let prompt = build_prompt(
        "fix the bug",
        "MAKER REASONING here",
        "diff",
        "ok",
        &sample_rubric(),
        true, // include plan (legacy)
        &[],
    );
    assert!(prompt.contains("PLAN (excerpt):\nMAKER REASONING here"));
}

/// A diff/plan/test-output whose excerpt cutoff lands mid-multibyte-char
/// must not panic ("byte index N is not a char boundary").
#[test]
fn build_prompt_does_not_panic_on_multibyte_boundary() {
    // "🦀" is 4 bytes; pad so the excerpt cutoffs (6000/1500/1000) fall
    // squarely inside the emoji rather than before or after it.
    let diff = format!("{}🦀{}", "d".repeat(5_999), "e".repeat(50));
    let plan = format!("{}🦀{}", "p".repeat(1_499), "q".repeat(50));
    let test_output = format!("{}🦀{}", "t".repeat(999), "u".repeat(50));
    let prompt = build_prompt(
        "goal",
        &plan,
        &diff,
        &test_output,
        &sample_rubric(),
        true,
        &[],
    );
    // Must not panic, and must not contain a truncated (invalid) partial
    // emoji — Rust's `String` type guarantees well-formed UTF-8, so if
    // this compiles and runs it already proves no mid-char slice occurred.
    assert!(prompt.contains("GOAL:\ngoal"));
}

#[test]
fn new_verifier_is_isolated_by_default_and_builder_opts_out() {
    let client = std::sync::Arc::new(crate::api_client::AnthropicClient::new("test-key"));
    assert!(
        VerifierAgent::new(client.clone()).isolated,
        "isolated by default"
    );
    assert!(
        !VerifierAgent::new(client).with_plan_context().isolated,
        "builder opts back into plan context"
    );
}

#[test]
fn default_rubric_has_criteria() {
    let r = default_rubric();
    assert!(!r.criteria.is_empty());
    assert_eq!(r.name, "default");
}

// ── Verifier as Explicit Gate — model/effort resolver ───────────────────

#[test]
fn resolve_verifier_defaults_to_opus_for_a_non_opus_worker() {
    let (model, effort) = resolve_verifier(model_sonnet(), None, None);
    assert_eq!(model, model_opus());
    assert!(effort.is_none());
}

#[test]
fn resolve_verifier_never_grades_its_own_homework() {
    // The one case where the default (Opus) would equal the worker: an
    // escalated retry already running on Opus. The resolver must pick a
    // different model instead of silently grading itself.
    let (model, _) = resolve_verifier(model_opus(), None, None);
    assert_ne!(model, model_opus());
    assert_eq!(model, model_sonnet());
}

#[test]
fn resolve_verifier_honors_an_explicit_override() {
    let (model, _) = resolve_verifier(model_sonnet(), Some(crate::claude::model_haiku()), None);
    assert_eq!(model, crate::claude::model_haiku());
}

#[test]
fn resolve_verifier_passes_effort_through_unchanged() {
    let (_, effort) = resolve_verifier(model_sonnet(), None, Some("high"));
    assert_eq!(effort.as_deref(), Some("high"));
}

#[test]
fn build_system_prompt_appends_effort_hint_when_set() {
    let prompt = build_system_prompt(Some("high"));
    assert!(prompt.starts_with(VERIFIER_SYSTEM));
    assert!(prompt.contains("Reasoning effort: high"));
}

#[test]
fn build_system_prompt_is_unchanged_when_effort_absent() {
    assert_eq!(build_system_prompt(None), VERIFIER_SYSTEM);
}

#[tokio::test]
async fn resolve_rubric_prefers_inline_task_rubric() {
    let inline = Rubric {
        name: "inline".into(),
        criteria: vec!["only this".into()],
    };
    let resolved = resolve_rubric(Some(inline), std::path::Path::new("/nonexistent")).await;
    assert_eq!(resolved.name, "inline");
}

#[tokio::test]
async fn resolve_rubric_loads_file_when_no_inline() {
    let dir = std::env::temp_dir().join(format!("lopi-rubric-{}", std::process::id()));
    let rubric_dir = dir.join(RUBRIC_DIR);
    tokio::fs::create_dir_all(&rubric_dir).await.unwrap();
    tokio::fs::write(
        rubric_dir.join("feature_completeness.toml"),
        "name = \"from_disk\"\ncriteria = [\"loaded from file\"]\n",
    )
    .await
    .unwrap();
    let resolved = resolve_rubric(None, &dir).await;
    assert_eq!(resolved.name, "from_disk");
    tokio::fs::remove_dir_all(&dir).await.ok();
}

#[tokio::test]
async fn resolve_rubric_falls_back_to_default_when_file_absent() {
    let resolved = resolve_rubric(None, std::path::Path::new("/nonexistent")).await;
    assert_eq!(resolved.name, "default");
}

#[tokio::test]
async fn load_rubric_file_returns_none_for_missing() {
    assert!(load_rubric_file(std::path::Path::new("/nonexistent"), "x")
        .await
        .is_none());
}

// ── Finding #1 — checklist-before-diff ───────────────────────────────────

#[test]
fn checklist_prompt_never_mentions_diff_or_code() {
    let rubric = sample_rubric();
    let prompt = build_checklist_prompt("fix the auth bug", &rubric);
    assert!(prompt.contains("GOAL:\nfix the auth bug"));
    assert!(prompt.contains("RUBRIC (safety):"));
    assert!(prompt.contains("- no scope creep"));
    // No diff/code content of any kind — `build_checklist_prompt` takes no
    // diff parameter at all, so this is a structural guarantee, not a
    // filtering one. (The prompt text itself does say the word "diff" once,
    // to tell the model it hasn't seen one — that's the point, not a leak.)
    assert!(!prompt.contains("DIFF"));
    assert!(!prompt.contains("```"));
}

#[test]
fn checklist_system_prompt_states_no_code_seen_yet() {
    assert!(CHECKLIST_SYSTEM.contains("have NOT been shown any code"));
}

#[test]
fn build_checklist_system_prompt_appends_effort_hint() {
    let prompt = build_checklist_system_prompt(Some("high"));
    assert!(prompt.starts_with(CHECKLIST_SYSTEM));
    assert!(prompt.contains("Reasoning effort: high"));
}

#[test]
fn build_checklist_system_prompt_unchanged_when_effort_absent() {
    assert_eq!(build_checklist_system_prompt(None), CHECKLIST_SYSTEM);
}

#[test]
fn parse_checklist_valid_json() {
    let raw =
        r#"{"checklist":["handles the empty-input case","rejects unauthenticated requests"]}"#;
    let items = parse_checklist(raw).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], "handles the empty-input case");
}

#[test]
fn parse_checklist_strips_markdown_fences() {
    let raw = "```json\n{\"checklist\":[\"one criterion\"]}\n```";
    assert_eq!(
        parse_checklist(raw).unwrap(),
        vec!["one criterion".to_string()]
    );
}

#[test]
fn parse_checklist_invalid_json_returns_err() {
    assert!(parse_checklist("not json").is_err());
}

#[test]
fn parse_checklist_empty_array_is_fine() {
    assert_eq!(
        parse_checklist(r#"{"checklist":[]}"#).unwrap(),
        Vec::<String>::new()
    );
}

/// The grading prompt folds in the checker's own pre-derived checklist as a
/// distinct, labelled section — not silently merged into the rubric section,
/// so it's visibly "what the checker itself committed to before seeing code".
#[test]
fn build_prompt_includes_the_checklist_section_when_present() {
    let checklist = vec![
        "validates the input length".to_string(),
        "returns a typed error, not a panic".to_string(),
    ];
    let prompt = build_prompt(
        "add input validation",
        "",
        "diff --git a b",
        "ok",
        &sample_rubric(),
        false,
        &checklist,
    );
    assert!(prompt.contains("YOUR OWN CHECKLIST (written before you saw any code):"));
    assert!(prompt.contains("- validates the input length"));
    assert!(prompt.contains("- returns a typed error, not a panic"));
    // The checklist section must appear before the diff — grading against
    // criteria already committed to, not the reverse.
    let checklist_pos = prompt.find("YOUR OWN CHECKLIST").unwrap();
    let diff_pos = prompt.find("DIFF (excerpt)").unwrap();
    assert!(checklist_pos < diff_pos, "checklist must precede the diff");
}

#[test]
fn build_prompt_omits_the_checklist_section_when_empty() {
    let prompt = build_prompt("goal", "", "diff", "ok", &sample_rubric(), false, &[]);
    assert!(!prompt.contains("YOUR OWN CHECKLIST"));
}

/// End-to-end fail-closed check through the real two-call `verify` path: a
/// repo path that cannot exist makes the CLI spawn's `current_dir` fail
/// deterministically for *both* the checklist call and the grading call, so
/// `verify` must still surface an overall `Err` (never silently swallow the
/// checklist failure and then also silently pass the grading step) — same
/// fixture pattern `verifier_runner.rs`'s
/// `requested_but_unavailable_verifier_fails_closed` uses.
#[tokio::test]
async fn verify_stays_fail_closed_end_to_end_when_the_repo_is_unreachable() {
    let verifier = VerifierAgent::new_cli(std::path::PathBuf::from(
        "/nonexistent/path/that/cannot/possibly/exist/lopi-g1-checklist-kt",
    ));
    let result = verifier
        .verify(
            "prove the two-call gate can't silently pass",
            "",
            "diff --git a b",
            "",
            &sample_rubric(),
            model_opus(),
            None,
        )
        .await;
    assert!(
        result.is_err(),
        "an unreachable backend must fail the whole verify() call, not just the checklist step"
    );
}

/// A checklist-derivation failure alone (isolated from the grading call)
/// must not panic and must return `Err` rather than a bogus checklist —
/// `verify`'s own fallback (empty checklist, warn) is what makes this safe
/// to call speculatively.
#[tokio::test]
async fn derive_checklist_fails_closed_on_an_unreachable_backend() {
    let verifier = VerifierAgent::new_cli(std::path::PathBuf::from(
        "/nonexistent/path/that/cannot/possibly/exist/lopi-g1-checklist-kt2",
    ));
    let result = verifier
        .derive_checklist("some goal", &sample_rubric(), model_opus(), None)
        .await;
    assert!(result.is_err());
}
