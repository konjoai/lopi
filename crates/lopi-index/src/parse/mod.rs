//! Tree-sitter parsing dispatch + the helpers every `parse/<lang>.rs` shares
//! (signature slicing, doc-comment lookup, call-site callee extraction).
//!
//! Adding a grammar: a new [`crate::Language`] variant, one arm each in
//! [`parse_file`] and `grammar_extensions` (`reindex.rs`), and a new
//! `parse/<lang>.rs` implementing that language's own symbol/ref walk using
//! the helpers below. Parse failures are never fatal — [`parse_file`]
//! returns `Err`, and every caller (`reindex.rs`) logs + skips the file.

mod go;
mod js;
mod js_common;
mod python;
mod rust;
mod ts;

use crate::types::{Language, NewRef, NewSymbol};
use anyhow::{Context, Result};

/// One file's extracted symbols + refs, before database insertion.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ParsedFile {
    /// Symbols found, in the order encountered (parent before child).
    pub symbols: Vec<NewSymbol>,
    /// References found, in source order.
    pub refs: Vec<NewRef>,
}

/// Parse `source` (assumed to be `lang`) into symbols + refs.
///
/// # Errors
/// Returns `Err` if the parser fails to build a tree at all (tree-sitter's
/// error-recovery means a syntax error alone does not fail this — only a
/// parser/language mismatch or a `None` tree does).
pub fn parse_file(lang: Language, source: &str) -> Result<ParsedFile> {
    match lang {
        Language::Rust => rust::extract(source),
        Language::TypeScript => ts::extract(source),
        Language::JavaScript => js::extract(source),
        Language::Python => python::extract(source),
        Language::Go => go::extract(source),
    }
}

/// Run `parser` over `source`, erroring rather than panicking when
/// tree-sitter can't produce a tree at all.
pub(crate) fn parse_tree(
    parser: &mut tree_sitter::Parser,
    source: &str,
) -> Result<tree_sitter::Tree> {
    parser
        .parse(source, None)
        .context("tree-sitter returned no tree")
}

/// One line, normalized-whitespace signature: `node`'s text up to (not
/// including) its `body` field, or the whole node when there is no body
/// field (e.g. a trait method with no default impl). Never includes a body.
pub(crate) fn signature_of(node: tree_sitter::Node, source: &[u8]) -> String {
    let end = node
        .child_by_field_name("body")
        .map_or_else(|| node.end_byte(), |b| b.start_byte());
    let start = node.start_byte();
    let raw = std::str::from_utf8(&source[start..end.max(start)]).unwrap_or("");
    normalize_ws(raw.trim_end_matches(['{', ';']).trim())
}

/// Collapse runs of whitespace (including newlines) to single spaces.
pub(crate) fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first line of the contiguous comment block immediately preceding
/// `node` (its previous sibling chain), stripped of `strip_prefixes`. `None`
/// when there's no comment directly above, or a blank line separates them.
pub(crate) fn doc_first_line(
    node: tree_sitter::Node,
    source: &[u8],
    comment_kinds: &[&str],
    strip_prefixes: &[&str],
) -> Option<String> {
    let mut prev = node.prev_sibling()?;
    let mut expected_end_row = node.start_position().row;
    let mut lines = Vec::new();
    while comment_kinds.contains(&prev.kind()) && prev.end_position().row + 1 >= expected_end_row {
        let text = prev.utf8_text(source).ok()?.trim();
        let stripped = strip_prefixes
            .iter()
            .find_map(|p| text.strip_prefix(p))
            .unwrap_or(text)
            .trim();
        lines.push(stripped.to_string());
        expected_end_row = prev.start_position().row;
        match prev.prev_sibling() {
            Some(p) => prev = p,
            None => break,
        }
    }
    lines.reverse();
    lines.into_iter().find(|l| !l.is_empty())
}

/// The textual name of a call expression's callee, descending into the
/// rightmost identifier-like leaf of `node` (handles `foo()`,
/// `obj.method()`, `Type::assoc()`, `pkg.Func()` uniformly across grammars
/// without per-language special-casing).
pub(crate) fn callee_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    if node.kind().ends_with("identifier") {
        return node.utf8_text(source).ok().map(str::to_string);
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .last()
        .and_then(|c| callee_name(c, source))
}

/// Record a call-site reference: read `node`'s `"function"` field, resolve
/// its callee name via [`callee_name`], and push a [`NewRef`] scoped to
/// `enclosing_fn`. Shared by every `parse/<lang>.rs`'s own call-expression
/// handling — the "get the callee, push a ref" tail is identical across
/// languages even though each has a different node kind that triggers it
/// (`call_expression` for Rust/Go/JS/TS, `call` for Python) and a different
/// shape for `enclosing_fn` (a bare `Option<usize>` vs. a field on that
/// language's own traversal `Ctx`), so only this common tail is factored
/// out rather than the whole per-language `walk`.
pub(crate) fn push_call_ref(
    node: tree_sitter::Node,
    source: &[u8],
    enclosing_fn: Option<usize>,
    out: &mut ParsedFile,
) {
    let Some(func) = node.child_by_field_name("function") else {
        return;
    };
    let Some(name) = callee_name(func, source) else {
        return;
    };
    out.refs.push(NewRef {
        from_local_id: enclosing_fn,
        to_name: name,
        line: line1(node),
    });
}

/// 1-based start line for a node.
pub(crate) fn line1(node: tree_sitter::Node) -> u32 {
    (node.start_position().row + 1) as u32
}

/// 1-based end line for a node.
pub(crate) fn end_line1(node: tree_sitter::Node) -> u32 {
    (node.end_position().row + 1) as u32
}

#[cfg(test)]
mod tests {
    use super::normalize_ws;

    #[test]
    fn normalize_ws_collapses_newlines_and_indentation() {
        assert_eq!(normalize_ws("fn foo(\n    x: i32,\n)"), "fn foo( x: i32, )");
    }

    #[test]
    fn normalize_ws_trims_and_single_spaces() {
        assert_eq!(normalize_ws("  a   b  "), "a b");
    }
}
