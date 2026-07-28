//! Rust symbol/ref extraction (tree-sitter-rust).
//!
//! `impl_item` is the one Rust-specific wrinkle the generic helpers in
//! `parse/mod.rs` don't cover: it has no `name` field (its "name" is the
//! `type` field — the type being implemented for), and it turns every
//! directly-nested `function_item` into a [`SymbolKind::Method`] instead of
//! a free [`SymbolKind::Fn`]. `trait_item` gets the same treatment for its
//! default-method bodies.

use super::{doc_first_line, end_line1, line1, parse_tree, signature_of, ParsedFile};
use crate::types::{NewSymbol, SymbolKind};
use anyhow::{Context, Result};
use tree_sitter::Node;

const COMMENT_KINDS: &[&str] = &["line_comment", "block_comment"];
const DOC_PREFIXES: &[&str] = &["///", "//!", "/**", "/*!", "/*", "//"];

/// Extract symbols + refs from Rust source.
///
/// # Errors
/// Returns `Err` if the rust grammar can't be loaded or tree-sitter
/// produces no tree at all.
pub fn extract(source: &str) -> Result<ParsedFile> {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    parser.set_language(&lang).context("loading rust grammar")?;
    let tree = parse_tree(&mut parser, source)?;
    let bytes = source.as_bytes();

    let mut out = ParsedFile::default();
    let mut next_id = 0usize;
    let ctx = Ctx {
        qual_prefix: String::new(),
        parent_local: None,
        enclosing_fn: None,
        in_container: false,
    };
    walk(tree.root_node(), bytes, &ctx, &mut out, &mut next_id);
    Ok(out)
}

/// Threaded traversal state — cheap to clone, so each recursive call builds
/// its own updated copy rather than mutating a shared one.
#[derive(Clone)]
struct Ctx {
    qual_prefix: String,
    parent_local: Option<usize>,
    enclosing_fn: Option<usize>,
    in_container: bool,
}

fn walk(node: Node, src: &[u8], ctx: &Ctx, out: &mut ParsedFile, next_id: &mut usize) {
    if node.kind() == "call_expression" {
        super::push_call_ref(node, src, ctx.enclosing_fn, out);
    }

    let child_ctx = match node.kind() {
        "function_item" => symbol_ctx(
            node,
            src,
            ctx,
            if ctx.in_container {
                SymbolKind::Method
            } else {
                SymbolKind::Fn
            },
            "name",
            false,
            false,
            out,
            next_id,
        ),
        "struct_item" => symbol_ctx(
            node,
            src,
            ctx,
            SymbolKind::Struct,
            "name",
            false,
            false,
            out,
            next_id,
        ),
        "enum_item" => symbol_ctx(
            node,
            src,
            ctx,
            SymbolKind::Enum,
            "name",
            false,
            false,
            out,
            next_id,
        ),
        "trait_item" => symbol_ctx(
            node,
            src,
            ctx,
            SymbolKind::Trait,
            "name",
            true,
            true,
            out,
            next_id,
        ),
        "const_item" => symbol_ctx(
            node,
            src,
            ctx,
            SymbolKind::Const,
            "name",
            false,
            false,
            out,
            next_id,
        ),
        "type_item" => symbol_ctx(
            node,
            src,
            ctx,
            SymbolKind::Type,
            "name",
            false,
            false,
            out,
            next_id,
        ),
        "mod_item" => symbol_ctx(
            node,
            src,
            ctx,
            SymbolKind::Module,
            "name",
            true,
            false,
            out,
            next_id,
        ),
        "impl_item" => impl_ctx(node, src, ctx, out, next_id),
        _ => ctx.clone(),
    };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, &child_ctx, out, next_id);
    }
}

/// Handle a declaration node whose "name" is `child_by_field_name(name_field)`.
///
/// `namespaces` and `methods_children` are independent: a `mod` namespaces
/// its children's qualified names (`m::inner`) but does not turn a nested
/// `fn` into a `Method` — only `impl`/`trait` do that.
#[allow(clippy::too_many_arguments)]
fn symbol_ctx(
    node: Node,
    src: &[u8],
    ctx: &Ctx,
    kind: SymbolKind,
    name_field: &str,
    namespaces: bool,
    methods_children: bool,
    out: &mut ParsedFile,
    next_id: &mut usize,
) -> Ctx {
    let Some(name_node) = node.child_by_field_name(name_field) else {
        return ctx.clone();
    };
    let Ok(name) = name_node.utf8_text(src) else {
        return ctx.clone();
    };
    let local_id = push_symbol(node, src, ctx, kind, name, out, next_id);

    let is_fn_like = matches!(kind, SymbolKind::Fn | SymbolKind::Method);
    Ctx {
        qual_prefix: if namespaces {
            format!("{}{name}::", ctx.qual_prefix)
        } else {
            ctx.qual_prefix.clone()
        },
        parent_local: Some(local_id),
        enclosing_fn: if is_fn_like {
            Some(local_id)
        } else {
            ctx.enclosing_fn
        },
        in_container: methods_children,
    }
}

/// `impl_item` has no `name` field — its identity is the `type` field (the
/// type being implemented for), and its direct `function_item` children are
/// methods, not free functions.
fn impl_ctx(node: Node, src: &[u8], ctx: &Ctx, out: &mut ParsedFile, next_id: &mut usize) -> Ctx {
    let Some(type_node) = node.child_by_field_name("type") else {
        return ctx.clone();
    };
    let Ok(type_text) = type_node.utf8_text(src) else {
        return ctx.clone();
    };
    let name = super::normalize_ws(type_text);
    let local_id = push_symbol(node, src, ctx, SymbolKind::Impl, &name, out, next_id);
    Ctx {
        qual_prefix: format!("{}{name}::", ctx.qual_prefix),
        parent_local: Some(local_id),
        enclosing_fn: ctx.enclosing_fn,
        in_container: true,
    }
}

fn push_symbol(
    node: Node,
    src: &[u8],
    ctx: &Ctx,
    kind: SymbolKind,
    name: &str,
    out: &mut ParsedFile,
    next_id: &mut usize,
) -> usize {
    let local_id = *next_id;
    *next_id += 1;
    out.symbols.push(NewSymbol {
        local_id,
        local_parent: ctx.parent_local,
        lang: crate::types::Language::Rust,
        kind,
        name: name.to_string(),
        qualified_name: format!("{}{name}", ctx.qual_prefix),
        signature: signature_of(node, src),
        doc_first_line: doc_first_line(node, src, COMMENT_KINDS, DOC_PREFIXES),
        line_start: line1(node),
        line_end: end_line1(node),
        byte_start: node.start_byte() as u32,
        byte_end: node.end_byte() as u32,
        is_public: has_visibility_modifier(node),
    });
    local_id
}

fn has_visibility_modifier(node: Node) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|c| c.kind() == "visibility_modifier");
    found
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::extract;
    use crate::types::SymbolKind;

    #[test]
    fn extracts_free_function_with_doc_and_signature() {
        let src = "/// Adds one.\npub fn inc(x: i32) -> i32 {\n    x + 1\n}\n";
        let out = extract(src).unwrap();
        assert_eq!(out.symbols.len(), 1);
        let s = &out.symbols[0];
        assert_eq!(s.name, "inc");
        assert_eq!(s.qualified_name, "inc");
        assert_eq!(s.kind, SymbolKind::Fn);
        assert_eq!(s.signature, "pub fn inc(x: i32) -> i32");
        assert_eq!(s.doc_first_line.as_deref(), Some("Adds one."));
        assert!(s.is_public);
    }

    #[test]
    fn impl_methods_become_methods_with_qualified_names() {
        let src = "struct Foo;\nimpl Foo {\n    pub fn bar(&self) {}\n}\n";
        let out = extract(src).unwrap();
        let bar = out.symbols.iter().find(|s| s.name == "bar").unwrap();
        assert_eq!(bar.kind, SymbolKind::Method);
        assert_eq!(bar.qualified_name, "Foo::bar");
        let imp = out
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Impl)
            .unwrap();
        assert_eq!(bar.local_parent, Some(imp.local_id));
    }

    #[test]
    fn private_function_is_not_public() {
        let src = "fn hidden() {}\n";
        let out = extract(src).unwrap();
        assert!(!out.symbols[0].is_public);
    }

    #[test]
    fn call_inside_function_is_recorded_with_enclosing_fn() {
        let src = "fn a() {\n    b();\n}\nfn b() {}\n";
        let out = extract(src).unwrap();
        let a = out.symbols.iter().find(|s| s.name == "a").unwrap();
        assert_eq!(out.refs.len(), 1);
        assert_eq!(out.refs[0].to_name, "b");
        assert_eq!(out.refs[0].from_local_id, Some(a.local_id));
    }

    #[test]
    fn method_call_extracts_rightmost_identifier() {
        let src = "fn a(x: Foo) {\n    x.bar();\n}\n";
        let out = extract(src).unwrap();
        assert_eq!(out.refs[0].to_name, "bar");
    }

    #[test]
    fn struct_enum_trait_const_type_mod_all_extracted() {
        let src = r#"
struct S;
enum E { A }
trait T {}
const C: i32 = 1;
type Alias = i32;
mod m { fn inner() {} }
"#;
        let out = extract(src).unwrap();
        let kinds: Vec<_> = out.symbols.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SymbolKind::Struct));
        assert!(kinds.contains(&SymbolKind::Enum));
        assert!(kinds.contains(&SymbolKind::Trait));
        assert!(kinds.contains(&SymbolKind::Const));
        assert!(kinds.contains(&SymbolKind::Type));
        assert!(kinds.contains(&SymbolKind::Module));
        let inner = out.symbols.iter().find(|s| s.name == "inner").unwrap();
        assert_eq!(inner.qualified_name, "m::inner");
    }

    #[test]
    fn syntax_error_does_not_panic() {
        let src = "fn broken(x: {{{ ??? \n";
        assert!(
            extract(src).is_ok(),
            "tree-sitter error recovery, never a hard failure"
        );
    }
}
