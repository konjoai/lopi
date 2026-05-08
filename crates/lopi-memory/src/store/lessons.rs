use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::MemoryStore;

/// A retrieved lesson from the `lessons` table.
#[derive(Debug, sqlx::FromRow)]
pub struct LessonRow {
    /// UUID of the lesson.
    pub id: String,
    /// Repository path the lesson belongs to.
    pub repo_path: String,
    /// Lesson category: `strategy`, `recovery`, or `optimization`.
    pub category: String,
    /// Human-readable lesson content for prompt injection.
    pub content: String,
    /// Wall-clock time when the lesson was written (`RFC 3339`).
    pub created_at: String,
}

impl MemoryStore {
    /// Minimum score threshold for writing a lesson.
    ///
    /// Below this value the run is not informative enough to generalise from;
    /// persisting it would degrade future retrieval quality.
    pub const LESSON_QUALITY_GATE: f32 = 0.6;

    /// Persist a lesson from a completed run.
    ///
    /// `category` must be one of `"strategy"`, `"recovery"`, or `"optimization"`.
    /// Writes are silently skipped when `score < LESSON_QUALITY_GATE`.
    ///
    /// # Errors
    /// Returns `Err` if the database write fails.
    pub async fn save_lesson(
        &self,
        repo_path: &str,
        category: &str,
        content: &str,
        task_id: Option<&str>,
        score: f32,
    ) -> Result<()> {
        if score < Self::LESSON_QUALITY_GATE {
            tracing::debug!(score, "lesson below quality gate — skipping write");
            return Ok(());
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO lessons (id, repo_path, category, content, task_id, score, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&id)
        .bind(repo_path)
        .bind(category)
        .bind(content)
        .bind(task_id)
        .bind(f64::from(score))
        .bind(&now)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Load up to `limit` lessons for `repo_path`, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_lessons(&self, repo_path: &str, limit: i64) -> Result<Vec<LessonRow>> {
        let rows = sqlx::query_as::<_, LessonRow>(
            "SELECT id, repo_path, category, content, created_at \
             FROM lessons WHERE repo_path = ?1 \
             ORDER BY created_at DESC LIMIT ?2",
        )
        .bind(repo_path)
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Sum token usage and estimated cost for today (UTC) from `turn_metrics`.
    ///
    /// Returns `(total_tokens, total_cost_usd)`.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn daily_token_totals(&self) -> Result<(i64, f64)> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let row: (i64, f64) = sqlx::query_as(
            "SELECT COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens), 0), \
                    COALESCE(SUM(estimated_cost_usd), 0.0) \
             FROM turn_metrics WHERE timestamp >= ?1",
        )
        .bind(format!("{today}T00:00:00Z"))
        .fetch_one(&self.read_pool)
        .await?;
        Ok(row)
    }
}
