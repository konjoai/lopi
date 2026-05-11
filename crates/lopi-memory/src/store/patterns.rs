use super::{MemoryStore, PatternRow};
use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::ScoreWeights;

/// Jaccard similarity between two token sets derived from goal fingerprint strings.
/// Returns a value in [0.0, 1.0] — 1.0 means identical token sets.
pub fn jaccard_similarity(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
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
    let ratio = (intersection / union) as f32;
    ratio
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

impl MemoryStore {
    /// Jaccard similarity search over stored keyword fingerprints.
    ///
    /// Returns up to 5 patterns most similar to `goal` with Jaccard score > 0.3.
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
             last_seen, derived_from_postmortem, user_annotation FROM patterns",
        )
        .fetch_all(&self.read_pool)
        .await?;

        let mut scored: Vec<(f32, PatternRow)> = all
            .into_iter()
            .filter_map(|row| {
                let sim = jaccard_similarity(&query_fp, &row.goal_keywords);
                if sim > 0.3 {
                    Some((sim, row))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(5).map(|(_, r)| r).collect())
    }

    /// Load all patterns ordered by `success_rate` descending.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_patterns(&self, limit: i64) -> Result<Vec<PatternRow>> {
        let rows = sqlx::query_as::<_, PatternRow>(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, \
             last_seen, derived_from_postmortem, user_annotation \
             FROM patterns ORDER BY COALESCE(success_rate, 0) DESC, last_seen DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Fetch a single pattern by id prefix (for `lopi learn show`).
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn find_pattern_by_id_prefix(&self, prefix: &str) -> Result<Option<PatternRow>> {
        let pattern = format!("{prefix}%");
        let row = sqlx::query_as::<_, PatternRow>(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, \
             last_seen, derived_from_postmortem, user_annotation \
             FROM patterns WHERE id LIKE ?1 LIMIT 1",
        )
        .bind(pattern)
        .fetch_optional(&self.read_pool)
        .await?;
        Ok(row)
    }

    /// Persist a new pattern derived from a failed-run post-mortem.
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
            "SELECT AVG(score_test_pass_rate), COUNT(*) \
             FROM attempts WHERE task_id = ?1",
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
        let attempt_count_f = attempt_count as f64;
        let now = Utc::now().to_rfc3339();

        if let Some((existing_id, prev_avg, prev_sr)) = existing {
            let new_avg = f64::midpoint(prev_avg.unwrap_or(0.0), attempt_count_f).max(1.0);
            let new_sr = f64::midpoint(prev_sr.unwrap_or(0.0), success_rate).clamp(0.0, 1.0);
            sqlx::query(
                "UPDATE patterns \
                 SET avg_attempts = ?1, success_rate = ?2, last_seen = ?3 WHERE id = ?4",
            )
            .bind(new_avg)
            .bind(new_sr)
            .bind(&now)
            .bind(existing_id)
            .execute(&self.write_pool)
            .await?;
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO patterns (id, goal_keywords, avg_attempts, success_rate, last_seen) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(id)
            .bind(&fingerprint)
            .bind(attempt_count_f)
            .bind(success_rate)
            .bind(&now)
            .execute(&self.write_pool)
            .await?;
        }
        Ok(())
    }

    /// Update user annotation for a pattern. Values: `'approved'`, `'rejected'`, or `None`.
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

    /// Load all patterns that have a user annotation (approved or rejected).
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

    /// Derive evolved `ScoreWeights` from the annotation signal.
    ///
    /// Approved patterns → lower penalties; rejected patterns → higher penalties.
    /// Falls back to `ScoreWeights::default()` when no annotated patterns exist.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn compute_weight_adjustments(&self) -> Result<ScoreWeights> {
        let annotated = self.load_annotated_patterns().await?;
        let approved: Vec<_> = annotated
            .iter()
            .filter(|p| p.user_annotation.as_deref() == Some("approved"))
            .collect();
        let rejected: Vec<_> = annotated
            .iter()
            .filter(|p| p.user_annotation.as_deref() == Some("rejected"))
            .collect();

        if approved.is_empty() && rejected.is_empty() {
            return Ok(ScoreWeights::default());
        }

        let approved_avg = if approved.is_empty() {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let n = approved.len() as f64;
            approved.iter().filter_map(|p| p.avg_attempts).sum::<f64>() / n
        };
        let rejected_avg = if rejected.is_empty() {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let n = rejected.len() as f64;
            rejected.iter().filter_map(|p| p.avg_attempts).sum::<f64>() / n
        };

        let signal = (rejected_avg - approved_avg).clamp(-2.0, 2.0);
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

    /// Count post-mortem-derived patterns created in the last `since_hours` hours.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn recent_postmortem_count(&self, since_hours: i64) -> Result<i64> {
        let cutoff = (Utc::now() - chrono::Duration::hours(since_hours)).to_rfc3339();
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM patterns \
             WHERE derived_from_postmortem = 1 AND last_seen >= ?1",
        )
        .bind(&cutoff)
        .fetch_one(&self.read_pool)
        .await
        .context("recent_postmortem_count query failed")?;
        Ok(row.0)
    }
}
