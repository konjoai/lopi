//! The four tool operations `lopi-mcp`'s deferred index server wraps:
//! `find`, `read`, `refs`, and the composite `query`. Pure against
//! [`IndexStore`] (+ the repo's working tree for `read`'s actual source
//! text) — no MCP/JSON-RPC concern lives here, so these are unit-testable
//! without a transport.
//!
//! Every envelope carries `truncated`/`total_matches` — an agent that
//! cannot tell it saw 10 of 340 results will confidently reason from 10.

use crate::store::{IndexStore, RefDirection, RefHit, SymbolFilter};
use crate::types::{Language, Symbol, SymbolKind};
use anyhow::{Context, Result};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::Path;

/// A bounded result set that tells the caller what it didn't see.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope<T> {
    /// The (possibly truncated) result items.
    pub items: Vec<T>,
    /// Whether `items` is a prefix of a larger match set.
    pub truncated: bool,
    /// The full match count before truncation.
    pub total_matches: usize,
}

/// One `lopi_find` hit — a pointer, never a body.
#[derive(Debug, Clone, PartialEq)]
pub struct FindHit {
    /// Fully-qualified name.
    pub qualified_name: String,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Repo-relative path.
    pub path: String,
    /// 1-based start line.
    pub line_start: u32,
    /// One-line signature.
    pub signature: String,
    /// First doc-comment line, if any.
    pub doc_first_line: Option<String>,
}

/// Fuzzy name + signature + doc match, ranked, bounded to `limit`. Never
/// returns a body — that's what [`read`] is for.
///
/// # Errors
/// Returns `Err` on a store query failure.
pub async fn find(
    store: &IndexStore,
    repo_id: &str,
    query_text: &str,
    kind: Option<SymbolKind>,
    lang: Option<Language>,
    path_glob: Option<&str>,
    limit: u32,
) -> Result<Envelope<FindHit>> {
    let filter = SymbolFilter {
        kind,
        lang,
        path_glob,
    };
    let candidates = store.list(repo_id, &filter).await?;
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, Symbol)> = candidates
        .into_iter()
        .filter_map(|sym| {
            if query_text.is_empty() {
                return Some((0, sym));
            }
            let haystack = format!(
                "{} {} {}",
                sym.name,
                sym.signature,
                sym.doc_first_line.as_deref().unwrap_or("")
            );
            matcher
                .fuzzy_match(&haystack, query_text)
                .map(|score| (score, sym))
        })
        .collect();
    // Deterministic ordering: score desc, then qualified_name asc as a tiebreak.
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.qualified_name.cmp(&b.1.qualified_name))
    });

    let total_matches = scored.len();
    let limit = limit.max(1) as usize;
    let items = scored
        .into_iter()
        .take(limit)
        .map(|(_, sym)| FindHit {
            qualified_name: sym.qualified_name,
            kind: sym.kind,
            path: sym.path,
            line_start: sym.line_start,
            signature: sym.signature,
            doc_first_line: sym.doc_first_line,
        })
        .collect();
    Ok(Envelope {
        items,
        truncated: total_matches > limit,
        total_matches,
    })
}

/// What `read` should return: either a symbol by qualified name, or an
/// explicit `(path, line_start, line_end)` span.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadTarget {
    /// Resolve via the symbol index.
    QualifiedName(String),
    /// An explicit span — no index lookup needed.
    Span {
        /// Repo-relative path.
        path: String,
        /// 1-based inclusive start line.
        line_start: u32,
        /// 1-based inclusive end line.
        line_end: u32,
    },
}

/// The exact source span `lopi_read` returned, bounded to `max_lines`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadResult {
    /// Repo-relative path.
    pub path: String,
    /// 1-based inclusive start line actually returned.
    pub line_start: u32,
    /// 1-based inclusive end line actually returned.
    pub line_end: u32,
    /// The source text for `[line_start, line_end]`.
    pub text: String,
    /// Whether the span was cut to fit `max_lines` (head + tail, never a
    /// silent cut — see `continue_from`).
    pub truncated: bool,
    /// When `truncated`, the line to resume from with another `lopi_read`
    /// call — the explicit continuation the brief requires instead of a
    /// silent cut.
    pub continue_from: Option<u32>,
}

/// Read the exact source span for `target`, bounded to `max_lines` (head +
/// tail with an explicit elision marker when the span is larger),
/// `context_lines` of padding on each side.
///
/// # Errors
/// Returns `Err` if a qualified-name target doesn't resolve, or the file
/// can't be read.
pub async fn read(
    store: &IndexStore,
    repo_id: &str,
    repo_root: &Path,
    target: ReadTarget,
    context_lines: u32,
    max_lines: u32,
) -> Result<ReadResult> {
    let (path, line_start, line_end) = match target {
        ReadTarget::Span {
            path,
            line_start,
            line_end,
        } => (path, line_start, line_end),
        ReadTarget::QualifiedName(qname) => {
            let sym = store
                .get_by_qualified_name(repo_id, &qname)
                .await?
                .with_context(|| format!("no symbol named `{qname}` in the index"))?;
            (sym.path, sym.line_start, sym.line_end)
        }
    };

    let contents = std::fs::read_to_string(repo_root.join(&path))
        .with_context(|| format!("reading {path}"))?;
    let lines: Vec<&str> = contents.lines().collect();
    let total = lines.len() as u32;
    let start = line_start.saturating_sub(context_lines).max(1);
    let end = (line_end + context_lines).min(total.max(1));

    let span_len = end.saturating_sub(start) + 1;
    if span_len <= max_lines || max_lines == 0 {
        let text = slice_lines(&lines, start, end);
        return Ok(ReadResult {
            path,
            line_start: start,
            line_end: end,
            text,
            truncated: false,
            continue_from: None,
        });
    }

    let head_len = max_lines / 2;
    let tail_len = max_lines - head_len;
    let head_end = start + head_len.saturating_sub(1);
    let tail_start = end.saturating_sub(tail_len) + 1;
    let head = slice_lines(&lines, start, head_end);
    let tail = slice_lines(&lines, tail_start, end);
    let text = format!(
        "{head}\n… [elided lines {}-{}; call lopi_read again with line_start={} to continue] …\n{tail}",
        head_end + 1,
        tail_start.saturating_sub(1),
        head_end + 1,
    );
    Ok(ReadResult {
        path,
        line_start: start,
        line_end: end,
        text,
        truncated: true,
        continue_from: Some(head_end + 1),
    })
}

fn slice_lines(lines: &[&str], start: u32, end: u32) -> String {
    let s = start.saturating_sub(1) as usize;
    let e = (end as usize).min(lines.len());
    if s >= e {
        return String::new();
    }
    lines[s..e].join("\n")
}

/// Callers/callees of `qualified_name`, up to `depth` hops. Depth is the
/// caller's responsibility to clamp to [`crate::IndexConfig::refs_depth`]'s
/// hard cap of 3 — this function trusts what it's given, since the config
/// clamp already lives at the one call site that reads it from `lopi.toml`.
///
/// # Errors
/// Returns `Err` if `qualified_name` doesn't resolve to a symbol, or on a
/// store query failure.
pub async fn refs(
    store: &IndexStore,
    repo_id: &str,
    qualified_name: &str,
    direction: RefDirection,
    depth: u32,
    limit: u32,
) -> Result<Envelope<RefHit>> {
    let sym = store
        .get_by_qualified_name(repo_id, qualified_name)
        .await?
        .with_context(|| format!("no symbol named `{qualified_name}` in the index"))?;
    let hits = match direction {
        RefDirection::Callers => store.callers(repo_id, sym.id, depth).await?,
        RefDirection::Callees => store.callees(repo_id, sym.id, depth).await?,
    };
    let total_matches = hits.len();
    let limit = limit.max(1) as usize;
    let items: Vec<RefHit> = hits.into_iter().take(limit).collect();
    Ok(Envelope {
        truncated: total_matches > limit,
        total_matches,
        items,
    })
}

/// One composite `lopi_query` request: a `find`, optionally followed by a
/// `refs` expansion of each hit. Deliberately a fixed struct rather than a
/// query language, per the brief.
#[derive(Debug, Clone, PartialEq)]
pub struct QuerySpec {
    /// Free-text query for the initial `find`.
    pub find_text: String,
    /// Optional kind filter for the initial `find`.
    pub kind: Option<SymbolKind>,
    /// Optional language filter for the initial `find`.
    pub lang: Option<Language>,
    /// Optional path glob filter for the initial `find`.
    pub path_glob: Option<String>,
    /// Max `find` hits to expand.
    pub limit: u32,
    /// When set, each `find` hit is expanded with a `refs` call in this direction.
    pub then_refs: Option<RefDirection>,
    /// `then_refs`'s traversal depth (ignored when `then_refs` is `None`).
    pub refs_depth: u32,
}

/// One composite result row: a `find` hit plus its (optional) `refs` expansion.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryRow {
    /// The `find` hit.
    pub symbol: FindHit,
    /// Its `refs` expansion, when `QuerySpec::then_refs` was set.
    pub refs: Option<Envelope<RefHit>>,
}

/// Run a composite `find` (+ optional `refs` per hit) in one call — the
/// tool that replaces a find-then-read-then-refs round-trip chain. Each
/// avoided round trip is an avoided inference pass, not just avoided tokens.
///
/// # Errors
/// Returns `Err` on a store query failure.
pub async fn composite_query(
    store: &IndexStore,
    repo_id: &str,
    spec: &QuerySpec,
) -> Result<Envelope<QueryRow>> {
    let found = find(
        store,
        repo_id,
        &spec.find_text,
        spec.kind,
        spec.lang,
        spec.path_glob.as_deref(),
        spec.limit,
    )
    .await?;

    let mut rows = Vec::with_capacity(found.items.len());
    for hit in found.items {
        let refs = match spec.then_refs {
            Some(direction) => Some(
                refs(
                    store,
                    repo_id,
                    &hit.qualified_name,
                    direction,
                    spec.refs_depth,
                    spec.limit,
                )
                .await?,
            ),
            None => None,
        };
        rows.push(QueryRow { symbol: hit, refs });
    }

    Ok(Envelope {
        truncated: found.truncated,
        total_matches: found.total_matches,
        items: rows,
    })
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
