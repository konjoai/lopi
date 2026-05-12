//! Pattern mining, retrieval, annotation, and weight calibration.
//!
//! Separated from `store/mod.rs` to stay within the 500-line budget.

use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::ScoreWeights;
use std::collections::HashSet;

use super::MemoryStore;

// ── Types ────────────────────────────────────────────────────────────────────

/// A pattern row retrieved from the `patterns` table.
#[derive(Debug, sqlx::FromRow)]
pub struct PatternRow {
    pub id: String,
    pub goal_keywords: String,
    pub successful_constraints: Option<String>,
    pub avg_attempts: Option<f64>,
    pub success_rate: Option<f64>,
    pub last_seen: String,
    /// 1 when derived from a failure post-mortem; 0 when mined from task stats.
    #[sqlx(default)]
    pub derived_from_postmortem: i64,
    /// User validation: `'approved'`, `'rejected'`, or `None` (unannotated).
    #[sqlx(default)]
    pub user_annotation: Option<String>,
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Jaccard similarity between two token sets (goal fingerprints).
/// Returns a value in [0.0, 1.0] — 1.0 means identical token sets.
pub fn jaccard_similarity(a: &str, b: &str) -> f32 {
    let tokens_a: HashSet<&str> = a.split_whitespace().collect();
    let tokens_b: HashSet<&str> = b.split_whitespace().collect();
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let intersection = tokens_a.intersection(&tokens_b).count() as f64;
    #[allow(clippy::cast_precision_loss)]
    let union = tokens_a.union(&tokens_b).count() as f64;
    if union == 0.0 {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    { (intersection / union) as f32 }
}

/// Build the keyword fingerprint for a goal string.
/// Sorted, deduped tokens longer than 3 characters, lowercased.
pub fn keyword_fingerprint(goal: &str) -> String {
    let mut words: Vec<String> = goal
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() > 3)
        .map(str::to_lowercase)
        .collect();
    words.sort_unstable();
    words.dedup();
    words.join(" ")
}

// ── MemoryStore impl ──────────────────────────────────────────────────────────

impl MemoryStore {
    /// Find patterns with Jaccard similarity ≥ 0.3 to the goal (max 5).
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn find_similar_patterns(&self, goal: &str) -> Result<Vec<PatternRow>> {
        let query_fp = keyword_fingerprint(goal);
        if query_fp.is_empty() {
            return Ok(vec![]);
        }
        let all: Vec<PatternRow> = sqlx::query_as::<_, PatternRow>(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, \
             last_seen, derived_from_postmortem FROM patterns",
        )
        .fetch_all(&self.read_pool)
        .await?;

        let mut scored: Vec<(f32, PatternRow)> = all
            .into_iter()
            .filter_map(|row| {
                let sim = jaccard_similarity(&query_fp, &row.goal_keywords);
                (sim > 0.3).then_some((sim, row))
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(5).map(|(_, r)| r).collect())
    }

    /// Load all patterns ordered by success rate descending.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_patterns(&self, limit: i64) -> Result<Vec<PatternRow>> {
        sqlx::query_as::<_, PatternRow>(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, \
             last_seen, derived_from_postmortem \
             FROM patterns ORDER BY COALESCE(success_rate, 0) DESC, last_seen DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await
        .context("load_patterns query failed")
    }

    /// Fetch a single pattern by id prefix (for `lopi learn show`).
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn find_pattern_by_id_prefix(&self, prefix: &str) -> Result<Option<PatternRow>> {
        sqlx::query_as::<_, PatternRow>(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, \
             last_seen, derived_from_postmortem \
             FROM patterns WHERE id LIKE ?1 LIMIT 1",
        )
        .bind(format!("{prefix}%"))
        .fetch_optional(&self.read_pool)
        .await
        .context("find_pattern_by_id_prefix query failed")
    }

    /// Insert a post-mortem-derived pattern constraint.
    ///
    /// # Errors
    /// Returns `Err` if the database insert fails.
    pub async fn insert_postmortem_pattern(
        &self,
        goal_keywords: &str,
        constraint: &str,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO patterns \
             (id, goal_keywords, successful_constraints, avg_attempts, success_rate, \
              last_seen, derived_from_postmortem) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
        )
        .bind(&id)
        .bind(goal_keywords)
        .bind(constraint)
        .bind(0.0_f64)
        .bind(0.0_f64)
        .bind(now)
        .execute(&self.write_pool)
        .await?;
        Ok(id)
    }

    /// Mine a completed task's attempts into the patterns table.
    ///
    /// # Errors
    /// Returns `Err` if any database query or update fails.
    pub async fn mine_patterns(&self, task_id: &lopi_core::TaskId, goal: &str) -> Result<()> {
        let fingerprint = keyword_fingerprint(goal);
        if fingerprint.is_empty() {
            return Ok(());
        }
        let stats: Option<(f64, i64)> = sqlx::query_as(
            "SELECT AVG(score_test_pass_rate), COUNT(*) FROM attempts WHERE task_id = ?1",
        )
        .bind(task_id.0.to_string())
        .fetch_optional(&self.read_pool)
        .await?;

        let (avg_pass, attempt_count) = stats.unwrap_or((0.0, 0));
        let success_rate = avg_pass.clamp(0.0, 1.0);
        let existing: Option<(String, Option<f64>, Option<f64>)> = sqlx::query_as(
            "SELECT id, avg_attempts, success_rate FROM patterns WHERE goal_keywords = ?1",
        )
        .bind(&fingerprint)
        .fetch_optional(&self.read_pool)
        .await?;

        #[allow(clippy::cast_precision_loss)]
        let attempt_f = attempt_count as f64;
        let now = Utc::now().to_rfc3339();

        if let Some((existing_id, prev_avg, prev_sr)) = existing {
            let new_avg = f64::midpoint(prev_avg.unwrap_or(0.0), attempt_f).max(1.0);
            let new_sr = f64::midpoint(prev_sr.unwrap_or(0.0), success_rate).clamp(0.0, 1.0);
            sqlx::query(
                "UPDATE patterns SET avg_attempts=?1, success_rate=?2, last_seen=?3 WHERE id=?4",
            )
            .bind(new_avg).bind(new_sr).bind(&now).bind(existing_id)
            .execute(&self.write_pool).await?;
        } else {
            sqlx::query(
                "INSERT INTO patterns (id, goal_keywords, avg_attempts, success_rate, last_seen) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&fingerprint).bind(attempt_f).bind(success_rate).bind(&now)
            .execute(&self.write_pool).await?;
        }
        Ok(())
    }

    /// Update user annotation for a pattern ('approved', 'rejected', or None).
    ///
    /// # Errors
    /// Returns `Err` if the database update fails.
    pub async fn annotate_pattern(&self, pattern_id: &str, annotation: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE patterns SET user_annotation = ?1 WHERE id = ?2")
            .bind(annotation)
            .bind(pattern_id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Load all annotated patterns (approved or rejected) for trust calibration.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_annotated_patterns(&self) -> Result<Vec<PatternRow>> {
        sqlx::query_as(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, \
             last_seen, derived_from_postmortem, user_annotation \
             FROM patterns WHERE user_annotation IS NOT NULL ORDER BY last_seen DESC LIMIT 100",
        )
        .fetch_all(&self.read_pool)
        .await
        .context("load_annotated_patterns query failed")
    }

    /// Compute score weight adjustments from approved vs rejected pattern signals.
    ///
    /// When approved patterns required fewer attempts than rejected ones, the
    /// lint penalty is tightened (stricter quality = more approvals).
    /// When the opposite is true, the penalty is loosened.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn compute_weight_adjustments(&self) -> Result<ScoreWeights> {
        let annotated = self.load_annotated_patterns().await?;
        let approved: Vec<_> = annotated.iter()
            .filter(|p| p.user_annotation.as_deref() == Some("approved"))
            .collect();
        let rejected: Vec<_> = annotated.iter()
            .filter(|p| p.user_annotation.as_deref() == Some("rejected"))
            .collect();

        if approved.is_empty() && rejected.is_empty() {
            return Ok(ScoreWeights::default());
        }

        let avg = |patterns: &[&PatternRow]| -> f64 {
            if patterns.is_empty() { return 0.0; }
            patterns.iter().filter_map(|p| p.avg_attempts).sum::<f64>() / patterns.len() as f64
        };
        let signal = (avg(&rejected) - avg(&approved)).clamp(-2.0, 2.0);
        #[allow(clippy::cast_possible_truncation)]
        let delta = (signal * 0.005) as f32;
        let base = ScoreWeights::default();
        Ok(ScoreWeights {
            lint_penalty_per_error: (base.lint_penalty_per_error - delta).clamp(0.01, 0.20),
            lint_penalty_cap: base.lint_penalty_cap,
            diff_penalty_per_kloc: (base.diff_penalty_per_kloc - delta).clamp(0.01, 0.30),
            diff_penalty_cap: base.diff_penalty_cap,
        })
    }
}
