//! Direct-insert bypasses for deterministic `lopi demo` fixture content.
//!
//! `lopi demo`'s fixture generator needs demo content to be byte-identical
//! for a given seed — no merge/average with an existing row (unlike
//! [`super::pattern_upsert::upsert_pattern_row`]) and no quality gate
//! (unlike [`super::lessons::MemoryStore::save_lesson`]'s
//! `LESSON_QUALITY_GATE`). Only the demo generator should call these; every
//! other write path keeps going through the existing mine/save functions.

use anyhow::Result;
use chrono::{DateTime, Utc};

use super::MemoryStore;

/// Exact field set for a direct `patterns` insert — mirrors `PatternRow`
/// minus nothing (every field is caller-controlled), used by `lopi demo`'s
/// generator so pattern content is byte-identical for a given seed. Unlike
/// `upsert_pattern_row` (this crate's live-run write path), this never
/// merges with an existing row — always a fresh INSERT.
///
/// `source` is not a field here: the demo seed always writes `'lopi_run'`
/// so demo patterns read as if they were mined from live runs.
/// `user_annotation` is likewise not a field — always inserted as `NULL`.
pub struct DemoPatternSeed {
    /// Primary key for the new row.
    pub id: String,
    /// Space-separated keyword fingerprint (see `keyword_fingerprint`).
    pub goal_keywords: String,
    /// JSON array of constraint strings, or `None`.
    pub successful_constraints: Option<String>,
    /// Rolling average attempt count to write verbatim.
    pub avg_attempts: f64,
    /// Success rate (0.0-1.0) to write verbatim.
    pub success_rate: f64,
    /// Timestamp to write as `last_seen`.
    pub last_seen: DateTime<Utc>,
    /// Whether this pattern reads as post-mortem-derived.
    pub derived_from_postmortem: bool,
    /// Coarse per-project ecosystem label, or `None`.
    pub toolchain: Option<String>,
    /// Occurrence count to write verbatim.
    pub occurrence_count: i64,
}

/// Exact field set for a direct `lessons` insert, bypassing `save_lesson`'s
/// `LESSON_QUALITY_GATE` — demo lessons must appear regardless of a
/// fabricated score.
pub struct DemoLessonSeed {
    /// Primary key for the new row.
    pub id: String,
    /// Repository path the lesson belongs to.
    pub repo_path: String,
    /// Lesson category: `"strategy"`, `"recovery"`, or `"optimization"`.
    pub category: String,
    /// Human-readable lesson content for prompt injection.
    pub content: String,
    /// Optional owning task id.
    pub task_id: Option<String>,
    /// Timestamp to write as `created_at`.
    pub created_at: DateTime<Utc>,
}

impl MemoryStore {
    /// Insert a pattern row with every field exactly as given — no
    /// fingerprint merge, no averaging. Only `lopi demo`'s generator should
    /// call this; every other write path should keep going through
    /// `mine_patterns`/`upsert_pattern_row`.
    ///
    /// # Errors
    /// Returns `Err` if the database insert fails.
    pub async fn seed_demo_pattern(&self, p: &DemoPatternSeed) -> Result<()> {
        sqlx::query(
            "INSERT INTO patterns \
             (id, goal_keywords, successful_constraints, avg_attempts, success_rate, \
              last_seen, derived_from_postmortem, user_annotation, toolchain, source, \
              occurrence_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, 'lopi_run', ?9)",
        )
        .bind(&p.id)
        .bind(&p.goal_keywords)
        .bind(&p.successful_constraints)
        .bind(p.avg_attempts)
        .bind(p.success_rate)
        .bind(p.last_seen.to_rfc3339())
        .bind(i64::from(p.derived_from_postmortem))
        .bind(&p.toolchain)
        .bind(p.occurrence_count)
        .execute(&self.write_pool)
        .await?;
        let mut conn = self.write_pool.acquire().await?;
        Self::index_pattern_keywords(&mut conn, &p.id, &p.goal_keywords).await?;
        Ok(())
    }

    /// Insert a lesson row unconditionally, bypassing `LESSON_QUALITY_GATE`.
    /// Only `lopi demo`'s generator should call this. Writes `score = 1.0`
    /// (the `lessons` table's `score` column is `NOT NULL`) — the exact
    /// value is irrelevant here since nothing gates on it once the row
    /// exists.
    ///
    /// # Errors
    /// Returns `Err` if the database insert fails.
    pub async fn seed_demo_lesson(&self, l: &DemoLessonSeed) -> Result<()> {
        sqlx::query(
            "INSERT INTO lessons (id, repo_path, category, content, task_id, score, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 1.0, ?6) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&l.id)
        .bind(&l.repo_path)
        .bind(&l.category)
        .bind(&l.content)
        .bind(&l.task_id)
        .bind(l.created_at.to_rfc3339())
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    fn pattern_seed() -> DemoPatternSeed {
        DemoPatternSeed {
            id: "demo-pattern-1".into(),
            goal_keywords: "fix flaky test".into(),
            successful_constraints: Some(r#"["run tests twice"]"#.into()),
            avg_attempts: 2.5,
            success_rate: 0.9,
            last_seen: Utc::now(),
            derived_from_postmortem: true,
            toolchain: Some("rust".into()),
            occurrence_count: 7,
        }
    }

    #[tokio::test]
    async fn seed_demo_pattern_round_trips_every_field() {
        let s = store().await;
        let seed = pattern_seed();
        s.seed_demo_pattern(&seed).await.unwrap();

        let loaded = s.find_pattern_by_id_prefix("demo-pattern-1").await.unwrap();
        let row = loaded.expect("seeded pattern should be found");
        assert_eq!(row.id, seed.id);
        assert_eq!(row.goal_keywords, seed.goal_keywords);
        assert_eq!(row.successful_constraints, seed.successful_constraints);
        assert_eq!(row.avg_attempts, Some(seed.avg_attempts));
        assert_eq!(row.success_rate, Some(seed.success_rate));
        assert_eq!(row.derived_from_postmortem, 1);
        assert_eq!(row.toolchain, seed.toolchain);
        assert_eq!(row.source, "lopi_run");
        assert_eq!(row.occurrence_count, seed.occurrence_count);
    }

    #[tokio::test]
    async fn seed_demo_pattern_is_a_direct_insert_not_a_merge() {
        // Two seeds with the same goal_keywords must produce two distinct
        // rows (unlike upsert_pattern_row, which would merge them).
        let s = store().await;
        let mut first = pattern_seed();
        first.id = "demo-pattern-a".into();
        let mut second = pattern_seed();
        second.id = "demo-pattern-b".into();
        s.seed_demo_pattern(&first).await.unwrap();
        s.seed_demo_pattern(&second).await.unwrap();

        let all = s.load_patterns(10).await.unwrap();
        assert_eq!(all.len(), 2, "direct insert never merges by fingerprint");
    }

    #[tokio::test]
    async fn seed_demo_lesson_bypasses_quality_gate_unlike_save_lesson() {
        let s = store().await;
        let repo_path = "/demo/repos/aurora-api";

        // save_lesson with a score below LESSON_QUALITY_GATE silently
        // skips the write.
        s.save_lesson(repo_path, "strategy", "low quality", None, 0.1)
            .await
            .unwrap();
        assert!(s.load_lessons(repo_path, 10).await.unwrap().is_empty());

        // seed_demo_lesson has no score gate at all — it always appears.
        s.seed_demo_lesson(&DemoLessonSeed {
            id: "demo-lesson-1".into(),
            repo_path: repo_path.into(),
            category: "strategy".into(),
            content: "seeded lesson".into(),
            task_id: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
        let lessons = s.load_lessons(repo_path, 10).await.unwrap();
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].content, "seeded lesson");
    }
}
