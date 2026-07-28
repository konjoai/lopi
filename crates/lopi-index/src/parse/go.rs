//! Go symbol/ref extraction (tree-sitter-go).
//!
//! Go's one wrinkle relative to `parse/rust.rs`'s `impl_item` case: there's
//! no separate container node for a type's methods — a `method_declaration`
//! carries its own `name` field *and* a `receiver` field (the `(f *Foo)`
//! part), so the qualifying type name has to be pulled out of the receiver's
//! subtree rather than from a parent stack.

use super::{doc_first_line, end_line1, line1, parse_tree, signature_of, ParsedFile};
use crate::types::{Language, NewSymbol, SymbolKind};
use anyhow::{Context, Result};
use tree_sitter::Node;

const COMMENT_KINDS: &[&str] = &["comment"];
const DOC_PREFIXES: &[&str] = &["//", "/*"];

/// Extract symbols + refs from Go source.
///
/// # Errors
/// Returns `Err` if the grammar can't be loaded or tree-sitter produces no
/// tree at all.
pub fn extract(source: &str) -> Result<ParsedFile> {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
    parser.set_language(&lang).context("loading go grammar")?;
    let tree = parse_tree(&mut parser, source)?;
    let bytes = source.as_bytes();

    let mut out = ParsedFile::default();
    let mut next_id = 0usize;
    walk(tree.root_node(), bytes, None, &mut out, &mut next_id);
    Ok(out)
}

fn walk(node: Node, src: &[u8], enclosing_fn: Option<usize>, out: &mut ParsedFile, next_id: &mut usize) {
    if node.kind() == "call_expression" {
        super::push_call_ref(node, src, enclosing_fn, out);
    }

    let mut child_enclosing = enclosing_fn;
    match node.kind() {
        "function_declaration" => {
            if let Some(local_id) = push_named(node, src, SymbolKind::Fn, node.utf8_text(src).ok(), out, next_id) {
                child_enclosing = Some(local_id);
            }
        }
        "method_declaration" => {
            if let Some(local_id) = push_method(node, src, out, next_id) {
                child_enclosing = Some(local_id);
            }
        }
        "type_declaration" => push_type_specs(node, src, out, next_id),
        "const_declaration" => push_const_specs(node, src, out, next_id),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, child_enclosing, out, next_id);
    }
}

/// `function_declaration`: has a `name` field directly, no receiver, no container.
fn push_named(
    node: Node,
    src: &[u8],
    kind: SymbolKind,
    _unused: Option<&str>,
    out: &mut ParsedFile,
    next_id: &mut usize,
) -> Option<usize> {
    let name = node.child_by_field_name("name")?.utf8_text(src).ok()?;
    Some(push_symbol(node, src, kind, None, name.to_string(), out, next_id))
}

fn push_method(node: Node, src: &[u8], out: &mut ParsedFile, next_id: &mut usize) -> Option<usize> {
    let name = node.child_by_field_name("name")?.utf8_text(src).ok()?;
    let receiver_type = node
        .child_by_field_name("receiver")
        .and_then(|r| find_type_identifier(r, src));
    let qualified = receiver_type
        .as_ref()
        .map_or_else(|| name.to_string(), |t| format!("{t}.{name}"));
    Some(push_symbol(node, src, SymbolKind::Method, Some(qualified), name.to_string(), out, next_id))
}

/// A `type_declaration` can carry multiple `type_spec` children
/// (`type ( A struct{}; B int )`); each becomes its own symbol.
fn push_type_specs(node: Node, src: &[u8], out: &mut ParsedFile, next_id: &mut usize) {
    let mut cursor = node.walk();
    for spec in node.named_children(&mut cursor).filter(|c| c.kind() == "type_spec") {
        let Some(name_node) = spec.child_by_field_name("name") else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(src) else {
            continue;
        };
        let kind = spec
            .child_by_field_name("type")
            .map_or(SymbolKind::Type, |t| match t.kind() {
                "struct_type" => SymbolKind::Struct,
                "interface_type" => SymbolKind::Trait,
                _ => SymbolKind::Type,
            });
        push_symbol(spec, src, kind, None, name.to_string(), out, next_id);
    }
}

/// A `const_declaration` can carry multiple `const_spec` children.
fn push_const_specs(node: Node, src: &[u8], out: &mut ParsedFile, next_id: &mut usize) {
    let mut cursor = node.walk();
    for spec in node.named_children(&mut cursor).filter(|c| c.kind() == "const_spec") {
        let Some(name_node) = spec.child_by_field_name("name") else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(src) else {
            continue;
        };
        push_symbol(spec, src, SymbolKind::Const, None, name.to_string(), out, next_id);
    }
}

fn push_symbol(
    node: Node,
    src: &[u8],
    kind: SymbolKind,
    qualified_override: Option<String>,
    name: String,
    out: &mut ParsedFile,
    next_id: &mut usize,
) -> usize {
    let local_id = *next_id;
    *next_id += 1;
    let is_public = name.chars().next().is_some_and(char::is_uppercase);
    out.symbols.push(NewSymbol {
        local_id,
        local_parent: None,
        lang: Language::Go,
        kind,
        qualified_name: qualified_override.unwrap_or_else(|| name.clone()),
        name,
        signature: signature_of(node, src),
        doc_first_line: doc_first_line(node, src, COMMENT_KINDS, DOC_PREFIXES),
        line_start: line1(node),
        line_end: end_line1(node),
        byte_start: node.start_byte() as u32,
        byte_end: node.end_byte() as u32,
        is_public,
    });
    local_id
}

/// Depth-first search for the first `type_identifier` inside `node` — used
/// to pull `Foo` out of a method receiver's `(f *Foo)`/`(f Foo)`.
fn find_type_identifier(node: Node, src: &[u8]) -> Option<String> {
    if node.kind() == "type_identifier" {
        return node.utf8_text(src).ok().map(str::to_string);
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find_map(|c| find_type_identifier(c, src));
    found
}


#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::extract;
    use crate::types::SymbolKind;

    #[test]
    fn extracts_function_and_method_with_receiver_qualification() {
        let src = "package main\n\ntype Foo struct{}\n\nfunc (f *Foo) Bar() {\n  baz()\n}\n\nfunc Top() {}\n";
        let out = extract(src).unwrap();
        let bar = out.symbols.iter().find(|s| s.name == "Bar").unwrap();
        assert_eq!(bar.kind, SymbolKind::Method);
        assert_eq!(bar.qualified_name, "Foo.Bar");
        assert!(bar.is_public);

        let top = out.symbols.iter().find(|s| s.name == "Top").unwrap();
        assert_eq!(top.kind, SymbolKind::Fn);

        assert_eq!(out.refs.len(), 1);
        assert_eq!(out.refs[0].to_name, "baz");
        assert_eq!(out.refs[0].from_local_id, Some(bar.local_id));
    }

    #[test]
    fn struct_interface_and_const_extracted() {
        let src = "package main\n\ntype Shape interface{ Area() int }\n\nconst Max = 1\n\nfunc lower() {}\n";
        let out = extract(src).unwrap();
        assert!(out.symbols.iter().any(|s| s.kind == SymbolKind::Trait && s.name == "Shape"));
        assert!(out.symbols.iter().any(|s| s.kind == SymbolKind::Const && s.name == "Max"));
        let lower = out.symbols.iter().find(|s| s.name == "lower").unwrap();
        assert!(!lower.is_public, "lowercase Go identifiers are unexported");
    }
}
