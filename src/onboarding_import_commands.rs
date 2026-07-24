//! Onboarding-Import-1 (Phase 5) — `lopi import` CLI orchestration.
//!
//! Walks `~/.claude/projects` for historical Claude Code session
//! transcripts (via `lopi_agent::transcript_import`), detects each
//! session's project toolchain (via `crate::toolchain_detect`), and
//! backfills `lopi-memory`'s `patterns` table (via
//! `MemoryStore::backfill_onboarding_pattern`) so a new lopi install
//! starts with real signal instead of a cold store.
//!
//! Idempotent on session id — a re-run (reinstall, new machine) skips
//! sessions already imported. `--dry-run` opens the store read-only (to
//! report accurate already-imported/would-import status) but writes
//! nothing.

use anyhow::Result;
use lopi_agent::transcript_import::{
    discover_transcripts, extract_success_constraint, parse_session_file, HistoricalSession,
};
use lopi_memory::{BackfillOutcome, MemoryStore, OnboardingPattern};
use std::path::{Path, PathBuf};

use crate::toolchain_detect::detect_toolchain;

/// Default `~/.claude` directory location, mirroring `util::db_path`'s
/// `$HOME`-based resolution (no extra `dirs`-crate dependency).
fn default_claude_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".claude")
}

/// Best-effort label for a session with no recorded `cwd` — the encoded
/// project-directory folder name under `~/.claude/projects/`, still useful
/// for the `onboarding_imports` audit trail even though it isn't a
/// resolvable filesystem path for toolchain detection.
fn fallback_project_label(transcript_path: &Path) -> String {
    transcript_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// One session's derived backfill inputs, before hitting the store.
struct Candidate {
    session_id: String,
    project_dir: String,
    goal: String,
    toolchain: Option<String>,
    constraint: Option<String>,
}

fn derive_candidate(transcript_path: &Path, session: &HistoricalSession) -> Option<Candidate> {
    // Nothing to mine a goal from — the whole point of Phase 3's fingerprint
    // is a human-authored goal string.
    let goal = session.human_turns.first()?.clone();
    let project_dir = session
        .project_dir
        .clone()
        .unwrap_or_else(|| fallback_project_label(transcript_path));
    // Toolchain detection needs a real filesystem path (Phase 2's whole
    // premise); the fallback label above is not one, so only attempt
    // detection when the transcript actually carried a `cwd`.
    let toolchain = session
        .project_dir
        .as_deref()
        .and_then(|dir| detect_toolchain(Path::new(dir)));
    Some(Candidate {
        session_id: session.session_id.clone(),
        project_dir,
        goal,
        toolchain,
        constraint: extract_success_constraint(session),
    })
}

/// Per-run tallies, returned so callers (tests, primarily — the CLI just
/// prints them) can assert on the actual counts instead of only on the
/// printed summary line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportSummary {
    /// Sessions inserted (or, in `--dry-run`, that would have been).
    pub imported: u32,
    /// Sessions already present in `onboarding_imports` — skipped.
    pub already_imported: u32,
    /// Sessions with no human turn to derive a goal from — skipped.
    pub skipped_no_goal: u32,
    /// Sessions whose goal produced an empty keyword fingerprint — skipped.
    pub skipped_empty_fingerprint: u32,
}

/// Run the onboarding import. `dry_run` opens the store read-only to report
/// accurate status but performs no writes.
///
/// # Errors
/// Returns `Err` if the store can't be opened.
pub async fn run(dry_run: bool, claude_dir: Option<PathBuf>, db_path: PathBuf) -> Result<()> {
    run_import(dry_run, claude_dir, db_path).await?;
    Ok(())
}

/// The actual import logic, split out from [`run`] so tests can assert on
/// the resulting [`ImportSummary`] directly rather than only on stdout.
async fn run_import(
    dry_run: bool,
    claude_dir: Option<PathBuf>,
    db_path: PathBuf,
) -> Result<ImportSummary> {
    let claude_dir = claude_dir.unwrap_or_else(default_claude_dir);
    let transcripts = discover_transcripts(&claude_dir);

    println!(
        "🧭 lopi import — scanning {} ({} transcript file{} found)",
        claude_dir.display(),
        transcripts.len(),
        if transcripts.len() == 1 { "" } else { "s" }
    );
    if transcripts.is_empty() {
        println!(
            "  No transcripts found. Nothing to import — this is a normal state on a \
             fresh machine or a fresh `~/.claude` install."
        );
        return Ok(ImportSummary::default());
    }

    let store = MemoryStore::open(&db_path).await?;
    let mut summary = ImportSummary::default();

    for path in &transcripts {
        let session = match parse_session_file(path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("skipping unreadable transcript {}: {e:#}", path.display());
                continue;
            }
        };
        let Some(candidate) = derive_candidate(path, &session) else {
            summary.skipped_no_goal += 1;
            continue;
        };

        let short_id = &candidate.session_id[..8.min(candidate.session_id.len())];
        let toolchain_label = candidate.toolchain.as_deref().unwrap_or("unknown");
        let goal_preview = preview(&candidate.goal, 60);

        if dry_run {
            // Dry-run must never call backfill_onboarding_pattern — a genuine
            // insert there would write — so the idempotency check happens
            // here instead, read-only, purely to report accurate status.
            if store
                .onboarding_session_imported(&candidate.session_id)
                .await?
            {
                summary.already_imported += 1;
            } else {
                println!(
                    "  [dry-run] would import {short_id} · {toolchain_label} · \"{goal_preview}\"{}",
                    if candidate.constraint.is_some() {
                        " · +constraint"
                    } else {
                        ""
                    }
                );
                summary.imported += 1;
            }
            continue;
        }

        let item = OnboardingPattern {
            session_id: &candidate.session_id,
            project_dir: &candidate.project_dir,
            goal: &candidate.goal,
            toolchain: candidate.toolchain.as_deref(),
            successful_constraints: candidate.constraint.as_deref(),
        };
        match store.backfill_onboarding_pattern(&item).await? {
            BackfillOutcome::Inserted(id) => {
                println!(
                    "  ✅ {short_id} · {toolchain_label} · \"{goal_preview}\" → pattern {}",
                    &id[..8.min(id.len())]
                );
                summary.imported += 1;
            }
            BackfillOutcome::AlreadyImported => summary.already_imported += 1,
            BackfillOutcome::EmptyFingerprint => summary.skipped_empty_fingerprint += 1,
        }
    }

    println!();
    println!(
        "🧭 {} {} · {} already imported · {} with no human turn \
         · {} with an empty keyword fingerprint",
        if dry_run { "would import" } else { "imported" },
        summary.imported,
        summary.already_imported,
        summary.skipped_no_goal,
        summary.skipped_empty_fingerprint,
    );
    Ok(summary)
}

/// Truncate a goal string to `cap` characters for a one-line preview.
fn preview(s: &str, cap: usize) -> String {
    let first_line = s.lines().next().unwrap_or("").trim();
    if first_line.chars().count() <= cap {
        return first_line.to_string();
    }
    first_line.chars().take(cap).collect::<String>() + "…"
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopi_agent::transcript_import::HistoricalSession;
    use lopi_memory::MemoryStore;

    #[test]
    fn derive_candidate_returns_none_without_a_human_turn() {
        let session = HistoricalSession {
            session_id: "s".into(),
            ..Default::default()
        };
        assert!(derive_candidate(Path::new("/tmp/s.jsonl"), &session).is_none());
    }

    #[test]
    fn derive_candidate_falls_back_to_the_project_folder_label_without_a_cwd() {
        let session = HistoricalSession {
            session_id: "s".into(),
            human_turns: vec!["do the thing".into()],
            ..Default::default()
        };
        let path = Path::new("/home/user/.claude/projects/-home-user-lopi/s.jsonl");
        let candidate = derive_candidate(path, &session).unwrap();
        assert_eq!(candidate.project_dir, "-home-user-lopi");
        assert!(candidate.toolchain.is_none());
    }

    #[test]
    fn preview_truncates_and_takes_only_the_first_line() {
        assert_eq!(preview("short goal", 60), "short goal");
        assert_eq!(preview("first line\nsecond line", 60), "first line");
        let long = "x".repeat(80);
        assert_eq!(preview(&long, 10), "xxxxxxxxxx…");
    }

    #[test]
    fn default_claude_dir_resolves_under_home() {
        std::env::set_var("HOME", "/tmp/lopi-onboarding-import-test-home");
        assert_eq!(
            default_claude_dir(),
            PathBuf::from("/tmp/lopi-onboarding-import-test-home/.claude")
        );
    }

    fn write_transcript(claude_dir: &Path, project_folder: &str, session_id: &str, line: &str) {
        let dir = claude_dir.join("projects").join(project_folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{session_id}.jsonl")), line).unwrap();
    }

    fn human_turn_line(session_id: &str, text: &str) -> String {
        format!(
            r#"{{"type":"user","message":{{"role":"user","content":"{text}"}},"sessionId":"{session_id}"}}"#
        )
    }

    #[tokio::test]
    async fn run_import_reports_zero_and_never_opens_a_store_when_nothing_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join("claude-empty"); // never created
        let db_path = dir.path().join("lopi.db");

        let summary = run_import(false, Some(claude_dir), db_path.clone())
            .await
            .unwrap();
        assert_eq!(summary, ImportSummary::default());
        assert!(!db_path.exists(), "must not open/create the store");
    }

    #[tokio::test]
    async fn run_import_skips_a_session_with_no_human_turn() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join("claude");
        write_transcript(
            &claude_dir,
            "-home-user-proj",
            "session-no-human",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#,
        );
        let db_path = dir.path().join("lopi.db");

        let summary = run_import(false, Some(claude_dir), db_path).await.unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                skipped_no_goal: 1,
                ..Default::default()
            }
        );
    }

    #[tokio::test]
    async fn run_import_dry_run_reports_would_import_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join("claude");
        write_transcript(
            &claude_dir,
            "-home-user-proj",
            "session-dry",
            &human_turn_line("session-dry", "migrate the database schema"),
        );
        let db_path = dir.path().join("lopi.db");

        let summary = run_import(true, Some(claude_dir), db_path.clone())
            .await
            .unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                imported: 1,
                ..Default::default()
            }
        );

        let store = MemoryStore::open(&db_path).await.unwrap();
        assert!(store.load_patterns(10).await.unwrap().is_empty());
        assert!(!store
            .onboarding_session_imported("session-dry")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn run_import_real_run_inserts_and_a_second_run_reports_already_imported() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join("claude");
        write_transcript(
            &claude_dir,
            "-home-user-proj",
            "session-real",
            &human_turn_line("session-real", "migrate the database schema"),
        );
        let db_path = dir.path().join("lopi.db");

        let first = run_import(false, Some(claude_dir.clone()), db_path.clone())
            .await
            .unwrap();
        assert_eq!(
            first,
            ImportSummary {
                imported: 1,
                ..Default::default()
            }
        );

        let store = MemoryStore::open(&db_path).await.unwrap();
        assert_eq!(store.load_patterns(10).await.unwrap().len(), 1);

        let second = run_import(false, Some(claude_dir), db_path.clone())
            .await
            .unwrap();
        assert_eq!(
            second,
            ImportSummary {
                already_imported: 1,
                ..Default::default()
            }
        );
        assert_eq!(
            store.load_patterns(10).await.unwrap().len(),
            1,
            "re-run must not duplicate the pattern row"
        );
    }

    #[tokio::test]
    async fn run_import_dry_run_reports_already_imported_for_a_session_from_a_prior_real_run() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join("claude");
        write_transcript(
            &claude_dir,
            "-home-user-proj",
            "session-dry-after-real",
            &human_turn_line("session-dry-after-real", "migrate the database schema"),
        );
        let db_path = dir.path().join("lopi.db");

        let real = run_import(false, Some(claude_dir.clone()), db_path.clone())
            .await
            .unwrap();
        assert_eq!(
            real,
            ImportSummary {
                imported: 1,
                ..Default::default()
            }
        );

        let dry = run_import(true, Some(claude_dir), db_path).await.unwrap();
        assert_eq!(
            dry,
            ImportSummary {
                already_imported: 1,
                ..Default::default()
            }
        );
    }

    #[tokio::test]
    async fn run_import_real_run_reports_empty_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join("claude");
        // A human turn whose every word is <=3 chars — keyword_fingerprint()
        // filters all of them out, so backfill_onboarding_pattern returns
        // EmptyFingerprint rather than inserting anything.
        write_transcript(
            &claude_dir,
            "-home-user-proj",
            "session-empty-fp",
            &human_turn_line("session-empty-fp", "fix a bug now ok"),
        );
        let db_path = dir.path().join("lopi.db");

        let summary = run_import(false, Some(claude_dir), db_path.clone())
            .await
            .unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                skipped_empty_fingerprint: 1,
                ..Default::default()
            }
        );
        let store = MemoryStore::open(&db_path).await.unwrap();
        assert!(store.load_patterns(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn run_wrapper_drives_a_real_import_through_to_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join("claude");
        write_transcript(
            &claude_dir,
            "-home-user-proj",
            "session-via-run",
            &human_turn_line("session-via-run", "migrate the database schema"),
        );
        let db_path = dir.path().join("lopi.db");

        run(false, Some(claude_dir), db_path.clone()).await.unwrap();

        let store = MemoryStore::open(&db_path).await.unwrap();
        assert_eq!(store.load_patterns(10).await.unwrap().len(), 1);
        assert!(store
            .onboarding_session_imported("session-via-run")
            .await
            .unwrap());
    }
}
