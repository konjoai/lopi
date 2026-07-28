//! Shared TS/JS walker — TypeScript's grammar is JavaScript's plus a few
//! extra declaration kinds (`interface_declaration`, `type_alias_declaration`),
//! so `ts.rs`/`js.rs` are thin wrappers over this one walk, parameterized by
//! `ts_extras` rather than duplicated.

use super::{doc_first_line, end_line1, line1, signature_of, ParsedFile};
use crate::types::{Language, NewSymbol, SymbolKind};
use tree_sitter::Node;

const COMMENT_KINDS: &[&str] = &["comment"];
const DOC_PREFIXES: &[&str] = &["/**", "/*", "//"];

/// Run the shared TS/JS walk over an already-parsed tree.
pub(super) fn walk_tree(root: Node, src: &[u8], lang: Language, ts_extras: bool) -> ParsedFile {
    let mut out = ParsedFile::default();
    let mut next_id = 0usize;
    let ctx = Ctx {
        qual_prefix: String::new(),
        parent_local: None,
        enclosing_fn: None,
        in_class: false,
    };
    walk(root, src, lang, ts_extras, &ctx, &mut out, &mut next_id);
    out
}

#[derive(Clone)]
struct Ctx {
    qual_prefix: String,
    parent_local: Option<usize>,
    enclosing_fn: Option<usize>,
    in_class: bool,
}

fn walk(
    node: Node,
    src: &[u8],
    lang: Language,
    ts_extras: bool,
    ctx: &Ctx,
    out: &mut ParsedFile,
    next_id: &mut usize,
) {
    if node.kind() == "call_expression" {
        super::push_call_ref(node, src, ctx.enclosing_fn, out);
    }

    let child_ctx = match node.kind() {
        "function_declaration" => named(node, src, lang, SymbolKind::Fn, false, ctx, out, next_id),
        "class_declaration" => named(node, src, lang, SymbolKind::Class, true, ctx, out, next_id),
        "method_definition" if ctx.in_class => named(
            node,
            src,
            lang,
            SymbolKind::Method,
            false,
            ctx,
            out,
            next_id,
        ),
        "interface_declaration" if ts_extras => {
            named(node, src, lang, SymbolKind::Trait, true, ctx, out, next_id)
        }
        "type_alias_declaration" if ts_extras => {
            named(node, src, lang, SymbolKind::Type, false, ctx, out, next_id)
        }
        "lexical_declaration" => {
            for declarator in const_declarators(node) {
                push_const(declarator, src, lang, ctx, out, next_id);
            }
            ctx.clone()
        }
        _ => ctx.clone(),
    };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, lang, ts_extras, &child_ctx, out, next_id);
    }
}

/// A declaration node with a `name` field — functions, classes, interfaces, type aliases.
#[allow(clippy::too_many_arguments)]
fn named(
    node: Node,
    src: &[u8],
    lang: Language,
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
    let local_id = push_symbol(node, src, lang, ctx, kind, name, out, next_id);
    let is_fn_like = matches!(kind, SymbolKind::Fn | SymbolKind::Method);
    Ctx {
        qual_prefix: if is_container {
            format!("{}{name}.", ctx.qual_prefix)
        } else {
            ctx.qual_prefix.clone()
        },
        parent_local: Some(local_id),
        enclosing_fn: if is_fn_like {
            Some(local_id)
        } else {
            ctx.enclosing_fn
        },
        in_class: is_container && kind == SymbolKind::Class,
    }
}

/// `const`/`let` declarators at any scope — only `const` becomes a symbol
/// (the brief's kind set has no binding-mutability distinction beyond that).
fn const_declarators(lexical_decl: Node) -> Vec<Node> {
    let is_const = lexical_decl.child(0).is_some_and(|kw| kw.kind() == "const");
    if !is_const {
        return Vec::new();
    }
    let mut cursor = lexical_decl.walk();
    lexical_decl
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "variable_declarator")
        .collect()
}

fn push_const(
    declarator: Node,
    src: &[u8],
    lang: Language,
    ctx: &Ctx,
    out: &mut ParsedFile,
    next_id: &mut usize,
) {
    let Some(name_node) = declarator.child_by_field_name("name") else {
        return;
    };
    let Ok(name) = name_node.utf8_text(src) else {
        return;
    };
    push_symbol(
        declarator,
        src,
        lang,
        ctx,
        SymbolKind::Const,
        name,
        out,
        next_id,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_symbol(
    node: Node,
    src: &[u8],
    lang: Language,
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
        lang,
        kind,
        name: name.to_string(),
        qualified_name: format!("{}{name}", ctx.qual_prefix),
        signature: signature_of(node, src),
        doc_first_line: doc_first_line(node, src, COMMENT_KINDS, DOC_PREFIXES),
        line_start: line1(node),
        line_end: end_line1(node),
        byte_start: node.start_byte() as u32,
        byte_end: node.end_byte() as u32,
        is_public: is_exported(node),
    });
    local_id
}

/// A declaration is exported when it's wrapped (directly, or via one
/// intermediate `lexical_declaration` — a `const`'s own parent, since a
/// `variable_declarator` isn't itself export-wrapped, its `lexical_declaration`
/// grandparent is) by `export_statement`.
fn is_exported(node: Node) -> bool {
    let mut cur = node;
    loop {
        match cur.parent() {
            Some(p) if p.kind() == "export_statement" => return true,
            Some(p) if p.kind() == "lexical_declaration" => cur = p,
            _ => return false,
        }
    }
}
