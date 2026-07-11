//! Append-only audit log.
//!
//! One row per actionable event across the orchestrator: task submit /
//! dispatch / DLQ entry, breaker trips, cache hit / miss, tool register /
//! deregister. The shape of `payload` is
//! per-action and intentionally schemaless — query tools project the
//! JSON they care about.
//!
//! Writes are cheap (single INSERT, no synchronous fsync because the
//! whole table is best-effort observability) and the read API supports
//! cursor pagination by autoincrement `id` so a UI tail loop can stream
//! new rows without re-scanning the entire table.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row as _;

use super::MemoryStore;

/// One audit row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRow {
    /// Monotonically-increasing row id (cursor).
    pub id: i64,
    /// ISO-8601 wall clock.
    pub ts: String,
    /// Stable action label — `task.dispatch`, `task.dead_letter`,
    /// `cache.hit`, `cache.miss`, `breaker.trip`, etc.
    pub action: String,
    /// Optional subject kind: `task` / `agent` / `tool`.
    pub subject_type: Option<String>,
    /// Optional subject identifier (usually a UUID or name).
    pub subject_id: Option<String>,
    /// Who initiated this event — `pool`, `api`, `webhook`, `telegram`, ...
    pub actor: Option<String>,
    /// Action-specific JSON payload (validated by callers, not the store).
    pub payload: Option<String>,
}

/// Inputs to [`MemoryStore::record_audit`]. Mirrors `AuditRow` minus
/// `id` and `ts`, both of which the store fills in.
#[derive(Debug, Clone)]
pub struct AuditInput {
    /// Short label describing the action taken (e.g. `"task.started"`).
    pub action: String,
    /// Category of the object the action was performed on.
    pub subject_type: Option<String>,
    /// Identifier of the specific subject object.
    pub subject_id: Option<String>,
    /// Label identifying who or what triggered the action.
    pub actor: Option<String>,
    /// Optional JSON-encoded payload with structured context.
    pub payload: Option<String>,
}

impl AuditInput {
    /// Build an entry from an `action` label only — everything else nil.
    #[must_use]
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            subject_type: None,
            subject_id: None,
            actor: None,
            payload: None,
        }
    }

    /// Builder helper — set the `(subject_type, subject_id)` pair.
    #[must_use]
    pub fn subject(mut self, kind: impl Into<String>, id: impl Into<String>) -> Self {
        self.subject_type = Some(kind.into());
        self.subject_id = Some(id.into());
        self
    }

    /// Builder helper — set the actor label.
    #[must_use]
    pub fn actor(mut self, who: impl Into<String>) -> Self {
        self.actor = Some(who.into());
        self
    }

    /// Builder helper — set a JSON payload (already-stringified).
    #[must_use]
    pub fn payload_json(mut self, json: impl Into<String>) -> Self {
        self.payload = Some(json.into());
        self
    }
}

/// Query filter for [`MemoryStore::query_audit`]. All fields optional —
/// unset fields are not filtered.
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// Only return rows with `id > since`. `0` = from the beginning.
    pub since_id: i64,
    /// Only return rows whose `action` matches.
    pub action: Option<String>,
    /// Only return rows whose `(subject_type, subject_id)` matches.
    pub subject_type: Option<String>,
    /// Subject ID filter paired with `subject_type`.
    pub subject_id: Option<String>,
    /// Page size — clamped to [1, 1000] by the store. Defaults to 100.
    pub limit: i64,
}

impl MemoryStore {
    /// Append one audit row. Returns the new `id`.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite insert fails.
    pub async fn record_audit(&self, input: &AuditInput) -> Result<i64> {
        let ts = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "INSERT INTO audit_log (ts, action, subject_type, subject_id, actor, payload)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&ts)
        .bind(&input.action)
        .bind(&input.subject_type)
        .bind(&input.subject_id)
        .bind(&input.actor)
        .bind(&input.payload)
        .execute(&self.write_pool)
        .await
        .context("inserting audit_log row")?;
        Ok(res.last_insert_rowid())
    }

    /// Cursor-paginated query — returns rows in `id ASC` order so a tail
    /// loop can pass back the largest `id` it has seen.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn query_audit(&self, q: &AuditQuery) -> Result<Vec<AuditRow>> {
        let limit = q.limit.clamp(1, 1000);
        // Build the SQL conditionally so absent filters don't constrain
        // the query plan.
        let mut sql = String::from(
            "SELECT id, ts, action, subject_type, subject_id, actor, payload
             FROM audit_log
             WHERE id > ?",
        );
        if q.action.is_some() {
            sql.push_str(" AND action = ?");
        }
        if q.subject_type.is_some() {
            sql.push_str(" AND subject_type = ?");
        }
        if q.subject_id.is_some() {
            sql.push_str(" AND subject_id = ?");
        }
        sql.push_str(" ORDER BY id ASC LIMIT ?");

        let mut query = sqlx::query(&sql).bind(q.since_id);
        if let Some(a) = &q.action {
            query = query.bind(a);
        }
        if let Some(st) = &q.subject_type {
            query = query.bind(st);
        }
        if let Some(sid) = &q.subject_id {
            query = query.bind(sid);
        }
        query = query.bind(limit);

        let rows = query
            .fetch_all(&self.read_pool)
            .await
            .context("querying audit_log")?;
        Ok(rows.into_iter().map(audit_from_row).collect())
    }

    /// Total row count — for `/metrics` and dashboards.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite query fails.
    pub async fn count_audit(&self) -> Result<u64> {
        let row = sqlx::query("SELECT COUNT(*) as c FROM audit_log")
            .fetch_one(&self.read_pool)
            .await
            .context("counting audit_log")?;
        let c: i64 = row.get("c");
        Ok(u64::try_from(c).unwrap_or(0))
    }
}

fn audit_from_row(row: sqlx::sqlite::SqliteRow) -> AuditRow {
    AuditRow {
        id: row.get("id"),
        ts: row.get("ts"),
        action: row.get("action"),
        subject_type: row.get("subject_type"),
        subject_id: row.get("subject_id"),
        actor: row.get("actor"),
        payload: row.get("payload"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn record_assigns_monotonic_ids() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let a = store
            .record_audit(&AuditInput::new("task.dispatch"))
            .await
            .unwrap();
        let b = store
            .record_audit(&AuditInput::new("task.dispatch"))
            .await
            .unwrap();
        let c = store
            .record_audit(&AuditInput::new("task.dispatch"))
            .await
            .unwrap();
        assert!(b > a && c > b, "ids increase strictly: {a} < {b} < {c}");
        assert_eq!(store.count_audit().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn query_filters_by_action() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .record_audit(&AuditInput::new("task.dispatch"))
            .await
            .unwrap();
        store
            .record_audit(&AuditInput::new("cache.hit"))
            .await
            .unwrap();
        store
            .record_audit(&AuditInput::new("cache.miss"))
            .await
            .unwrap();
        let q = AuditQuery {
            action: Some("cache.hit".into()),
            limit: 10,
            ..AuditQuery::default()
        };
        let rows = store.query_audit(&q).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, "cache.hit");
    }

    #[tokio::test]
    async fn query_filters_by_subject() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let alpha = AuditInput::new("task.dispatch").subject("task", "alpha");
        let beta = AuditInput::new("task.dispatch").subject("task", "beta");
        store.record_audit(&alpha).await.unwrap();
        store.record_audit(&beta).await.unwrap();
        store.record_audit(&beta).await.unwrap();
        let q = AuditQuery {
            subject_type: Some("task".into()),
            subject_id: Some("beta".into()),
            limit: 10,
            ..AuditQuery::default()
        };
        let rows = store.query_audit(&q).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.subject_id.as_deref() == Some("beta")));
    }

    #[tokio::test]
    async fn query_paginates_with_since_id() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let mut ids = Vec::new();
        for i in 0..5 {
            ids.push(
                store
                    .record_audit(&AuditInput::new(format!("e.{i}")))
                    .await
                    .unwrap(),
            );
        }
        // First page — 3 rows from the beginning.
        let page1 = store
            .query_audit(&AuditQuery {
                since_id: 0,
                limit: 3,
                ..AuditQuery::default()
            })
            .await
            .unwrap();
        assert_eq!(page1.len(), 3);
        let cursor = page1.last().unwrap().id;
        // Second page — picks up after the cursor.
        let page2 = store
            .query_audit(&AuditQuery {
                since_id: cursor,
                limit: 10,
                ..AuditQuery::default()
            })
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);
        assert!(page2.iter().all(|r| r.id > cursor));
    }

    #[tokio::test]
    async fn query_respects_limit_clamp() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        for i in 0..6 {
            store
                .record_audit(&AuditInput::new(format!("e.{i}")))
                .await
                .unwrap();
        }
        // limit=0 / negative clamps to 1.
        let r = store
            .query_audit(&AuditQuery {
                limit: 0,
                ..AuditQuery::default()
            })
            .await
            .unwrap();
        assert_eq!(r.len(), 1, "limit=0 clamped to 1");
    }

    #[tokio::test]
    async fn payload_round_trips_as_json_text() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let json = r#"{"breaker":"agent","scope":"task","burned_usd":1.50}"#;
        store
            .record_audit(
                &AuditInput::new("breaker.trip")
                    .subject("breaker", "task")
                    .actor("pool")
                    .payload_json(json),
            )
            .await
            .unwrap();
        let rows = store
            .query_audit(&AuditQuery {
                action: Some("breaker.trip".into()),
                limit: 10,
                ..AuditQuery::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].payload.as_deref(), Some(json));
        assert_eq!(rows[0].actor.as_deref(), Some("pool"));
        assert_eq!(rows[0].subject_type.as_deref(), Some("breaker"));
    }
}
