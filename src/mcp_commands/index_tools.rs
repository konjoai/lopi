//! `lopi mcp-index-serve` — the Finding #4 symbol-index tool set
//! (`lopi_find`/`lopi_read`/`lopi_refs`/`lopi_query`) over stdio, kept
//! deliberately separate from `mcp-serve`'s curated task-management tools.
//!
//! **Why a separate server, not four more entries in `mcp-serve`'s
//! `tool_defs()`.** `mcp-serve`'s own doc comment already states the
//! discipline this sprint's brief calls "deferred": *"every additional
//! tool is context budget spent on every turn a plugin user has
//! installed."* Folding these four in would mean every `mcp-serve` session
//! (including ones that never touch code navigation) pays their schema
//! tokens. A caller that wants symbol navigation — a human's Claude Code
//! session, or lopi's own spawned worker via `--mcp-config` when
//! `context.mode = "index"` — opts into *this* server specifically, and one
//! that doesn't never sees these schemas at all. True per-tool schema
//! deferral (a client fetching a stub now, a full schema only when it
//! decides to call the tool) is a client-side MCP behavior this codebase
//! doesn't control — see `LEDGER.md`'s Finding #4 entry for the full
//! reasoning. This module's contribution is keeping the four schemas out of
//! `--allowedTools`/the system prompt entirely, reachable only through a
//! connection a session must explicitly make.

use anyhow::{Context, Result};
use lopi_index::query::{self, QuerySpec, ReadTarget};
use lopi_index::{IndexConfig, IndexStore, Language, RefDirection, SymbolKind};
use lopi_mcp::McpTool;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The tool handler backing `lopi mcp-index-serve`.
pub(super) struct IndexToolHandler {
    store: IndexStore,
    repo_root: PathBuf,
    repo_id: String,
    cfg: IndexConfig,
}

impl IndexToolHandler {
    pub(super) fn new(store: IndexStore, repo_root: PathBuf, repo_id: String, cfg: IndexConfig) -> Self {
        Self { store, repo_root, repo_id, cfg }
    }
}

impl lopi_mcp::ToolHandler for IndexToolHandler {
    fn tools(&self) -> Vec<McpTool> {
        tool_defs()
    }

    fn call(&self, name: &str, arguments: Value) -> impl std::future::Future<Output = Result<String>> + Send {
        let store = self.store.clone();
        let repo_root = self.repo_root.clone();
        let repo_id = self.repo_id.clone();
        let cfg = self.cfg.clone();
        let name = name.to_string();
        async move { dispatch(&store, &repo_root, &repo_id, &cfg, &name, arguments).await }
    }
}

fn tool_defs() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "lopi_find".into(),
            description: "Fuzzy name/signature/doc search over the repo's symbol index. Returns pointers (qualified_name, path, line), never bodies.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Free-text query." },
                    "kind": { "type": "string", "enum": kind_names(), "description": "Restrict to one symbol kind." },
                    "lang": { "type": "string", "enum": lang_names(), "description": "Restrict to one language." },
                    "path_glob": { "type": "string", "description": "SQLite GLOB pattern to restrict paths." },
                    "limit": { "type": "integer", "description": "Max results (default 50)." },
                },
                "required": ["query"],
            }),
            meta: None,
        },
        McpTool {
            name: "lopi_read".into(),
            description: "Read the exact source span for a symbol (by qualified_name) or an explicit path/line range. The only index tool that returns code, bounded by config.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "qualified_name": { "type": "string", "description": "Read this symbol's span. Mutually exclusive with path/line_start/line_end." },
                    "path": { "type": "string" },
                    "line_start": { "type": "integer" },
                    "line_end": { "type": "integer" },
                    "context_lines": { "type": "integer", "description": "Padding lines on each side (default 0)." },
                },
            }),
            meta: None,
        },
        McpTool {
            name: "lopi_refs".into(),
            description: "Callers or callees of a symbol, up to depth 3 — replaces a grep-and-read spiral.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "qualified_name": { "type": "string" },
                    "direction": { "type": "string", "enum": ["callers", "callees"] },
                    "depth": { "type": "integer", "description": "Traversal depth, hard-capped at 3." },
                },
                "required": ["qualified_name", "direction"],
            }),
            meta: None,
        },
        McpTool {
            name: "lopi_query".into(),
            description: "Composite find + optional refs expansion in one call — avoids a find-then-read-then-refs round trip chain.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "find": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" },
                            "kind": { "type": "string", "enum": kind_names() },
                            "lang": { "type": "string", "enum": lang_names() },
                            "path_glob": { "type": "string" },
                            "limit": { "type": "integer" },
                        },
                        "required": ["query"],
                    },
                    "then_refs": {
                        "type": "object",
                        "properties": {
                            "direction": { "type": "string", "enum": ["callers", "callees"] },
                            "depth": { "type": "integer" },
                        },
                        "required": ["direction"],
                    },
                },
                "required": ["find"],
            }),
            meta: None,
        },
    ]
}

fn kind_names() -> Vec<&'static str> {
    ["fn", "method", "struct", "enum", "trait", "impl", "class", "const", "type", "module"].to_vec()
}

fn lang_names() -> Vec<&'static str> {
    ["rust", "typescript", "javascript", "python", "go"].to_vec()
}

async fn dispatch(
    store: &IndexStore,
    repo_root: &std::path::Path,
    repo_id: &str,
    cfg: &IndexConfig,
    name: &str,
    args: Value,
) -> Result<String> {
    let result = match name {
        "lopi_find" => call_find(store, repo_id, cfg, &args).await?,
        "lopi_read" => call_read(store, repo_root, repo_id, cfg, &args).await?,
        "lopi_refs" => call_refs(store, repo_id, cfg, &args).await?,
        "lopi_query" => call_query(store, repo_id, cfg, &args).await?,
        other => anyhow::bail!("unknown tool: {other}"),
    };
    Ok(result.to_string())
}

async fn call_find(store: &IndexStore, repo_id: &str, cfg: &IndexConfig, args: &Value) -> Result<Value> {
    let query_text = args.get("query").and_then(Value::as_str).unwrap_or("");
    let kind = args.get("kind").and_then(Value::as_str).and_then(SymbolKind::parse);
    let lang = args.get("lang").and_then(Value::as_str).and_then(Language::parse);
    let path_glob = args.get("path_glob").and_then(Value::as_str);
    let limit = args.get("limit").and_then(Value::as_u64).map_or(cfg.max_results, |n| n as u32);

    let env = query::find(store, repo_id, query_text, kind, lang, path_glob, limit).await?;
    Ok(json!({
        "truncated": env.truncated,
        "total_matches": env.total_matches,
        "items": env.items.iter().map(|h| json!({
            "qualified_name": h.qualified_name,
            "kind": h.kind.as_str(),
            "path": h.path,
            "line_start": h.line_start,
            "signature": h.signature,
            "doc_first_line": h.doc_first_line,
        })).collect::<Vec<_>>(),
    }))
}

async fn call_read(
    store: &IndexStore,
    repo_root: &std::path::Path,
    repo_id: &str,
    cfg: &IndexConfig,
    args: &Value,
) -> Result<Value> {
    let target = if let Some(qname) = args.get("qualified_name").and_then(Value::as_str) {
        ReadTarget::QualifiedName(qname.to_string())
    } else {
        let path = args.get("path").and_then(Value::as_str).context("missing path")?;
        let line_start = args.get("line_start").and_then(Value::as_u64).context("missing line_start")? as u32;
        let line_end = args.get("line_end").and_then(Value::as_u64).context("missing line_end")? as u32;
        ReadTarget::Span { path: path.to_string(), line_start, line_end }
    };
    let context_lines = args.get("context_lines").and_then(Value::as_u64).unwrap_or(0) as u32;

    let result = query::read(store, repo_id, repo_root, target, context_lines, cfg.max_read_lines).await?;
    Ok(json!({
        "path": result.path,
        "line_start": result.line_start,
        "line_end": result.line_end,
        "text": result.text,
        "truncated": result.truncated,
        "continue_from": result.continue_from,
    }))
}

async fn call_refs(store: &IndexStore, repo_id: &str, cfg: &IndexConfig, args: &Value) -> Result<Value> {
    let qname = args.get("qualified_name").and_then(Value::as_str).context("missing qualified_name")?;
    let direction = match args.get("direction").and_then(Value::as_str) {
        Some("callers") => RefDirection::Callers,
        Some("callees") => RefDirection::Callees,
        _ => anyhow::bail!("direction must be \"callers\" or \"callees\""),
    };
    let depth = args
        .get("depth")
        .and_then(Value::as_u64)
        .map_or(cfg.refs_depth(), |d| (d as u32).min(3));

    let env = query::refs(store, repo_id, qname, direction, depth, cfg.max_results).await?;
    Ok(json!({
        "truncated": env.truncated,
        "total_matches": env.total_matches,
        "items": env.items.iter().map(|h| json!({
            "qualified_name": h.qualified_name,
            "path": h.path,
            "line": h.line,
        })).collect::<Vec<_>>(),
    }))
}

async fn call_query(store: &IndexStore, repo_id: &str, cfg: &IndexConfig, args: &Value) -> Result<Value> {
    let find = args.get("find").context("missing find")?;
    let find_text = find.get("query").and_then(Value::as_str).unwrap_or("").to_string();
    let kind = find.get("kind").and_then(Value::as_str).and_then(SymbolKind::parse);
    let lang = find.get("lang").and_then(Value::as_str).and_then(Language::parse);
    let path_glob = find.get("path_glob").and_then(Value::as_str).map(str::to_string);
    let limit = find.get("limit").and_then(Value::as_u64).map_or(cfg.max_results, |n| n as u32);

    let (then_refs, refs_depth) = match args.get("then_refs") {
        Some(tr) => {
            let dir = match tr.get("direction").and_then(Value::as_str) {
                Some("callers") => RefDirection::Callers,
                Some("callees") => RefDirection::Callees,
                _ => anyhow::bail!("then_refs.direction must be \"callers\" or \"callees\""),
            };
            let depth = tr.get("depth").and_then(Value::as_u64).map_or(cfg.refs_depth(), |d| (d as u32).min(3));
            (Some(dir), depth)
        }
        None => (None, 0),
    };

    let spec = QuerySpec { find_text, kind, lang, path_glob, limit, then_refs, refs_depth };
    let env = query::composite_query(store, repo_id, &spec).await?;
    Ok(json!({
        "truncated": env.truncated,
        "total_matches": env.total_matches,
        "items": env.items.iter().map(|row| json!({
            "symbol": {
                "qualified_name": row.symbol.qualified_name,
                "kind": row.symbol.kind.as_str(),
                "path": row.symbol.path,
                "line_start": row.symbol.line_start,
                "signature": row.symbol.signature,
                "doc_first_line": row.symbol.doc_first_line,
            },
            "refs": row.refs.as_ref().map(|r| json!({
                "truncated": r.truncated,
                "total_matches": r.total_matches,
                "items": r.items.iter().map(|h| json!({
                    "qualified_name": h.qualified_name,
                    "path": h.path,
                    "line": h.line,
                })).collect::<Vec<_>>(),
            })),
        })).collect::<Vec<_>>(),
    }))
}
