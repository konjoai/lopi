//! Shared `SQLite` dual-pool WAL connection setup — one serializing write
//! connection, up to N read-only connections. `lopi-memory::MemoryStore`
//! and `lopi-index::IndexStore` both need exactly this shape (a single
//! writer avoids `SQLite`'s one-concurrent-writer limit becoming lock
//! contention; a separate read-only pool lets `SELECT`s run concurrently
//! under WAL without blocking or being blocked by writes) — this module is
//! the one place that pattern is written down, so a third caller never has
//! to copy it by hand again.

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

/// Open a single-connection write pool for `path`, WAL + `synchronous =
/// NORMAL`, creating the file (and its parent directory, if missing) on
/// first use.
///
/// `foreign_keys` is explicit, not defaulted on: `SQLite` disables FK
/// enforcement per-connection unless asked, and a caller whose schema
/// predates this helper (rows already written under no enforcement) needs
/// to opt in deliberately rather than have enforcement silently begin
/// rejecting inserts it previously allowed.
///
/// Apply the caller's schema against this pool before opening a read pool
/// on the same path, so concurrent readers never race table creation.
///
/// # Errors
/// Returns `Err` if the path can't be parsed as a `SQLite` URL or the pool
/// can't be opened.
pub async fn open_write_pool(path: &Path, foreign_keys: bool) -> Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let url = format!("sqlite://{}", path.display());
    let mut opts = SqliteConnectOptions::from_str(&url)
        .context("parsing sqlite path (write)")?
        .create_if_missing(true)
        .pragma("journal_mode", "WAL")
        .pragma("synchronous", "NORMAL")
        .pragma("busy_timeout", "5000");
    if foreign_keys {
        opts = opts.pragma("foreign_keys", "ON");
    }
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .context("opening sqlite write pool")
}

/// Open an up-to-`max_connections` read-only pool for `path`. Call after
/// the write pool has applied its schema (see [`open_write_pool`]). See
/// that function's doc comment for why `foreign_keys` is explicit.
///
/// # Errors
/// Returns `Err` if the path can't be parsed as a `SQLite` URL or the pool
/// can't be opened.
pub async fn open_read_pool(
    path: &Path,
    max_connections: u32,
    foreign_keys: bool,
) -> Result<SqlitePool> {
    let url = format!("sqlite://{}", path.display());
    let mut opts = SqliteConnectOptions::from_str(&url)
        .context("parsing sqlite path (read)")?
        .read_only(true)
        .pragma("busy_timeout", "5000");
    if foreign_keys {
        opts = opts.pragma("foreign_keys", "ON");
    }
    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(opts)
        .await
        .context("opening sqlite read pool")
}

/// Open a single shared in-memory pool — one connection, serving both reads
/// and writes, since `:memory:` doesn't support WAL or multiple connections
/// sharing state. Meant for tests; apply the caller's schema against the
/// returned pool before using it. See [`open_write_pool`]'s doc comment for
/// why `foreign_keys` is explicit.
///
/// # Errors
/// Returns `Err` if the in-memory pool can't be opened.
pub async fn open_in_memory_pool(foreign_keys: bool) -> Result<SqlitePool> {
    let mut opts =
        SqliteConnectOptions::from_str("sqlite::memory:").context("parsing in-memory sqlite")?;
    if foreign_keys {
        opts = opts.pragma("foreign_keys", "ON");
    }
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .context("opening in-memory sqlite pool")
}

/// Apply a schema file's `CREATE TABLE IF NOT EXISTS`/`ALTER TABLE ... ADD
/// COLUMN` statements against `pool`, idempotently — safe to call on every
/// `open()`, not just first-run. `CREATE TABLE IF NOT EXISTS` is naturally
/// re-runnable; an `ALTER TABLE` statement's duplicate-column error (already
/// applied in a prior run) is swallowed rather than propagated, which is
/// what makes appending new `ALTER TABLE` statements to a schema file a safe
/// way to evolve it without a numbered-migrations system. Any other
/// statement's error is fatal (schema file, not a caller mistake, so
/// wrong-enough to bubble up rather than retry-swallow).
///
/// Splits on a literal `;`, then strips `--`-prefixed comment lines from
/// each resulting chunk — so a schema file's own comments must never
/// contain a semicolon themselves, or that character splits the comment
/// line in half and leaks its back half into the next chunk as literal
/// (invalid) SQL.
///
/// # Errors
/// Returns `Err` on the first statement that fails and isn't a duplicate-column `ALTER TABLE`.
pub async fn apply_schema(pool: &SqlitePool, schema_sql: &str) -> Result<()> {
    for stmt in schema_sql.split(';') {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }
        let body: String = s
            .lines()
            .filter(|l| !l.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if body.is_empty() {
            continue;
        }
        let result = sqlx::query(&body).execute(pool).await;
        if let Err(e) = result {
            if !body.to_lowercase().starts_with("alter table") {
                return Err(e).context("applying schema");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[tokio::test]
    async fn write_pool_creates_missing_parent_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested/db.sqlite");
        let pool = open_write_pool(&path, true).await.unwrap();
        sqlx::query("CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        assert!(path.exists());
    }

    #[tokio::test]
    async fn read_pool_cannot_write() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("db.sqlite");
        let write_pool = open_write_pool(&path, true).await.unwrap();
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .execute(&write_pool)
            .await
            .unwrap();

        let read_pool = open_read_pool(&path, 4, true).await.unwrap();
        let err = sqlx::query("INSERT INTO t (id) VALUES (1)")
            .execute(&read_pool)
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("read"), "{err}");
    }

    #[tokio::test]
    async fn in_memory_pool_reads_back_a_write() {
        let pool = open_in_memory_pool(true).await.unwrap();
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t (id) VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();
        let row: (i64,) = sqlx::query_as("SELECT id FROM t")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 1);
    }

    #[tokio::test]
    async fn apply_schema_is_idempotent_across_create_and_alter() {
        let pool = open_in_memory_pool(false).await.unwrap();
        let schema = "\
            CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY);\n\
            ALTER TABLE t ADD COLUMN name TEXT;\n";
        apply_schema(&pool, schema).await.unwrap();
        apply_schema(&pool, schema).await.unwrap();
        sqlx::query("INSERT INTO t (id, name) VALUES (1, 'x')")
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn apply_schema_strips_comment_lines() {
        let pool = open_in_memory_pool(false).await.unwrap();
        let schema = "-- a comment\nCREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY);\n";
        apply_schema(&pool, schema).await.unwrap();
        sqlx::query("INSERT INTO t (id) VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn apply_schema_propagates_non_alter_errors() {
        let pool = open_in_memory_pool(false).await.unwrap();
        let err = apply_schema(&pool, "SELECT * FROM nonexistent_table;")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("applying schema"));
    }
}
