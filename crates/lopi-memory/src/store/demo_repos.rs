//! Synthetic repo descriptors for `lopi demo`.
//!
//! Real repos are discovered by filesystem scan in `lopi-ui` — there is no
//! `repos` table for them. Demo mode must never touch the real filesystem,
//! so its "repo list" is this table instead: rows only ever exist on a
//! store built by `lopi demo`'s fixture generator, empty on every real
//! store.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::MemoryStore;

/// A synthetic repo descriptor written by `lopi demo`'s fixture generator.
/// Empty on every real store.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DemoRepoRow {
    /// Repo name, e.g. `"aurora-api"`. Primary key — a second insert with
    /// the same name upserts in place.
    pub name: String,
    /// Coarse stack label, e.g. `"Rust service"`, `"TypeScript web app"`.
    pub stack: String,
    /// Synthetic filesystem path, e.g. `"/demo/repos/aurora-api"` — never a
    /// real path on disk.
    pub path: String,
    /// Short human-readable description.
    pub description: String,
    /// Caller-assigned display order — [`MemoryStore::load_demo_repos`]
    /// sorts by this field so insertion order is reproducible across loads
    /// regardless of SQLite's default row order.
    pub sort_order: i64,
}

impl MemoryStore {
    /// Insert one synthetic repo descriptor (upsert by name).
    ///
    /// # Errors
    /// Returns `Err` if the database write fails.
    pub async fn insert_demo_repo(&self, row: &DemoRepoRow) -> Result<()> {
        sqlx::query(
            "INSERT INTO demo_repos (name, stack, path, description, sort_order) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(name) DO UPDATE SET \
             stack = excluded.stack, path = excluded.path, \
             description = excluded.description, sort_order = excluded.sort_order",
        )
        .bind(&row.name)
        .bind(&row.stack)
        .bind(&row.path)
        .bind(&row.description)
        .bind(row.sort_order)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Load every synthetic repo descriptor, ordered by `sort_order`.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_demo_repos(&self) -> Result<Vec<DemoRepoRow>> {
        let rows = sqlx::query_as::<_, DemoRepoRow>(
            "SELECT name, stack, path, description, sort_order \
             FROM demo_repos ORDER BY sort_order ASC",
        )
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    fn repo(name: &str, order: i64) -> DemoRepoRow {
        DemoRepoRow {
            name: name.into(),
            stack: "Rust service".into(),
            path: format!("/demo/repos/{name}"),
            description: format!("{name} description"),
            sort_order: order,
        }
    }

    #[tokio::test]
    async fn insert_then_load_round_trips_all_fields() {
        let s = store().await;
        s.insert_demo_repo(&repo("aurora-api", 0)).await.unwrap();
        let rows = s.load_demo_repos().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "aurora-api");
        assert_eq!(rows[0].stack, "Rust service");
        assert_eq!(rows[0].path, "/demo/repos/aurora-api");
        assert_eq!(rows[0].description, "aurora-api description");
        assert_eq!(rows[0].sort_order, 0);
    }

    #[tokio::test]
    async fn loading_empty_store_returns_empty() {
        let s = store().await;
        assert!(s.load_demo_repos().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn multiple_inserts_preserve_stable_order_across_loads() {
        let s = store().await;
        s.insert_demo_repo(&repo("zeta", 2)).await.unwrap();
        s.insert_demo_repo(&repo("alpha", 0)).await.unwrap();
        s.insert_demo_repo(&repo("mid", 1)).await.unwrap();

        for _ in 0..3 {
            let rows = s.load_demo_repos().await.unwrap();
            let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
            assert_eq!(names, vec!["alpha", "mid", "zeta"]);
        }
    }

    #[tokio::test]
    async fn insert_upserts_by_name() {
        let s = store().await;
        s.insert_demo_repo(&repo("aurora-api", 0)).await.unwrap();
        let mut updated = repo("aurora-api", 5);
        updated.stack = "TypeScript web app".into();
        s.insert_demo_repo(&updated).await.unwrap();
        let rows = s.load_demo_repos().await.unwrap();
        assert_eq!(rows.len(), 1, "same name upserts in place");
        assert_eq!(rows[0].stack, "TypeScript web app");
        assert_eq!(rows[0].sort_order, 5);
    }
}
