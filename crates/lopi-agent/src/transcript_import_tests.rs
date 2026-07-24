//! Unit tests for `transcript_import.rs` — split out to keep the decoder
//! module under the 500-line file gate. Included via `#[path]` so
//! `super::*` still resolves to the decoder's items.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn blank_and_malformed_lines_never_panic() {
    assert_eq!(parse_line(""), TranscriptEvent::Other);
    assert_eq!(parse_line("   "), TranscriptEvent::Other);
    assert_eq!(parse_line("not json at all"), TranscriptEvent::Other);
    assert_eq!(parse_line(r#"{"truncated":"#), TranscriptEvent::Other);
    assert_eq!(parse_line(r#"{"no_type":true}"#), TranscriptEvent::Other);
    assert_eq!(
        parse_line(r#"{"type":"queue-operation","operation":"enqueue"}"#),
        TranscriptEvent::Other
    );
    assert_eq!(
        parse_line(r#"{"type":"attachment","attachment":{}}"#),
        TranscriptEvent::Other
    );
    assert_eq!(
        parse_line(r#"{"type":"last-prompt","lastPrompt":"x"}"#),
        TranscriptEvent::Other
    );
}

/// KT-A's core finding: a plain-string `message.content` on a `type:"user"`
/// line is a genuine human turn — the shape a real captured session
/// transcript line actually has.
#[test]
fn plain_string_user_content_is_a_human_turn() {
    let line = r#"{"type":"user","message":{"role":"user","content":"fix the flaky test"},"cwd":"/home/user/lopi","sessionId":"abc-123"}"#;
    match parse_line(line) {
        TranscriptEvent::HumanTurn {
            text,
            cwd,
            session_id,
        } => {
            assert_eq!(text, "fix the flaky test");
            assert_eq!(cwd.as_deref(), Some("/home/user/lopi"));
            assert_eq!(session_id.as_deref(), Some("abc-123"));
        }
        other => panic!("expected HumanTurn, got {other:?}"),
    }
}

/// KT-A's core finding, the other half: a `type:"user"` line whose
/// `message.content` is a list containing a `tool_result` block is a tool
/// result wrapped in a user-shaped envelope, not a human turn — the exact
/// ambiguity `claude_events.rs` had to handle for the live-stream format,
/// confirmed present here too against a real captured sample.
#[test]
fn list_content_with_tool_result_block_is_not_a_human_turn() {
    let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok","is_error":false,"tool_use_id":"t1"}]},"toolUseResult":"ok","cwd":"/home/user/lopi","sessionId":"abc-123"}"#;
    assert_eq!(
        parse_line(line),
        TranscriptEvent::ToolResult { is_error: false }
    );
}

#[test]
fn tool_result_error_flag_is_read_from_the_block() {
    let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"boom","is_error":true,"tool_use_id":"t1"}]}}"#;
    assert_eq!(
        parse_line(line),
        TranscriptEvent::ToolResult { is_error: true }
    );
}

/// A list `content` on a `type:"user"` line that carries no `tool_result`
/// block (e.g. a human turn with an image attachment) is still a genuine
/// human turn — defensively concatenate its text blocks.
#[test]
fn list_content_without_tool_result_is_still_a_human_turn() {
    let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"look at this"},{"type":"image","source":{}}]}}"#;
    match parse_line(line) {
        TranscriptEvent::HumanTurn { text, .. } => assert_eq!(text, "look at this"),
        other => panic!("expected HumanTurn, got {other:?}"),
    }
}

#[test]
fn assistant_text_blocks_are_concatenated() {
    let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"done"},{"type":"tool_use","name":"Bash","input":{}}]}}"#;
    assert_eq!(
        parse_line(line),
        TranscriptEvent::AssistantTurn {
            text: "done".to_string()
        }
    );
}

#[test]
fn assistant_line_with_no_text_block_is_other() {
    let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{}}]}}"#;
    assert_eq!(parse_line(line), TranscriptEvent::Other);
}

fn write_transcript(dir: &std::path::Path, name: &str, lines: &[&str]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, lines.join("\n")).unwrap();
    path
}

#[test]
fn parse_session_file_groups_turns_and_falls_back_session_id_to_filename() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_transcript(
        dir.path(),
        "session-xyz.jsonl",
        &[
            r#"{"type":"user","message":{"role":"user","content":"add a rate limiter"},"cwd":"/repo","sessionId":"session-xyz"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"working on it"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok","is_error":false}]}}"#,
            "not even json",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"done, tests pass"}]}}"#,
        ],
    );

    let session = parse_session_file(&path).unwrap();
    assert_eq!(session.session_id, "session-xyz");
    assert_eq!(session.project_dir.as_deref(), Some("/repo"));
    assert_eq!(session.human_turns, vec!["add a rate limiter"]);
    assert_eq!(
        session.assistant_texts,
        vec!["working on it", "done, tests pass"]
    );
    assert_eq!(session.tool_result_errors, vec![false]);
}

#[test]
fn parse_session_file_missing_falls_back_to_filename_stem_when_no_session_id_present() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_transcript(
        dir.path(),
        "12345.jsonl",
        &[r#"{"type":"user","message":{"role":"user","content":"hello"}}"#],
    );
    let session = parse_session_file(&path).unwrap();
    assert_eq!(session.session_id, "12345");
}

#[test]
fn parse_session_file_errors_on_a_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    assert!(parse_session_file(&dir.path().join("nope.jsonl")).is_err());
}

#[test]
fn discover_transcripts_walks_project_directories_one_level_deep() {
    let dir = tempfile::tempdir().unwrap();
    let claude_dir = dir.path();
    let proj_a = claude_dir.join("projects").join("-home-user-lopi");
    let proj_b = claude_dir.join("projects").join("-home-user-squish");
    std::fs::create_dir_all(&proj_a).unwrap();
    std::fs::create_dir_all(&proj_b).unwrap();
    write_transcript(&proj_a, "s1.jsonl", &["{}"]);
    write_transcript(&proj_a, "notes.txt", &["ignore me"]);
    write_transcript(&proj_b, "s2.jsonl", &["{}"]);

    let mut found = discover_transcripts(claude_dir);
    found.sort();
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|p| p.extension().and_then(|e| e.to_str()) == Some("jsonl")));
}

#[test]
fn discover_transcripts_on_a_missing_claude_dir_returns_empty_not_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist");
    assert!(discover_transcripts(&missing).is_empty());
}

fn session_with(assistant_texts: Vec<&str>, tool_errors: Vec<bool>) -> HistoricalSession {
    HistoricalSession {
        session_id: "s".to_string(),
        project_dir: Some("/repo".to_string()),
        human_turns: vec!["do the thing".to_string()],
        assistant_texts: assistant_texts.into_iter().map(str::to_string).collect(),
        tool_result_errors: tool_errors,
    }
}

#[test]
fn session_looks_successful_requires_both_clean_tail_and_success_language() {
    // Clean tail + explicit success language → true.
    assert!(session_looks_successful(&session_with(
        vec!["all done, tests pass now"],
        vec![false, false, false],
    )));

    // Error in the tail → false, even with success-sounding final text.
    assert!(!session_looks_successful(&session_with(
        vec!["tests pass"],
        vec![false, true],
    )));

    // Clean tail but no explicit success language → false.
    assert!(!session_looks_successful(&session_with(
        vec!["here is a summary of what changed"],
        vec![false],
    )));

    // Clean tail, success word present, but a failure word also present → false.
    assert!(!session_looks_successful(&session_with(
        vec!["fixed one bug but there is still an error elsewhere"],
        vec![false],
    )));

    // No assistant text at all → false (nothing to confirm success with).
    assert!(!session_looks_successful(&session_with(vec![], vec![false])));
}

#[test]
fn session_looks_successful_only_checks_the_trailing_tool_results() {
    // An error early in a long session, but the tail is clean and the
    // final text confirms success — the heuristic should still pass.
    let mut errors = vec![true];
    errors.extend(std::iter::repeat_n(false, TAIL_WINDOW));
    let session = session_with(vec!["fixed it, all tests passing"], errors);
    assert!(session_looks_successful(&session));
}

#[test]
fn extract_success_constraint_returns_none_when_heuristic_fails() {
    let session = session_with(vec!["still broken"], vec![true]);
    assert!(extract_success_constraint(&session).is_none());
}

#[test]
fn extract_success_constraint_truncates_long_text() {
    let long = "x".repeat(CONSTRAINT_CAP + 50) + " tests pass";
    let session = session_with(vec![&long], vec![false]);
    let constraint = extract_success_constraint(&session).unwrap();
    assert!(constraint.chars().count() <= CONSTRAINT_CAP + 1); // +1 for the "…"
    assert!(constraint.ends_with('…'));
}
