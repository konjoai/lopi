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

/// Run the onboarding import. `dry_run` opens the store read-only to report
/// accurate status but performs no writes.
///
/// # Errors
/// Returns `Err` if the store can't be opened.
pub async fn run(dry_run: bool, claude_dir: Option<PathBuf>, db_path: PathBuf) -> Result<()> {
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
        return Ok(());
    }

    let store = MemoryStore::open(&db_path).await?;
    let mut imported = 0u32;
    let mut already_imported = 0u32;
    let mut skipped_no_goal = 0u32;
    let mut skipped_empty_fingerprint = 0u32;

    for path in &transcripts {
        let session = match parse_session_file(path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("skipping unreadable transcript {}: {e:#}", path.display());
                continue;
            }
        };
        let Some(candidate) = derive_candidate(path, &session) else {
            skipped_no_goal += 1;
            continue;
        };

        if store
            .onboarding_session_imported(&candidate.session_id)
            .await?
        {
            already_imported += 1;
            continue;
        }

        let short_id = &candidate.session_id[..8.min(candidate.session_id.len())];
        let toolchain_label = candidate.toolchain.as_deref().unwrap_or("unknown");
        let goal_preview = preview(&candidate.goal, 60);

        if dry_run {
            println!(
                "  [dry-run] would import {short_id} · {toolchain_label} · \"{goal_preview}\"{}",
                if candidate.constraint.is_some() {
                    " · +constraint"
                } else {
                    ""
                }
            );
            imported += 1;
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
                imported += 1;
            }
            BackfillOutcome::AlreadyImported => already_imported += 1,
            BackfillOutcome::EmptyFingerprint => skipped_empty_fingerprint += 1,
        }
    }

    println!();
    println!(
        "🧭 {} {} · {already_imported} already imported · {skipped_no_goal} with no human turn \
         · {skipped_empty_fingerprint} with an empty keyword fingerprint",
        if dry_run { "would import" } else { "imported" },
        imported
    );
    Ok(())
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
}
