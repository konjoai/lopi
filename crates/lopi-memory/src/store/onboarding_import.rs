//! Onboarding-Import-1 (Phase 3/5) — one-time backfill of `patterns` from
//! historical Claude Code session transcripts.
//!
//! Reuses [`super::patterns::keyword_fingerprint`] and
//! [`MemoryStore::upsert_pattern_row`] rather than a parallel query set —
//! the same upsert-by-fingerprint core `mine_patterns` uses for live task
//! runs. Session-level idempotency (Phase 5) is tracked separately via
//! `onboarding_imports`, keyed on the transcript's own `sessionId`, since a
//! goal-keyword fingerprint match against an existing pattern is not the
//! same guarantee as "this exact session was already imported" — two
//! different historical sessions can legitimately share a fingerprint.

use anyhow::Result;
use chrono::Utc;

use super::pattern_upsert::PatternExtra;
use super::patterns::keyword_fingerprint;
use super::MemoryStore;

/// One historical session ready to fold into `patterns` via
/// [`MemoryStore::backfill_onboarding_pattern`].
pub struct OnboardingPattern<'a> {
    /// The JSONL transcript's own `sessionId` — the idempotency key.
    pub session_id: &'a str,
    /// The project directory the session ran in (the JSONL's own `cwd`).
    pub project_dir: &'a str,
    /// A goal-like string derived from the session's first human turn.
    pub goal: &'a str,
    /// Toolchain label from walking `project_dir` for manifest files, if any.
    pub toolchain: Option<&'a str>,
    /// A short "what worked" string, populated only when the session's tail
    /// looks like a clean completion — see
    /// `lopi_agent::transcript_import::session_looks_successful`.
    pub successful_constraints: Option<&'a str>,
}

/// Outcome of a single [`MemoryStore::backfill_onboarding_pattern`] call.
#[derive(Debug, PartialEq, Eq)]
pub enum BackfillOutcome {
    /// A new or blended pattern row was written; carries the row's id.
    Inserted(String),
    /// `session_id` was already present in `onboarding_imports` — skipped.
    AlreadyImported,
    /// The goal string produced an empty keyword fingerprint — nothing to mine.
    EmptyFingerprint,
}

impl MemoryStore {
    /// Has this transcript session already been folded into `patterns`?
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn onboarding_session_imported(&self, session_id: &str) -> Result<bool> {
        let row: Option<String> =
            sqlx::query_scalar("SELECT session_id FROM onboarding_imports WHERE session_id = ?1")
                .bind(session_id)
                .fetch_optional(&self.read_pool)
                .await?;
        Ok(row.is_some())
    }

    /// Backfill one historical session into `patterns`, tagged
    /// `source = 'onboarding_import'`. Idempotent on `session_id`: a
    /// session already recorded in `onboarding_imports` is skipped, not
    /// re-blended.
    ///
    /// # Errors
    /// Returns `Err` if any database query or update fails.
    pub async fn backfill_onboarding_pattern(
        &self,
        item: &OnboardingPattern<'_>,
    ) -> Result<BackfillOutcome> {
        if self.onboarding_session_imported(item.session_id).await? {
            return Ok(BackfillOutcome::AlreadyImported);
        }
        let fingerprint = keyword_fingerprint(item.goal);
        if fingerprint.is_empty() {
            return Ok(BackfillOutcome::EmptyFingerprint);
        }
        let now = Utc::now().to_rfc3339();
        let extra = PatternExtra {
            toolchain: item.toolchain,
            successful_constraints: item.successful_constraints,
            source: Some("onboarding_import"),
        };
        // Single observed historical session per backfill call: one
        // "attempt" of evidence, and a binary success signal (1.0 when the
        // completion heuristic passed, 0.0 — i.e. no signal either way —
        // otherwise) rather than a real pass-rate average, since no
        // `attempts` rows exist for a transcript-derived pattern.
        let success_rate = f64::from(item.successful_constraints.is_some());

        let mut tx = self.write_pool.begin().await?;
        let pattern_id =
            Self::upsert_pattern_row(&mut tx, &fingerprint, 1.0, success_rate, &now, &extra)
                .await?;
        sqlx::query(
            "INSERT INTO onboarding_imports (session_id, project_dir, pattern_id, imported_at) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(item.session_id)
        .bind(item.project_dir)
        .bind(&pattern_id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(BackfillOutcome::Inserted(pattern_id))
    }
}

#[cfg(test)]
#[path = "onboarding_import_tests.rs"]
mod tests;
