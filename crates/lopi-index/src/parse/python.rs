//! Python symbol/ref extraction (tree-sitter-python).
//!
//! Scope note (see `LEDGER.md`): module-level constant assignment isn't
//! extracted. Python has no dedicated const-declaration syntax — a plain
//! `x = 1` assignment is indistinguishable at the grammar level from any
//! other variable binding (including ones inside function bodies), and
//! guessing "top-level + no reassignment" well enough to be useful is a
//! bigger job than this pass's budget covers. `fn`/`method`/`class` (what
//! `lopi_find`/`lopi_refs` actually need for navigation) are unaffected.

use super::{doc_first_line, end_line1, line1, parse_tree, signature_of, ParsedFile};
use crate::types::{Language, NewSymbol, SymbolKind};
use anyhow::{Context, Result};
use tree_sitter::Node;

const COMMENT_KINDS: &[&str] = &["comment"];
const DOC_PREFIXES: &[&str] = &["#"];

/// Extract symbols + refs from Python source.
///
/// # Errors
/// Returns `Err` if the grammar can't be loaded or tree-sitter produces no
/// tree at all.
pub fn extract(source: &str) -> Result<ParsedFile> {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    parser.set_language(&lang).context("loading python grammar")?;
    let tree = parse_tree(&mut parser, source)?;
    let bytes = source.as_bytes();

    let mut out = ParsedFile::default();
    let mut next_id = 0usize;
    let ctx = Ctx {
        qual_prefix: String::new(),
        parent_local: None,
        enclosing_fn: None,
        in_class: false,
    };
    walk(tree.root_node(), bytes, &ctx, &mut out, &mut next_id);
    Ok(out)
}

#[derive(Clone)]
struct Ctx {
    qual_prefix: String,
    parent_local: Option<usize>,
    enclosing_fn: Option<usize>,
    in_class: bool,
}

fn walk(node: Node, src: &[u8], ctx: &Ctx, out: &mut ParsedFile, next_id: &mut usize) {
    if node.kind() == "call" {
        super::push_call_ref(node, src, ctx.enclosing_fn, out);
    }

    let child_ctx = match node.kind() {
        "function_definition" => {
            let kind = if ctx.in_class { SymbolKind::Method } else { SymbolKind::Fn };
            named(node, src, kind, false, ctx, out, next_id)
        }
        "class_definition" => named(node, src, SymbolKind::Class, true, ctx, out, next_id),
        _ => ctx.clone(),
    };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, &child_ctx, out, next_id);
    }
}

fn named(
    node: Node,
    src: &[u8],
    kind: SymbolKind,
    is_container: bool,
    ctx: &Ctx,
    out: &mut ParsedFile,
    next_id: &mut usize,
) -> Ctx {
    let Some(name_node) = node.child_by_field_name("name") else {
        return ctx.clone();
    };
    let Ok(name) = name_node.utf8_text(src) else {
        return ctx.clone();
    };
    let local_id = *next_id;
    *next_id += 1;
    out.symbols.push(NewSymbol {
        local_id,
        local_parent: ctx.parent_local,
        lang: Language::Python,
        kind,
        name: name.to_string(),
        qualified_name: format!("{}{name}", ctx.qual_prefix),
        signature: signature_of(node, src),
        doc_first_line: doc_first_line(node, src, COMMENT_KINDS, DOC_PREFIXES),
        line_start: line1(node),
        line_end: end_line1(node),
        byte_start: node.start_byte() as u32,
        byte_end: node.end_byte() as u32,
        is_public: !name.starts_with('_'),
    });
    let is_fn_like = matches!(kind, SymbolKind::Fn | SymbolKind::Method);
    Ctx {
        qual_prefix: if is_container {
            format!("{}{name}.", ctx.qual_prefix)
        } else {
            ctx.qual_prefix.clone()
        },
        parent_local: Some(local_id),
        enclosing_fn: if is_fn_like { Some(local_id) } else { ctx.enclosing_fn },
        in_class: is_container,
    }
}


#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::extract;
    use crate::types::SymbolKind;

    #[test]
    fn extracts_function_with_doc() {
        let src = "def inc(x):\n    \"\"\"docstring not handled as # comment\"\"\"\n    return x + 1\n";
        let out = extract(src).unwrap();
        assert_eq!(out.symbols.len(), 1);
        assert_eq!(out.symbols[0].name, "inc");
        assert_eq!(out.symbols[0].kind, SymbolKind::Fn);
    }

    #[test]
    fn hash_comment_becomes_doc_first_line() {
        let src = "# Adds one.\ndef inc(x):\n    return x + 1\n";
        let out = extract(src).unwrap();
        assert_eq!(out.symbols[0].doc_first_line.as_deref(), Some("Adds one."));
    }

    #[test]
    fn class_methods_get_dotted_qualified_names() {
        let src = "class Foo:\n    def bar(self):\n        baz()\n        self.qux()\n";
        let out = extract(src).unwrap();
        let bar = out.symbols.iter().find(|s| s.name == "bar").unwrap();
        assert_eq!(bar.kind, SymbolKind::Method);
        assert_eq!(bar.qualified_name, "Foo.bar");
        let names: Vec<_> = out.refs.iter().map(|r| r.to_name.as_str()).collect();
        assert_eq!(names, vec!["baz", "qux"]);
    }

    #[test]
    fn leading_underscore_is_not_public() {
        let src = "def _hidden():\n    pass\n";
        let out = extract(src).unwrap();
        assert!(!out.symbols[0].is_public);
    }
}
