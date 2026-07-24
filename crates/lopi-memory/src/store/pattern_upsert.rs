//! Shared upsert-by-fingerprint core for `patterns` — split out of
//! `patterns.rs` to stay within the 400-line split guideline. Used by both
//! `patterns::mine_patterns` (live task stats) and
//! `onboarding_import::backfill_onboarding_pattern` (historical transcript
//! backfill) so the two write paths share one query set rather than a
//! second parallel one.

use anyhow::Result;

use super::MemoryStore;

/// Extra columns only the onboarding backfill path
/// ([`super::onboarding_import`]) sets; `mine_patterns`'s live-run insert
/// always passes `PatternExtra::default()`, leaving these columns at their
/// schema default (`toolchain` NULL, `source` `'lopi_run'`).
#[derive(Default)]
pub(super) struct PatternExtra<'a> {
    pub(super) toolchain: Option<&'a str>,
    pub(super) successful_constraints: Option<&'a str>,
    pub(super) source: Option<&'a str>,
}

impl MemoryStore {
    /// Look up an existing `patterns` row by `goal_keywords`, blend stats
    /// into it if found, or insert a fresh row (indexing its keywords
    /// either way).
    ///
    /// # Errors
    /// Returns `Err` if any database query or update fails.
    pub(super) async fn upsert_pattern_row(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        fingerprint: &str,
        avg_attempts: f64,
        success_rate: f64,
        now: &str,
        extra: &PatternExtra<'_>,
    ) -> Result<String> {
        let existing: Option<(String, Option<f64>, Option<f64>)> = sqlx::query_as(
            "SELECT id, avg_attempts, success_rate FROM patterns WHERE goal_keywords = ?1",
        )
        .bind(fingerprint)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some((existing_id, prev_avg, prev_sr)) = existing {
            let new_avg = f64::midpoint(prev_avg.unwrap_or(0.0), avg_attempts).max(1.0);
            let new_sr = f64::midpoint(prev_sr.unwrap_or(0.0), success_rate).clamp(0.0, 1.0);
            // `source` is deliberately never overwritten on an existing row:
            // a pattern already mined from a live task run stays 'lopi_run'
            // even if a later onboarding backfill blends historical evidence
            // into it — the row's provenance is "first observed", not "most
            // recently touched". `toolchain`/`successful_constraints` fill
            // in only when the row doesn't already have one (COALESCE), so a
            // live-mined row's genuine constraint is never clobbered by a
            // backfill's weaker, transcript-derived guess.
            sqlx::query(
                "UPDATE patterns SET avg_attempts=?1, success_rate=?2, last_seen=?3, \
                 toolchain=COALESCE(toolchain, ?4), \
                 successful_constraints=COALESCE(successful_constraints, ?5) \
                 WHERE id=?6",
            )
            .bind(new_avg)
            .bind(new_sr)
            .bind(now)
            .bind(extra.toolchain)
            .bind(extra.successful_constraints)
            .bind(&existing_id)
            .execute(&mut **tx)
            .await?;
            Ok(existing_id)
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO patterns \
                 (id, goal_keywords, avg_attempts, success_rate, last_seen, toolchain, \
                  successful_constraints, source) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, COALESCE(?8, 'lopi_run'))",
            )
            .bind(&id)
            .bind(fingerprint)
            .bind(avg_attempts)
            .bind(success_rate)
            .bind(now)
            .bind(extra.toolchain)
            .bind(extra.successful_constraints)
            .bind(extra.source)
            .execute(&mut **tx)
            .await?;
            Self::index_pattern_keywords(&mut *tx, &id, fingerprint).await?;
            Ok(id)
        }
    }
}
