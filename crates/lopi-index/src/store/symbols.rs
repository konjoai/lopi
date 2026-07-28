//! Symbol table CRUD: file-scoped insert/delete (the incremental reindex
//! unit) plus the filtered reads `query.rs`'s tools build on.

use crate::types::{Language, NewSymbol, Symbol, SymbolKind};
use anyhow::Result;
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, Row};

impl super::IndexStore {
    /// Delete every symbol (and, via `ON DELETE CASCADE`/explicit ref
    /// cleanup, every ref) rooted at `path`, then insert `symbols` for it.
    /// One transaction — a reindexed file's old and new rows are never both
    /// visible mid-write.
    ///
    /// Returns `(removed_symbol_count, local_id -> assigned_db_id)` — the
    /// map lets the caller insert this file's refs (which reference symbols
    /// by `local_id`) in a second step.
    ///
    /// Also upserts `files`, in the same transaction, regardless of whether
    /// `symbols` is empty — a file that parses to zero symbols still needs
    /// its hash recorded somewhere, or the dirty-tree sweep in `reindex.rs`
    /// would see it as changed on every single incremental pass (no symbol
    /// row means no `file_hash` to compare against).
    ///
    /// # Errors
    /// Returns `Err` on a write failure; the transaction rolls back.
    pub async fn replace_file_symbols(
        &self,
        repo_id: &str,
        path: &str,
        lang: Language,
        file_hash: &str,
        symbols: &[NewSymbol],
    ) -> Result<(usize, std::collections::HashMap<usize, i64>)> {
        let mut tx = self.write_pool().begin().await?;
        sqlx::query(
            "INSERT INTO files (repo_id, path, lang, file_hash) VALUES (?, ?, ?, ?)
             ON CONFLICT(repo_id, path) DO UPDATE SET lang = excluded.lang, file_hash = excluded.file_hash",
        )
        .bind(repo_id)
        .bind(path)
        .bind(lang.as_str())
        .bind(file_hash)
        .execute(&mut *tx)
        .await?;
        let removed: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM symbols WHERE repo_id = ? AND path = ?")
                .bind(repo_id)
                .bind(path)
                .fetch_one(&mut *tx)
                .await?;
        sqlx::query("DELETE FROM refs WHERE repo_id = ? AND path = ?")
            .bind(repo_id)
            .bind(path)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM symbols WHERE repo_id = ? AND path = ?")
            .bind(repo_id)
            .bind(path)
            .execute(&mut *tx)
            .await?;

        let mut local_to_db = std::collections::HashMap::with_capacity(symbols.len());
        for sym in symbols {
            let result = sqlx::query(
                "INSERT INTO symbols
                 (repo_id, path, lang, kind, name, qualified_name, signature,
                  doc_first_line, parent_id, line_start, line_end, byte_start,
                  byte_end, file_hash, is_public)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?)",
            )
            .bind(repo_id)
            .bind(path)
            .bind(sym.lang.as_str())
            .bind(sym.kind.as_str())
            .bind(&sym.name)
            .bind(&sym.qualified_name)
            .bind(&sym.signature)
            .bind(&sym.doc_first_line)
            .bind(sym.line_start)
            .bind(sym.line_end)
            .bind(sym.byte_start)
            .bind(sym.byte_end)
            .bind(file_hash)
            .bind(sym.is_public)
            .execute(&mut *tx)
            .await?;
            local_to_db.insert(sym.local_id, result.last_insert_rowid());
        }
        for sym in symbols {
            if let Some(local_parent) = sym.local_parent {
                if let (Some(&child_id), Some(&parent_id)) = (
                    local_to_db.get(&sym.local_id),
                    local_to_db.get(&local_parent),
                ) {
                    sqlx::query("UPDATE symbols SET parent_id = ? WHERE id = ?")
                        .bind(parent_id)
                        .bind(child_id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
        tx.commit().await?;
        Ok((removed.0 as usize, local_to_db))
    }

    /// Remove every symbol/ref/`files` row rooted at `path` (the file no
    /// longer exists).
    ///
    /// # Errors
    /// Returns `Err` on a write failure.
    pub async fn remove_file(&self, repo_id: &str, path: &str) -> Result<usize> {
        let removed: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM symbols WHERE repo_id = ? AND path = ?")
                .bind(repo_id)
                .bind(path)
                .fetch_one(self.write_pool())
                .await?;
        sqlx::query("DELETE FROM refs WHERE repo_id = ? AND path = ?")
            .bind(repo_id)
            .bind(path)
            .execute(self.write_pool())
            .await?;
        sqlx::query("DELETE FROM symbols WHERE repo_id = ? AND path = ?")
            .bind(repo_id)
            .bind(path)
            .execute(self.write_pool())
            .await?;
        sqlx::query("DELETE FROM files WHERE repo_id = ? AND path = ?")
            .bind(repo_id)
            .bind(path)
            .execute(self.write_pool())
            .await?;
        Ok(removed.0 as usize)
    }

    /// The stored hash for `path`, independent of how many symbols (if any)
    /// it produced — the dirty-tree sweep's per-file change signal.
    ///
    /// # Errors
    /// Returns `Err` on a query failure.
    pub async fn get_file_hash(&self, repo_id: &str, path: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT file_hash FROM files WHERE repo_id = ? AND path = ?")
                .bind(repo_id)
                .bind(path)
                .fetch_optional(self.read_pool())
                .await?;
        Ok(row.map(|(h,)| h))
    }

    /// Total symbol rows for `repo_id`.
    ///
    /// # Errors
    /// Returns `Err` on a query failure.
    pub async fn symbol_count(&self, repo_id: &str) -> Result<usize> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM symbols WHERE repo_id = ?")
            .bind(repo_id)
            .fetch_one(self.read_pool())
            .await?;
        Ok(row.0 as usize)
    }

    /// The exact-match qualified-name lookup `lopi_read` uses.
    ///
    /// # Errors
    /// Returns `Err` on a query failure.
    pub async fn get_by_qualified_name(
        &self,
        repo_id: &str,
        qualified_name: &str,
    ) -> Result<Option<Symbol>> {
        let row =
            sqlx::query("SELECT * FROM symbols WHERE repo_id = ? AND qualified_name = ? LIMIT 1")
                .bind(repo_id)
                .bind(qualified_name)
                .fetch_optional(self.read_pool())
                .await?;
        Ok(row.map(|r| symbol_from_row(&r)))
    }

    /// Every symbol matching `filter`, sorted deterministically by
    /// `(path, line_start, name)` — the candidate pool `query.rs::find`
    /// fuzzy-scores, and the source `map.rs` sorts its public surface from.
    ///
    /// # Errors
    /// Returns `Err` on a query failure.
    pub async fn list(&self, repo_id: &str, filter: &SymbolFilter<'_>) -> Result<Vec<Symbol>> {
        let mut sql = String::from("SELECT * FROM symbols WHERE repo_id = ?");
        if filter.kind.is_some() {
            sql.push_str(" AND kind = ?");
        }
        if filter.lang.is_some() {
            sql.push_str(" AND lang = ?");
        }
        if filter.path_glob.is_some() {
            sql.push_str(" AND path GLOB ?");
        }
        sql.push_str(" ORDER BY path, line_start, name");

        let mut q = sqlx::query(&sql).bind(repo_id);
        if let Some(k) = filter.kind {
            q = q.bind(k.as_str());
        }
        if let Some(l) = filter.lang {
            q = q.bind(l.as_str());
        }
        if let Some(g) = filter.path_glob {
            q = q.bind(g);
        }
        let rows = q.fetch_all(self.read_pool()).await?;
        Ok(rows.iter().map(symbol_from_row).collect())
    }

    /// Symbols ranked by inbound ref count, descending then by qualified
    /// name — the repo map's "most referenced" orientation list.
    ///
    /// # Errors
    /// Returns `Err` on a query failure.
    pub async fn most_referenced(&self, repo_id: &str, limit: u32) -> Result<Vec<(Symbol, i64)>> {
        let rows = sqlx::query(
            "SELECT s.*, COUNT(r.id) AS inbound
             FROM symbols s
             JOIN refs r ON r.to_symbol_id = s.id AND r.repo_id = s.repo_id
             WHERE s.repo_id = ?
             GROUP BY s.id
             ORDER BY inbound DESC, s.qualified_name ASC
             LIMIT ?",
        )
        .bind(repo_id)
        .bind(limit)
        .fetch_all(self.read_pool())
        .await?;
        Ok(rows
            .iter()
            .map(|r| (symbol_from_row(r), r.get::<i64, _>("inbound")))
            .collect())
    }
}

/// Filter for [`super::IndexStore::list`]. `None` on a field means
/// "unfiltered on that axis".
#[derive(Debug, Clone, Copy, Default)]
pub struct SymbolFilter<'a> {
    /// Restrict to one [`SymbolKind`].
    pub kind: Option<SymbolKind>,
    /// Restrict to one [`Language`].
    pub lang: Option<Language>,
    /// Restrict to paths matching this `SQLite` `GLOB` pattern.
    pub path_glob: Option<&'a str>,
}

fn symbol_from_row(row: &SqliteRow) -> Symbol {
    Symbol {
        id: row.get("id"),
        repo_id: row.get("repo_id"),
        path: row.get("path"),
        lang: Language::parse(row.get::<String, _>("lang").as_str()).unwrap_or(Language::Rust),
        kind: SymbolKind::parse(row.get::<String, _>("kind").as_str()).unwrap_or(SymbolKind::Fn),
        name: row.get("name"),
        qualified_name: row.get("qualified_name"),
        signature: row.get("signature"),
        doc_first_line: row.get("doc_first_line"),
        parent_id: row.get("parent_id"),
        line_start: row.get::<i64, _>("line_start") as u32,
        line_end: row.get::<i64, _>("line_end") as u32,
        byte_start: row.get::<i64, _>("byte_start") as u32,
        byte_end: row.get::<i64, _>("byte_end") as u32,
        file_hash: row.get("file_hash"),
        is_public: row.get("is_public"),
    }
}

/// Allow `sqlx::query_as::<_, Symbol>` in tests without hand-rolling `FromRow`.
impl FromRow<'_, SqliteRow> for Symbol {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(symbol_from_row(row))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::SymbolFilter;
    use crate::store::IndexStore;
    use crate::types::{Language, NewSymbol, SymbolKind};

    fn sample_symbol(local_id: usize, name: &str) -> NewSymbol {
        NewSymbol {
            local_id,
            local_parent: None,
            lang: Language::Rust,
            kind: SymbolKind::Fn,
            name: name.into(),
            qualified_name: name.into(),
            signature: format!("fn {name}()"),
            doc_first_line: None,
            line_start: 1,
            line_end: 3,
            byte_start: 0,
            byte_end: 10,
            is_public: true,
        }
    }

    #[tokio::test]
    async fn insert_then_list_round_trips() {
        let store = IndexStore::open_in_memory().await.unwrap();
        let syms = vec![sample_symbol(0, "run")];
        let (removed, map) = store
            .replace_file_symbols("repo", "src/lib.rs", Language::Rust, "hash1", &syms)
            .await
            .unwrap();
        assert_eq!(removed, 0);
        assert_eq!(map.len(), 1);

        let all = store.list("repo", &SymbolFilter::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "run");
        assert_eq!(all[0].signature, "fn run()");
    }

    #[tokio::test]
    async fn reindexing_a_file_replaces_its_symbols() {
        let store = IndexStore::open_in_memory().await.unwrap();
        store
            .replace_file_symbols(
                "repo",
                "src/lib.rs",
                Language::Rust,
                "hash1",
                &[sample_symbol(0, "old")],
            )
            .await
            .unwrap();
        let (removed, _) = store
            .replace_file_symbols(
                "repo",
                "src/lib.rs",
                Language::Rust,
                "hash2",
                &[sample_symbol(0, "new")],
            )
            .await
            .unwrap();
        assert_eq!(removed, 1, "the stale symbol was counted as removed");

        let all = store.list("repo", &SymbolFilter::default()).await.unwrap();
        assert_eq!(all.len(), 1, "only the new symbol remains");
        assert_eq!(all[0].name, "new");
    }

    #[tokio::test]
    async fn parent_child_resolves_to_real_db_id() {
        let store = IndexStore::open_in_memory().await.unwrap();
        let mut parent = sample_symbol(0, "MyImpl");
        parent.kind = SymbolKind::Impl;
        let mut child = sample_symbol(1, "method");
        child.kind = SymbolKind::Method;
        child.local_parent = Some(0);

        store
            .replace_file_symbols(
                "repo",
                "src/lib.rs",
                Language::Rust,
                "hash1",
                &[parent, child],
            )
            .await
            .unwrap();

        let all = store.list("repo", &SymbolFilter::default()).await.unwrap();
        let parent_row = all.iter().find(|s| s.name == "MyImpl").unwrap();
        let child_row = all.iter().find(|s| s.name == "method").unwrap();
        assert_eq!(child_row.parent_id, Some(parent_row.id));
    }

    #[tokio::test]
    async fn remove_file_deletes_its_symbols() {
        let store = IndexStore::open_in_memory().await.unwrap();
        store
            .replace_file_symbols(
                "repo",
                "src/lib.rs",
                Language::Rust,
                "hash1",
                &[sample_symbol(0, "run")],
            )
            .await
            .unwrap();
        let removed = store.remove_file("repo", "src/lib.rs").await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(store.symbol_count("repo").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn list_filters_by_kind_and_path_glob() {
        let store = IndexStore::open_in_memory().await.unwrap();
        let mut a = sample_symbol(0, "run");
        a.kind = SymbolKind::Fn;
        let mut b = sample_symbol(1, "Runner");
        b.kind = SymbolKind::Struct;
        store
            .replace_file_symbols("repo", "src/lib.rs", Language::Rust, "hash1", &[a, b])
            .await
            .unwrap();

        let fns = store
            .list(
                "repo",
                &SymbolFilter {
                    kind: Some(SymbolKind::Fn),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "run");

        let globbed = store
            .list(
                "repo",
                &SymbolFilter {
                    path_glob: Some("src/*.rs"),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(globbed.len(), 2);

        let no_match = store
            .list(
                "repo",
                &SymbolFilter {
                    path_glob: Some("tests/*.rs"),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(no_match.is_empty());
    }

    #[tokio::test]
    async fn get_by_qualified_name_exact_match_only() {
        let store = IndexStore::open_in_memory().await.unwrap();
        store
            .replace_file_symbols(
                "repo",
                "src/lib.rs",
                Language::Rust,
                "hash1",
                &[sample_symbol(0, "run")],
            )
            .await
            .unwrap();
        assert!(store
            .get_by_qualified_name("repo", "run")
            .await
            .unwrap()
            .is_some());
        assert!(store
            .get_by_qualified_name("repo", "ru")
            .await
            .unwrap()
            .is_none());
    }
}
