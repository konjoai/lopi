//! A2 (reflection) — durable, rollback-safe learnings.
//!
//! A *learning* is distilled from a failed or rolled-back attempt: the
//! evaluator's critique (why it failed), a short summary of what was attempted,
//! and the reject outcome. Unlike [`lessons`](super::lessons) there is **no
//! score gate** — a rejected attempt's lesson is precisely the low-score case
//! that must survive a gain-gate rollback (you learned what does *not* work).
//!
//! Retrieval is **relevance-filtered** (Jaccard over the goal keyword
//! fingerprint), **recency-ordered**, **deduped** on the critique text, and
//! **caller-capped** — bounded + relevant is the whole point, since unbounded or
//! irrelevant injection is the failure mode the A2 §2 kill-test punishes.

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::patterns::{jaccard_similarity, keyword_fingerprint};
use super::MemoryStore;

/// A durable learning distilled from a failed/rolled-back attempt.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LearningRow {
    /// UUID of the learning.
    pub id: String,
    /// Repository path the learning belongs to.
    pub repo_path: String,
    /// `keyword_fingerprint(goal)` — the relevance key for retrieval.
    pub goal_keywords: String,
    /// Why the attempt failed (the evaluator's flattened gaps/fix-hints).
    pub critique: String,
    /// Short summary of what the rejected attempt tried.
    pub attempted: String,
    /// The reject reason/outcome (e.g. `eval_rejected`, `non_gaining`).
    pub outcome: String,
    /// Wall-clock time when the learning was written (`RFC 3339`).
    pub created_at: String,
}

impl MemoryStore {
    /// How many recency-ordered candidates relevance retrieval scans before
    /// filtering. Bounds the read; injection is capped far tighter by the caller.
    const LEARNING_SCAN_LIMIT: i64 = 50;

    /// Minimum goal-keyword Jaccard for a learning to count as relevant. Reuses
    /// the pattern-retrieval threshold so "similar goal" means the same thing
    /// everywhere.
    pub const LEARNING_RELEVANCE_GATE: f32 = 0.3;

    /// Persist a learning from a failed/rolled-back attempt.
    ///
    /// **No score gate** — the point of A2 is that a rejected attempt still
    /// yields its lesson. Idempotent on `(repo_path, critique)`: an identical
    /// critique for the same repo is not written twice, so repeated rejections of
    /// the same failure mode don't bloat the table.
    ///
    /// # Errors
    /// Returns `Err` if the database write fails.
    pub async fn save_learning(
        &self,
        repo_path: &str,
        goal: &str,
        critique: &str,
        attempted: &str,
        outcome: &str,
        task_id: Option<&str>,
    ) -> Result<()> {
        if critique.trim().is_empty() {
            tracing::warn!("learning capture skipped — empty critique");
            return Ok(());
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let fingerprint = keyword_fingerprint(goal);
        sqlx::query(
            "INSERT INTO learnings \
                 (id, repo_path, goal_keywords, critique, attempted, outcome, task_id, created_at) \
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8 \
             WHERE NOT EXISTS ( \
                 SELECT 1 FROM learnings WHERE repo_path = ?2 AND critique = ?4 \
             )",
        )
        .bind(&id)
        .bind(repo_path)
        .bind(&fingerprint)
        .bind(critique)
        .bind(attempted)
        .bind(outcome)
        .bind(task_id)
        .bind(&now)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Load up to `limit` learnings for `repo_path`, newest first (no relevance
    /// filter). Used by diagnostics and as the candidate pool for retrieval.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_learnings(&self, repo_path: &str, limit: i64) -> Result<Vec<LearningRow>> {
        let rows = sqlx::query_as::<_, LearningRow>(
            "SELECT id, repo_path, goal_keywords, critique, attempted, outcome, created_at \
             FROM learnings WHERE repo_path = ?1 \
             ORDER BY created_at DESC LIMIT ?2",
        )
        .bind(repo_path)
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Retrieve the `limit` most relevant learnings for a new task's `goal`.
    ///
    /// Relevance = goal-keyword Jaccard ≥ [`LEARNING_RELEVANCE_GATE`]; ties break
    /// by recency (candidates arrive newest-first). Deduped on critique text so
    /// the same lesson never enters context twice. `limit` is the hard injection
    /// cap — the caller must not inject more than it asks for here.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn find_relevant_learnings(
        &self,
        repo_path: &str,
        goal: &str,
        limit: usize,
    ) -> Result<Vec<LearningRow>> {
        if limit == 0 {
            return Ok(vec![]);
        }
        let candidates = self
            .load_learnings(repo_path, Self::LEARNING_SCAN_LIMIT)
            .await?;
        Ok(rank_relevant(
            candidates,
            &keyword_fingerprint(goal),
            Self::LEARNING_RELEVANCE_GATE,
            limit,
        ))
    }
}

/// Pure relevance ranking: keep learnings whose goal-keyword Jaccard clears
/// `gate`, dedup on critique, sort by relevance (recency-preserving for ties
/// since `candidates` arrive newest-first), and take `limit`.
///
/// Split out from the async query so the filtering/cap/dedup logic is unit-tested
/// without a database.
fn rank_relevant(
    candidates: Vec<LearningRow>,
    goal_fingerprint: &str,
    gate: f32,
    limit: usize,
) -> Vec<LearningRow> {
    if limit == 0 {
        return vec![];
    }
    let mut scored: Vec<(f32, LearningRow)> = candidates
        .into_iter()
        .map(|row| {
            let rel = jaccard_similarity(goal_fingerprint, &row.goal_keywords);
            (rel, row)
        })
        .filter(|(rel, _)| *rel >= gate)
        .collect();
    // Stable sort by descending relevance keeps the newest-first input order
    // within an equal-relevance group.
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<LearningRow> = Vec::new();
    for (_, row) in scored {
        if seen.iter().any(|c| c == &row.critique) {
            continue;
        }
        seen.push(row.critique.clone());
        out.push(row);
        if out.len() >= limit {
            break;
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn row(critique: &str, keywords: &str, created_at: &str) -> LearningRow {
        LearningRow {
            id: Uuid::new_v4().to_string(),
            repo_path: "/repo".into(),
            goal_keywords: keywords.into(),
            critique: critique.into(),
            attempted: String::new(),
            outcome: String::new(),
            created_at: created_at.into(),
        }
    }

    #[test]
    fn relevant_kept_irrelevant_dropped() {
        let fp = keyword_fingerprint("fix the authentication timeout bug");
        let cands = vec![
            row(
                "auth token expired early",
                &keyword_fingerprint("authentication timeout token"),
                "2026-01-02T00:00:00Z",
            ),
            row(
                "image rendering slow",
                &keyword_fingerprint("optimize image rendering pipeline"),
                "2026-01-01T00:00:00Z",
            ),
        ];
        let out = rank_relevant(cands, &fp, MemoryStore::LEARNING_RELEVANCE_GATE, 3);
        assert_eq!(out.len(), 1, "only the on-topic learning survives");
        assert_eq!(out[0].critique, "auth token expired early");
    }

    #[test]
    fn respects_the_injection_cap() {
        let fp = keyword_fingerprint("refactor database migration runner");
        let kw = keyword_fingerprint("refactor database migration runner");
        let cands: Vec<LearningRow> = (0..10)
            .map(|i| row(&format!("lesson {i}"), &kw, "2026-01-01T00:00:00Z"))
            .collect();
        let out = rank_relevant(cands, &fp, MemoryStore::LEARNING_RELEVANCE_GATE, 3);
        assert_eq!(out.len(), 3, "cap of 3 is honoured");
    }

    #[test]
    fn dedups_identical_critiques() {
        let fp = keyword_fingerprint("parse the config toml file");
        let kw = keyword_fingerprint("parse the config toml file");
        let cands = vec![
            row("same critique", &kw, "2026-01-02T00:00:00Z"),
            row("same critique", &kw, "2026-01-01T00:00:00Z"),
        ];
        let out = rank_relevant(cands, &fp, MemoryStore::LEARNING_RELEVANCE_GATE, 5);
        assert_eq!(out.len(), 1, "duplicate critiques collapse to one");
    }

    #[test]
    fn zero_cap_returns_nothing() {
        let fp = keyword_fingerprint("anything");
        let cands = vec![row(
            "x",
            &keyword_fingerprint("anything"),
            "2026-01-01T00:00:00Z",
        )];
        assert!(rank_relevant(cands, &fp, MemoryStore::LEARNING_RELEVANCE_GATE, 0).is_empty());
    }
}
