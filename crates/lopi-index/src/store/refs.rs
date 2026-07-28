//! Ref table CRUD: file-scoped insert, a repo-wide best-effort resolution
//! pass, and the `callers`/`callees` traversal `lopi_refs` is built on.

use crate::types::NewRef;
use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};

/// One raw `callees` row: `(path, line, to_name, to_symbol_id, resolved_qualified_name)`.
type CalleeRow = (String, u32, String, Option<i64>, Option<String>);

/// One `lopi_refs` result row — see [`super::IndexStore::callers`]/
/// [`super::IndexStore::callees`] for what `qualified_name` means per direction.
#[derive(Debug, Clone, PartialEq)]
pub struct RefHit {
    /// The other end of the edge: the caller's owning symbol (callers
    /// direction) or the resolved callee (callees direction; falls back to
    /// the raw unresolved call-site text, prefixed `"?"`, when resolution failed).
    pub qualified_name: String,
    /// The reference's own location (the call site), not the target's.
    pub path: String,
    /// The reference's own line.
    pub line: u32,
}

/// Traversal direction for [`super::IndexStore::callers`]/`callees`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefDirection {
    /// Who calls the given symbol.
    Callers,
    /// What the given symbol calls.
    Callees,
}

impl super::IndexStore {
    /// Insert `refs` for `path`, resolving each `from_local_id` to the real
    /// database id via `local_to_db` (as produced by
    /// [`super::IndexStore::replace_file_symbols`] for the same file).
    /// `to_symbol_id` is left `NULL` — cross-file resolution happens
    /// repo-wide in [`Self::resolve_refs`], once every changed file in the
    /// pass has been (re)inserted.
    ///
    /// # Errors
    /// Returns `Err` on a write failure.
    pub async fn insert_file_refs(
        &self,
        repo_id: &str,
        path: &str,
        refs: &[NewRef],
        local_to_db: &HashMap<usize, i64>,
    ) -> Result<usize> {
        let mut tx = self.write_pool().begin().await?;
        for r in refs {
            let from_id = r.from_local_id.and_then(|lid| local_to_db.get(&lid).copied());
            sqlx::query(
                "INSERT INTO refs (repo_id, from_symbol_id, to_name, to_symbol_id, path, line)
                 VALUES (?, ?, ?, NULL, ?, ?)",
            )
            .bind(repo_id)
            .bind(from_id)
            .bind(&r.to_name)
            .bind(path)
            .bind(r.line)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(refs.len())
    }

    /// Best-effort resolution over *every* currently-unresolved ref in the
    /// repo. Correct but expensive on a large, mature index — most refs
    /// left unresolved after their first pass (calls into external crates,
    /// the standard library, or a genuinely ambiguous same-named method)
    /// stay unresolved forever, so re-scanning the whole backlog on every
    /// incremental reindex doesn't scale. [`Self::resolve_refs_for`] is the
    /// scoped, hot-path equivalent `reindex.rs` actually calls; this
    /// unscoped version remains for a manual full pass (`lopi index
    /// --full-resolve`, once that flag exists) and as the most
    /// straightforward correctness baseline for tests.
    ///
    /// # Errors
    /// Returns `Err` on a query or write failure.
    pub async fn resolve_refs(&self, repo_id: &str) -> Result<usize> {
        let unresolved: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, to_name FROM refs WHERE repo_id = ? AND to_symbol_id IS NULL",
        )
        .bind(repo_id)
        .fetch_all(self.read_pool())
        .await?;
        self.resolve_given(repo_id, unresolved).await
    }

    /// Scoped best-effort resolution — the hot-path counterpart to
    /// [`Self::resolve_refs`], covering exactly the two ways a ref can
    /// become newly resolvable in one reindex pass without re-scanning the
    /// whole repo's unresolved backlog:
    ///
    ///  1. A fresh, never-yet-checked ref from a file this pass touched
    ///     (`path` in `touched_paths`).
    ///  2. A pre-existing unresolved ref whose `to_name` matches a symbol
    ///     this pass just added (`to_name` in `new_symbol_names`) — the
    ///     "the target showed up later" case.
    ///
    /// Anything outside those two sets could not have changed resolvability
    /// this pass, so skipping it is a wall-time optimization, not an
    /// approximation — nothing correctly resolvable is missed. See
    /// `LEDGER.md`'s Finding #4 entry for the measured before/after.
    ///
    /// # Errors
    /// Returns `Err` on a query or write failure.
    pub async fn resolve_refs_for(
        &self,
        repo_id: &str,
        touched_paths: &[String],
        new_symbol_names: &[String],
    ) -> Result<usize> {
        if touched_paths.is_empty() && new_symbol_names.is_empty() {
            return Ok(0);
        }
        let path_ph = std::iter::repeat_n("?", touched_paths.len()).collect::<Vec<_>>().join(", ");
        let name_ph = std::iter::repeat_n("?", new_symbol_names.len()).collect::<Vec<_>>().join(", ");
        let mut clauses = Vec::new();
        if !touched_paths.is_empty() {
            clauses.push(format!("path IN ({path_ph})"));
        }
        if !new_symbol_names.is_empty() {
            clauses.push(format!("to_name IN ({name_ph})"));
        }
        let sql = format!(
            "SELECT id, to_name FROM refs WHERE repo_id = ? AND to_symbol_id IS NULL AND ({})",
            clauses.join(" OR ")
        );
        let mut q = sqlx::query_as::<_, (i64, String)>(&sql).bind(repo_id);
        for p in touched_paths {
            q = q.bind(p);
        }
        for n in new_symbol_names {
            q = q.bind(n);
        }
        let unresolved = q.fetch_all(self.read_pool()).await?;
        self.resolve_given(repo_id, unresolved).await
    }

    /// Shared tail of [`Self::resolve_refs`]/[`Self::resolve_refs_for`]:
    /// given a candidate `unresolved` set, look up matching symbols (only
    /// for the distinct names actually present — never the whole `symbols`
    /// table) and update every uniquely-resolvable one.
    async fn resolve_given(&self, repo_id: &str, unresolved: Vec<(i64, String)>) -> Result<usize> {
        if unresolved.is_empty() {
            return Ok(0);
        }
        let distinct_names: Vec<&str> = {
            let mut seen = HashSet::new();
            unresolved
                .iter()
                .filter(|(_, name)| seen.insert(name.as_str()))
                .map(|(_, name)| name.as_str())
                .collect()
        };
        let placeholders = std::iter::repeat_n("?", distinct_names.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT name, id FROM symbols WHERE repo_id = ? AND name IN ({placeholders})");
        let mut q = sqlx::query_as::<_, (String, i64)>(&sql).bind(repo_id);
        for name in &distinct_names {
            q = q.bind(*name);
        }
        let candidates = q.fetch_all(self.read_pool()).await?;

        let mut by_name: HashMap<String, Option<i64>> = HashMap::new();
        for (name, id) in candidates {
            by_name
                .entry(name)
                .and_modify(|slot| *slot = None) // second sighting: ambiguous
                .or_insert(Some(id));
        }

        let mut resolved_count = 0usize;
        let mut tx = self.write_pool().begin().await?;
        for (ref_id, to_name) in unresolved {
            if let Some(Some(sym_id)) = by_name.get(&to_name) {
                sqlx::query("UPDATE refs SET to_symbol_id = ? WHERE id = ?")
                    .bind(sym_id)
                    .bind(ref_id)
                    .execute(&mut *tx)
                    .await?;
                resolved_count += 1;
            }
        }
        tx.commit().await?;
        Ok(resolved_count)
    }

    /// Symbols that call `symbol_id`, breadth-first up to `depth` hops
    /// (already clamped to the brief's cap of 3 by [`crate::IndexConfig::refs_depth`]).
    ///
    /// # Errors
    /// Returns `Err` on a query failure.
    pub async fn callers(&self, repo_id: &str, symbol_id: i64, depth: u32) -> Result<Vec<RefHit>> {
        self.traverse(repo_id, symbol_id, depth, RefDirection::Callers)
            .await
    }

    /// Symbols `symbol_id` calls, breadth-first up to `depth` hops.
    ///
    /// # Errors
    /// Returns `Err` on a query failure.
    pub async fn callees(&self, repo_id: &str, symbol_id: i64, depth: u32) -> Result<Vec<RefHit>> {
        self.traverse(repo_id, symbol_id, depth, RefDirection::Callees)
            .await
    }

    async fn traverse(
        &self,
        repo_id: &str,
        root: i64,
        depth: u32,
        direction: RefDirection,
    ) -> Result<Vec<RefHit>> {
        let mut hits = Vec::new();
        let mut seen_edges: HashSet<(String, String, u32)> = HashSet::new();
        let mut visited_symbols: HashSet<i64> = HashSet::from([root]);
        let mut frontier: VecDeque<(i64, u32)> = VecDeque::from([(root, 0)]);

        while let Some((sym_id, hop)) = frontier.pop_front() {
            if hop >= depth {
                continue;
            }
            let (edge_hits, next_ids) = match direction {
                RefDirection::Callers => self.one_hop_callers(repo_id, sym_id).await?,
                RefDirection::Callees => self.one_hop_callees(repo_id, sym_id).await?,
            };
            for hit in edge_hits {
                let key = (hit.qualified_name.clone(), hit.path.clone(), hit.line);
                if seen_edges.insert(key) {
                    hits.push(hit);
                }
            }
            for next in next_ids {
                if visited_symbols.insert(next) {
                    frontier.push_back((next, hop + 1));
                }
            }
        }
        hits.sort_by(|a, b| {
            (a.qualified_name.as_str(), a.path.as_str(), a.line).cmp(&(
                b.qualified_name.as_str(),
                b.path.as_str(),
                b.line,
            ))
        });
        Ok(hits)
    }

    async fn one_hop_callers(
        &self,
        repo_id: &str,
        symbol_id: i64,
    ) -> Result<(Vec<RefHit>, Vec<i64>)> {
        let rows: Vec<(String, u32, Option<i64>, Option<String>)> = sqlx::query_as(
            "SELECT r.path, r.line, s.id, s.qualified_name
             FROM refs r LEFT JOIN symbols s ON s.id = r.from_symbol_id
             WHERE r.repo_id = ? AND r.to_symbol_id = ?
             ORDER BY r.path, r.line",
        )
        .bind(repo_id)
        .bind(symbol_id)
        .fetch_all(self.read_pool())
        .await?;
        let mut hits = Vec::with_capacity(rows.len());
        let mut next = Vec::new();
        for (path, line, from_id, from_qname) in rows {
            hits.push(RefHit {
                qualified_name: from_qname.unwrap_or_else(|| "<file scope>".into()),
                path,
                line,
            });
            if let Some(id) = from_id {
                next.push(id);
            }
        }
        Ok((hits, next))
    }

    async fn one_hop_callees(
        &self,
        repo_id: &str,
        symbol_id: i64,
    ) -> Result<(Vec<RefHit>, Vec<i64>)> {
        let rows: Vec<CalleeRow> = sqlx::query_as(
            "SELECT r.path, r.line, r.to_name, r.to_symbol_id, s.qualified_name
             FROM refs r LEFT JOIN symbols s ON s.id = r.to_symbol_id
             WHERE r.repo_id = ? AND r.from_symbol_id = ?
             ORDER BY r.path, r.line",
        )
        .bind(repo_id)
        .bind(symbol_id)
        .fetch_all(self.read_pool())
        .await?;
        let mut hits = Vec::with_capacity(rows.len());
        let mut next = Vec::new();
        for (path, line, to_name, to_id, to_qname) in rows {
            hits.push(RefHit {
                qualified_name: to_qname.unwrap_or_else(|| format!("?{to_name}")),
                path,
                line,
            });
            if let Some(id) = to_id {
                next.push(id);
            }
        }
        Ok((hits, next))
    }

    /// Total ref rows for `repo_id` — test/measurement helper.
    ///
    /// # Errors
    /// Returns `Err` on a query failure.
    pub async fn ref_count(&self, repo_id: &str) -> Result<usize> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM refs WHERE repo_id = ?")
            .bind(repo_id)
            .fetch_one(self.read_pool())
            .await?;
        Ok(row.0 as usize)
    }
}

#[cfg(test)]
#[path = "refs_tests.rs"]
mod tests;
