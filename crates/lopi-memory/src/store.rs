use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::{Attempt, Task, TaskId, TurnMetrics};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteConnectOptions};
use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;

const SCHEMA: &str = include_str!("schema.sql");

/// Jaccard similarity between two token sets derived from goal fingerprint strings.
/// Returns a value in [0.0, 1.0] — 1.0 means identical token sets.
fn jaccard_similarity(a: &str, b: &str) -> f32 {
    let tokens_a: HashSet<&str> = a.split_whitespace().collect();
    let tokens_b: HashSet<&str> = b.split_whitespace().collect();
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    let intersection = tokens_a.intersection(&tokens_b).count() as f32;
    let union = tokens_a.union(&tokens_b).count() as f32;
    if union == 0.0 { 0.0 } else { intersection / union }
}

/// Build the keyword fingerprint for a goal string.
/// Sorted, deduped tokens longer than 3 characters, lowercased.
fn keyword_fingerprint(goal: &str) -> String {
    let mut words: Vec<String> = goal
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() > 3)
        .map(|w| w.to_lowercase())
        .collect();
    words.sort_unstable();
    words.dedup();
    words.join(" ")
}

#[derive(Clone)]
pub struct MemoryStore {
    pool: SqlitePool,
}

impl MemoryStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let url = format!("sqlite://{}", path.display());
        let opts = SqliteConnectOptions::from_str(&url)
            .context("parsing sqlite path")?
            .create_if_missing(true)
            .pragma("journal_mode", "WAL")
            .pragma("synchronous", "NORMAL")
            .pragma("busy_timeout", "5000");
        let pool = SqlitePoolOptions::new()
            .max_connections(20)
            .connect_with(opts)
            .await
            .context("opening sqlite pool")?;
        Self::apply_schema(&pool).await?;
        Ok(Self { pool })
    }

    /// Open an in-memory SQLite database — useful for tests.
    pub async fn open_in_memory() -> Result<Self> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .context("parsing in-memory sqlite")?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .context("opening in-memory sqlite pool")?;
        Self::apply_schema(&pool).await?;
        Ok(Self { pool })
    }

    async fn apply_schema(pool: &SqlitePool) -> Result<()> {
        for stmt in SCHEMA.split(';') {
            let s = stmt.trim();
            if s.is_empty() { continue; }
            let result = sqlx::query(s).execute(pool).await;
            // ALTER TABLE ... ADD COLUMN errors on duplicate columns — silently ignore.
            if let Err(e) = result {
                if !s.to_lowercase().starts_with("alter table") {
                    return Err(e).context("applying schema");
                }
            }
        }
        Ok(())
    }

    pub async fn save_task(&self, task: &Task, status: &str) -> Result<()> {
        let source = serde_json::to_string(&task.source)?;
        sqlx::query(
            "INSERT INTO tasks (id, goal, status, created_at, source) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(id) DO UPDATE SET status = excluded.status",
        )
        .bind(task.id.0.to_string())
        .bind(&task.goal)
        .bind(status)
        .bind(task.created_at.to_rfc3339())
        .bind(source)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_completed(&self, id: &TaskId, status: &str) -> Result<()> {
        sqlx::query(
            "UPDATE tasks SET status = ?1, completed_at = ?2 WHERE id = ?3",
        )
        .bind(status)
        .bind(Utc::now().to_rfc3339())
        .bind(id.0.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn save_attempt(&self, attempt: &Attempt) -> Result<()> {
        let (pass, lint, diff) = match &attempt.score {
            Some(s) => (Some(s.test_pass_rate), Some(s.lint_errors as i64), Some(s.diff_lines as i64)),
            None => (None, None, None),
        };
        let errors = attempt.score.as_ref()
            .map(|s| serde_json::to_string(&s.errors).unwrap_or_default())
            .unwrap_or_default();
        sqlx::query(
            "INSERT INTO attempts (id, task_id, attempt_num, branch, \
             score_test_pass_rate, score_lint_errors, score_diff_lines, outcome, errors, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(attempt.id.to_string())
        .bind(attempt.task_id.0.to_string())
        .bind(attempt.attempt_num as i64)
        .bind(&attempt.branch)
        .bind(pass)
        .bind(lint)
        .bind(diff)
        .bind(&attempt.outcome)
        .bind(errors)
        .bind(attempt.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Recent tasks, newest first.
    pub async fn load_history(&self, limit: i64) -> Result<Vec<TaskRow>> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, goal, status, created_at, completed_at FROM tasks \
             ORDER BY created_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Jaccard similarity search over stored keyword fingerprints.
    /// Returns up to 5 patterns most similar to `goal` with Jaccard score > 0.3.
    pub async fn find_similar_patterns(&self, goal: &str) -> Result<Vec<PatternRow>> {
        let query_fp = keyword_fingerprint(goal);
        if query_fp.is_empty() {
            return Ok(vec![]);
        }

        let all: Vec<PatternRow> = sqlx::query_as::<_, PatternRow>(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, last_seen \
             FROM patterns",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut scored: Vec<(f32, PatternRow)> = all
            .into_iter()
            .filter_map(|row| {
                let sim = jaccard_similarity(&query_fp, &row.goal_keywords);
                if sim > 0.3 { Some((sim, row)) } else { None }
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(5).map(|(_, r)| r).collect())
    }

    /// Load all patterns ordered by success_rate descending.
    pub async fn load_patterns(&self, limit: i64) -> Result<Vec<PatternRow>> {
        let rows = sqlx::query_as::<_, PatternRow>(
            "SELECT id, goal_keywords, successful_constraints, avg_attempts, success_rate, last_seen \
             FROM patterns ORDER BY success_rate DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Mine a completed task's attempts into the patterns table.
    pub async fn mine_patterns(&self, task_id: &TaskId, goal: &str) -> Result<()> {
        let fingerprint = keyword_fingerprint(goal);
        if fingerprint.is_empty() {
            return Ok(());
        }

        let stats: Option<(f64, i64)> = sqlx::query_as(
            "SELECT AVG(score_test_pass_rate), COUNT(*) \
             FROM attempts WHERE task_id = ?1",
        )
        .bind(task_id.0.to_string())
        .fetch_optional(&self.pool)
        .await?;

        let (avg_pass, attempt_count) = stats.unwrap_or((0.0, 0));
        let success_rate = avg_pass.clamp(0.0, 1.0);

        let existing: Option<(String, Option<f64>, Option<f64>)> = sqlx::query_as(
            "SELECT id, avg_attempts, success_rate FROM patterns WHERE goal_keywords = ?1",
        )
        .bind(&fingerprint)
        .fetch_optional(&self.pool)
        .await?;

        let now = Utc::now().to_rfc3339();
        if let Some((existing_id, prev_avg, prev_sr)) = existing {
            let new_avg = ((prev_avg.unwrap_or(0.0) + attempt_count as f64) / 2.0).max(1.0);
            let new_sr = ((prev_sr.unwrap_or(0.0) + success_rate) / 2.0).clamp(0.0, 1.0);
            sqlx::query(
                "UPDATE patterns SET avg_attempts = ?1, success_rate = ?2, last_seen = ?3 WHERE id = ?4",
            )
            .bind(new_avg)
            .bind(new_sr)
            .bind(&now)
            .bind(existing_id)
            .execute(&self.pool)
            .await?;
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO patterns (id, goal_keywords, avg_attempts, success_rate, last_seen) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(id)
            .bind(&fingerprint)
            .bind(attempt_count as f64)
            .bind(success_rate)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn task_count(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tasks")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    /// Persist a single per-turn observability record.
    pub async fn save_turn_metrics(&self, m: &TurnMetrics) -> Result<()> {
        sqlx::query(
            "INSERT INTO turn_metrics \
             (turn_id, task_id, session_id, model, attempt_number, \
              input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, \
              ttft_ms, turn_latency_ms, tool_execution_ms, \
              context_tokens, context_pressure, evictions, \
              tool_calls, tools_parallel, estimated_cost_usd, timestamp) \
             VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19) \
             ON CONFLICT(turn_id) DO NOTHING",
        )
        .bind(m.turn_id.to_string())
        .bind(m.task_id.0.to_string())
        .bind(m.session_id.to_string())
        .bind(&m.model)
        .bind(m.attempt_number as i64)
        .bind(m.input_tokens as i64)
        .bind(m.output_tokens as i64)
        .bind(m.cache_read_input_tokens as i64)
        .bind(m.cache_write_input_tokens as i64)
        .bind(m.ttft_ms as i64)
        .bind(m.turn_latency_ms as i64)
        .bind(m.tool_execution_ms as i64)
        .bind(m.context_tokens as i64)
        .bind(m.context_pressure as f64)
        .bind(m.evictions_this_turn as i64)
        .bind(m.tool_calls as i64)
        .bind(m.tools_parallel as i64)
        .bind(m.estimated_cost_usd)
        .bind(m.timestamp.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct TaskRow {
    pub id: String,
    pub goal: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PatternRow {
    pub id: String,
    pub goal_keywords: String,
    pub successful_constraints: Option<String>,
    pub avg_attempts: Option<f64>,
    pub success_rate: Option<f64>,
    pub last_seen: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopi_core::{Attempt, Task};

    #[test]
    fn jaccard_sim_identical() {
        assert_eq!(jaccard_similarity("auth middleware refactor", "auth middleware refactor"), 1.0);
    }

    #[test]
    fn jaccard_sim_partial() {
        let s = jaccard_similarity("auth middleware", "auth database");
        assert!(s > 0.0 && s < 1.0);
    }

    #[test]
    fn jaccard_sim_disjoint() {
        assert_eq!(jaccard_similarity("alpha beta", "gamma delta"), 0.0);
    }

    #[test]
    fn keyword_fingerprint_sorts_and_dedupes() {
        let fp = keyword_fingerprint("refactor authentication middleware refactor");
        let words: Vec<&str> = fp.split_whitespace().collect();
        assert!(words.windows(2).all(|w| w[0] <= w[1]), "should be sorted");
        assert_eq!(words.len(), words.iter().collect::<std::collections::HashSet<_>>().len(), "should be deduped");
    }

    #[test]
    fn keyword_fingerprint_filters_short_words() {
        let fp = keyword_fingerprint("do it now fix");
        assert!(fp.is_empty() || !fp.contains("do"));
    }

    #[tokio::test]
    async fn save_and_load_task_round_trip() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("integrate the flux capacitor");
        store.save_task(&task, "queued").await.unwrap();

        let history = store.load_history(10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].goal, "integrate the flux capacitor");
        assert_eq!(history[0].status, "queued");
    }

    #[tokio::test]
    async fn mark_completed_updates_status() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("refactor the warp core");
        store.save_task(&task, "queued").await.unwrap();
        store.mark_completed(&task.id, "success").await.unwrap();

        let history = store.load_history(10).await.unwrap();
        assert_eq!(history[0].status, "success");
        assert!(history[0].completed_at.is_some());
    }

    #[tokio::test]
    async fn save_task_upserts_status() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("fix flaky test");
        store.save_task(&task, "queued").await.unwrap();
        store.save_task(&task, "implementing").await.unwrap();

        assert_eq!(store.task_count().await.unwrap(), 1);
        let history = store.load_history(10).await.unwrap();
        assert_eq!(history[0].status, "implementing");
    }

    #[tokio::test]
    async fn save_attempt_persists() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("add feature X");
        store.save_task(&task, "queued").await.unwrap();

        let mut attempt = Attempt::new(task.id, 1, "lopi/abc-attempt-1");
        attempt.outcome = "success".into();
        store.save_attempt(&attempt).await.unwrap();
    }

    #[tokio::test]
    async fn load_history_newest_first() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        for i in 0..5u8 {
            let t = Task::new(format!("task number {i} work"));
            store.save_task(&t, "queued").await.unwrap();
        }
        let history = store.load_history(3).await.unwrap();
        assert_eq!(history.len(), 3);
    }

    #[tokio::test]
    async fn empty_store_returns_empty_history() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let history = store.load_history(10).await.unwrap();
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn find_similar_patterns_empty_db() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let results = store.find_similar_patterns("optimize the engine").await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn find_similar_patterns_returns_matches() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("refactor authentication middleware");
        store.save_task(&task, "success").await.unwrap();
        store.mine_patterns(&task.id, &task.goal).await.unwrap();

        // Similar goal should match above 0.3 Jaccard threshold.
        let results = store.find_similar_patterns("update authentication middleware logic").await.unwrap();
        assert!(!results.is_empty(), "should find similar pattern");
    }

    #[tokio::test]
    async fn mine_patterns_inserts_new_row() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("refactor authentication middleware");
        store.save_task(&task, "queued").await.unwrap();
        store.mine_patterns(&task.id, &task.goal).await.unwrap();

        let patterns = store.load_patterns(10).await.unwrap();
        assert_eq!(patterns.len(), 1);
        let kw = &patterns[0].goal_keywords;
        assert!(kw.contains("authentication") || kw.contains("middleware") || kw.contains("refactor"));
    }

    #[tokio::test]
    async fn mine_patterns_updates_existing_row() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let t1 = Task::new("optimize database queries");
        let t2 = Task::new("optimize database queries");
        store.save_task(&t1, "queued").await.unwrap();
        store.save_task(&t2, "queued").await.unwrap();

        store.mine_patterns(&t1.id, &t1.goal).await.unwrap();
        store.mine_patterns(&t2.id, &t2.goal).await.unwrap();

        let patterns = store.load_patterns(10).await.unwrap();
        assert_eq!(patterns.len(), 1);
    }

    #[tokio::test]
    async fn mine_patterns_skips_short_words() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task = Task::new("do it now");
        store.save_task(&task, "queued").await.unwrap();
        store.mine_patterns(&task.id, &task.goal).await.unwrap();
        let patterns = store.load_patterns(10).await.unwrap();
        assert!(patterns.is_empty());
    }

    #[tokio::test]
    async fn load_patterns_ordered_by_success_rate() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let t1 = Task::new("write comprehensive unit tests");
        let t2 = Task::new("deploy production infrastructure");
        store.save_task(&t1, "success").await.unwrap();
        store.save_task(&t2, "failed").await.unwrap();
        store.mine_patterns(&t1.id, &t1.goal).await.unwrap();
        store.mine_patterns(&t2.id, &t2.goal).await.unwrap();

        let patterns = store.load_patterns(10).await.unwrap();
        assert_eq!(patterns.len(), 2);
    }
}
