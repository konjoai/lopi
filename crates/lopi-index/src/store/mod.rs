//! `SQLite`-backed symbol/ref store — connection setup and schema
//! application. Deliberately a separate database file from
//! `lopi-memory::MemoryStore` (`.lopi/index.db`, not the pattern-miner's
//! `lopi.db`): that schema feeds the pattern miner and must stay stable, and
//! this one is keyed to a commit and rebuilt/reindexed far more often.
//!
//! Connection setup shares `lopi-memory::MemoryStore::open`'s dual-pool WAL
//! pattern via `lopi_core::sqlite_pool` (one writer, up to 8 readers), with
//! `foreign_keys` turned on — `refs.to_symbol_id`'s `ON DELETE SET NULL`
//! and `refs.from_symbol_id`'s `ON DELETE CASCADE` are both inert without it
//! (`SQLite` disables FK enforcement per-connection by default), and unlike
//! `lopi-memory` this schema is new enough that there's no pre-existing data
//! written under no enforcement to worry about breaking.

mod refs;
mod symbols;

use anyhow::Result;
use lopi_core::sqlite_pool::{open_in_memory_pool, open_read_pool, open_write_pool};
use sqlx::sqlite::SqlitePool;
use std::path::Path;

pub use refs::{RefDirection, RefHit};
pub use symbols::SymbolFilter;

const SCHEMA: &str = include_str!("../schema.sql");

/// The gitignored, per-repo index database's conventional location, relative
/// to a repo's root.
pub const INDEX_DB_REL_PATH: &str = ".lopi/index.db";

/// `SQLite` dual-pool symbol/ref store: one serializing write connection, up
/// to 8 read-only connections.
#[derive(Clone)]
pub struct IndexStore {
    write_pool: SqlitePool,
    read_pool: SqlitePool,
}

impl IndexStore {
    /// Open or create a persistent index database at `path`.
    ///
    /// # Errors
    /// Returns `Err` if the database cannot be created or the schema cannot be applied.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let write_pool = open_write_pool(path, true).await?;
        Self::apply_schema(&write_pool).await?;
        let read_pool = open_read_pool(path, 8, true).await?;
        Ok(Self {
            write_pool,
            read_pool,
        })
    }

    /// Open an in-memory index database — used by tests and by a caller that
    /// wants a scratch index without touching disk.
    ///
    /// # Errors
    /// Returns `Err` if the in-memory database cannot be opened or the schema cannot be applied.
    pub async fn open_in_memory() -> Result<Self> {
        let pool = open_in_memory_pool(true).await?;
        Self::apply_schema(&pool).await?;
        Ok(Self {
            write_pool: pool.clone(),
            read_pool: pool,
        })
    }

    async fn apply_schema(pool: &SqlitePool) -> Result<()> {
        lopi_core::sqlite_pool::apply_schema(pool, SCHEMA).await
    }

    /// Read one meta key (`indexed_commit`, `schema_version`, …).
    ///
    /// # Errors
    /// Returns `Err` on a query failure.
    pub async fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.read_pool)
            .await?;
        Ok(row.map(|(v,)| v))
    }

    /// Upsert one meta key.
    ///
    /// # Errors
    /// Returns `Err` on a write failure.
    pub async fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO meta (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    pub(crate) fn write_pool(&self) -> &SqlitePool {
        &self.write_pool
    }

    pub(crate) fn read_pool(&self) -> &SqlitePool {
        &self.read_pool
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::IndexStore;

    #[tokio::test]
    async fn open_in_memory_applies_schema_and_is_idempotent_on_reopen() {
        let store = IndexStore::open_in_memory().await.unwrap();
        store.set_meta("schema_version", "1").await.unwrap();
        assert_eq!(
            store.get_meta("schema_version").await.unwrap(),
            Some("1".into())
        );
    }

    #[tokio::test]
    async fn open_persistent_db_survives_reopen() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".lopi/index.db");
        {
            let store = IndexStore::open(&path).await.unwrap();
            store.set_meta("indexed_commit", "abc123").await.unwrap();
        }
        let reopened = IndexStore::open(&path).await.unwrap();
        assert_eq!(
            reopened.get_meta("indexed_commit").await.unwrap(),
            Some("abc123".into())
        );
    }

    #[tokio::test]
    async fn missing_meta_key_is_none() {
        let store = IndexStore::open_in_memory().await.unwrap();
        assert_eq!(store.get_meta("nope").await.unwrap(), None);
    }
}
