//! `RepoMap::build` — a deterministic, token-budgeted orientation document
//! for the planning prompt. Byte-identical for the same commit: no
//! `HashMap` iteration order, no timestamps, no absolute paths, everything
//! sorted before it's rendered. This is the contract Sprint C's
//! `PrefixBuilder` (not yet built in this codebase — see `LEDGER.md`) will
//! need once it exists; until then, `RepoMap::build`'s output is inserted
//! directly into the planning prompt by `lopi-agent`'s `runner/seed.rs`.

use crate::store::{IndexStore, SymbolFilter};
use crate::types::Symbol;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

/// A built repo map — the rendered text plus whether the token budget forced
/// a truncation, so a caller can log/measure that separately from the text.
#[derive(Debug, Clone, PartialEq)]
pub struct RepoMap {
    /// The rendered map text.
    pub text: String,
    /// Whether any section was truncated to fit `budget_tokens`.
    pub truncated: bool,
}

/// Crude but stable token estimate: ~4 bytes/token, the same rule of thumb
/// used elsewhere in this codebase's cost accounting. Good enough for a
/// budget knob — this crate never bills against it, it only decides how
/// much to show.
fn estimate_tokens(s: &str) -> u32 {
    (s.len() as u32).div_ceil(4)
}

impl RepoMap {
    /// Build the repo map for `repo_id`, sourcing symbols from `store` and
    /// the directory skeleton by walking `repo_root`. `build_cmds` are the
    /// build/test/lint commands to surface verbatim (sourced by the caller
    /// from the repo's gate config — this module doesn't know about
    /// `.lopi/loop.toml`, keeping `lopi-index` decoupled from `lopi-core`).
    ///
    /// # Errors
    /// Returns `Err` on a store query failure.
    pub async fn build(
        store: &IndexStore,
        repo_id: &str,
        repo_root: &Path,
        skeleton_depth: u32,
        top_referenced: u32,
        build_cmds: &[(&str, &str)],
        budget_tokens: u32,
    ) -> Result<Self> {
        let skeleton = build_skeleton(repo_root, skeleton_depth);
        let public_surface = build_public_surface(store, repo_id).await?;
        let most_referenced = store.most_referenced(repo_id, top_referenced).await?;

        let sections = vec![
            ("Directory skeleton".to_string(), skeleton, false),
            (
                "Public surface by module".to_string(),
                render_public_surface(&public_surface),
                true, // droppable from the bottom under budget pressure
            ),
            (
                "Most-referenced symbols".to_string(),
                render_most_referenced(&most_referenced),
                true,
            ),
            (
                "Build / test / lint commands".to_string(),
                render_commands(build_cmds),
                false,
            ),
        ];

        Ok(render_budgeted(&sections, budget_tokens))
    }
}

/// Render sections in order, dropping droppable sections from the bottom of
/// the list (never mid-item) once the running total exceeds `budget_tokens`.
/// A dropped section is named in an explicit `[map truncated: ...]` line —
/// an agent that knows the map is partial goes and looks; one that thinks it
/// saw everything won't.
fn render_budgeted(sections: &[(String, String, bool)], budget_tokens: u32) -> RepoMap {
    let mut out = String::new();
    let mut used = 0u32;
    let mut dropped = Vec::new();

    for (i, (title, body, droppable)) in sections.iter().enumerate() {
        if body.is_empty() {
            continue;
        }
        let block = format!("## {title}\n{body}\n\n");
        let cost = estimate_tokens(&block);
        if *droppable && used + cost > budget_tokens && i > 0 {
            dropped.push(title.clone());
            continue;
        }
        out.push_str(&block);
        used += cost;
    }

    let truncated = !dropped.is_empty();
    if truncated {
        out.push_str(&format!(
            "[map truncated: dropped {} of {} sections for budget: {}]\n",
            dropped.len(),
            sections.len(),
            dropped.join(", ")
        ));
    }
    RepoMap {
        text: out,
        truncated,
    }
}

/// Directory skeleton to `depth`, file counts only past the cap — never
/// individual file names, per the brief. Sorted, no absolute paths.
fn build_skeleton(repo_root: &Path, depth: u32) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    walk_for_skeleton(repo_root, repo_root, 0, depth, &mut counts);
    counts
        .into_iter()
        .map(|(dir, count)| format!("{dir} ({count} files)"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn walk_for_skeleton(
    root: &Path,
    dir: &Path,
    level: u32,
    max_depth: u32,
    counts: &mut BTreeMap<String, usize>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    let key = if rel.as_os_str().is_empty() {
        ".".to_string()
    } else {
        rel.to_string_lossy().replace('\\', "/")
    };
    let mut file_count = 0usize;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') || name_str == "target" || name_str == "node_modules" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else {
            file_count += 1;
        }
    }
    if file_count > 0 {
        counts.insert(key, file_count);
    }
    if level < max_depth {
        subdirs.sort();
        for sub in subdirs {
            walk_for_skeleton(root, &sub, level + 1, max_depth, counts);
        }
    }
}

/// Per top-level module: public-surface symbols, signatures + doc first
/// lines only, grouped by the qualified name's first path segment.
async fn build_public_surface(
    store: &IndexStore,
    repo_id: &str,
) -> Result<BTreeMap<String, Vec<Symbol>>> {
    let all = store
        .list(
            repo_id,
            &SymbolFilter {
                ..Default::default()
            },
        )
        .await?;
    let mut by_module: BTreeMap<String, Vec<Symbol>> = BTreeMap::new();
    for sym in all {
        if !sym.is_public || sym.kind.is_container() {
            continue;
        }
        let module = top_level_module(&sym.path);
        by_module.entry(module).or_default().push(sym);
    }
    for symbols in by_module.values_mut() {
        symbols.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
    }
    Ok(by_module)
}

fn top_level_module(path: &str) -> String {
    path.split('/').next().unwrap_or(path).to_string()
}

fn render_public_surface(by_module: &BTreeMap<String, Vec<Symbol>>) -> String {
    by_module
        .iter()
        .map(|(module, symbols)| {
            let lines: Vec<String> = symbols
                .iter()
                .map(|s| match &s.doc_first_line {
                    Some(doc) => format!("  {} — {doc}", s.signature),
                    None => format!("  {}", s.signature),
                })
                .collect();
            format!("### {module}\n{}", lines.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_most_referenced(entries: &[(Symbol, i64)]) -> String {
    entries
        .iter()
        .map(|(sym, count)| {
            format!(
                "{} ({count} inbound refs) — {}",
                sym.qualified_name, sym.path
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_commands(cmds: &[(&str, &str)]) -> String {
    cmds.iter()
        .map(|(label, cmd)| format!("{label}: `{cmd}`"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::RepoMap;
    use crate::store::IndexStore;
    use crate::types::{Language, NewSymbol, SymbolKind};

    fn pub_fn(local_id: usize, name: &str) -> NewSymbol {
        NewSymbol {
            local_id,
            local_parent: None,
            lang: Language::Rust,
            kind: SymbolKind::Fn,
            name: name.into(),
            qualified_name: name.into(),
            signature: format!("pub fn {name}()"),
            doc_first_line: Some(format!("Does {name}.")),
            line_start: 1,
            line_end: 3,
            byte_start: 0,
            byte_end: 10,
            is_public: true,
        }
    }

    #[tokio::test]
    async fn build_twice_is_byte_identical() {
        let store = IndexStore::open_in_memory().await.unwrap();
        store
            .replace_file_symbols(
                "repo",
                "crates/a/lib.rs",
                Language::Rust,
                "h1",
                &[pub_fn(0, "one")],
            )
            .await
            .unwrap();
        store
            .replace_file_symbols(
                "repo",
                "crates/b/lib.rs",
                Language::Rust,
                "h2",
                &[pub_fn(0, "two")],
            )
            .await
            .unwrap();

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("crates/a")).unwrap();
        std::fs::write(dir.path().join("crates/a/lib.rs"), "pub fn one() {}\n").unwrap();

        let cmds = [("build", "cargo build"), ("test", "cargo test")];
        let a = RepoMap::build(&store, "repo", dir.path(), 3, 15, &cmds, 2_500)
            .await
            .unwrap();
        let b = RepoMap::build(&store, "repo", dir.path(), 3, 15, &cmds, 2_500)
            .await
            .unwrap();
        assert_eq!(
            a.text, b.text,
            "same commit must produce byte-identical output"
        );
    }

    #[tokio::test]
    async fn map_contains_no_absolute_paths_or_bodies() {
        let store = IndexStore::open_in_memory().await.unwrap();
        store
            .replace_file_symbols(
                "repo",
                "crates/a/lib.rs",
                Language::Rust,
                "h1",
                &[pub_fn(0, "one")],
            )
            .await
            .unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("crates/a")).unwrap();
        std::fs::write(dir.path().join("crates/a/lib.rs"), "pub fn one() {}\n").unwrap();

        let map = RepoMap::build(&store, "repo", dir.path(), 3, 15, &[], 2_500)
            .await
            .unwrap();
        let abs = dir.path().to_string_lossy().to_string();
        assert!(!map.text.contains(&abs), "no absolute paths in the map");
        assert!(map.text.contains("pub fn one()"));
        assert!(!map.text.contains("{}"), "signature only, never a body");
    }

    #[tokio::test]
    async fn tight_budget_truncates_and_says_so() {
        let store = IndexStore::open_in_memory().await.unwrap();
        for i in 0..50 {
            store
                .replace_file_symbols(
                    "repo",
                    &format!("crates/m{i}/lib.rs"),
                    Language::Rust,
                    "h",
                    &[pub_fn(0, &format!("fn_{i}"))],
                )
                .await
                .unwrap();
        }
        let dir = tempfile::TempDir::new().unwrap();
        let map = RepoMap::build(&store, "repo", dir.path(), 3, 15, &[], 20)
            .await
            .unwrap();
        assert!(map.truncated);
        assert!(map.text.contains("[map truncated:"));
    }

    #[tokio::test]
    async fn generous_budget_is_not_truncated() {
        let store = IndexStore::open_in_memory().await.unwrap();
        store
            .replace_file_symbols(
                "repo",
                "crates/a/lib.rs",
                Language::Rust,
                "h1",
                &[pub_fn(0, "one")],
            )
            .await
            .unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let map = RepoMap::build(&store, "repo", dir.path(), 3, 15, &[], 2_500)
            .await
            .unwrap();
        assert!(!map.truncated);
    }
}
