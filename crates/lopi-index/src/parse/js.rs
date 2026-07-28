//! JavaScript symbol/ref extraction (tree-sitter-javascript). Shares its
//! walk with `ts.rs` via `js_common` — see that module's doc comment.

use super::js_common::walk_tree;
use super::{parse_tree, ParsedFile};
use crate::types::Language;
use anyhow::{Context, Result};

/// Extract symbols + refs from JavaScript source.
///
/// # Errors
/// Returns `Err` if the grammar can't be loaded or tree-sitter produces no
/// tree at all.
pub fn extract(source: &str) -> Result<ParsedFile> {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
    parser.set_language(&lang).context("loading javascript grammar")?;
    let tree = parse_tree(&mut parser, source)?;
    Ok(walk_tree(tree.root_node(), source.as_bytes(), Language::JavaScript, false))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::extract;
    use crate::types::SymbolKind;

    #[test]
    fn extracts_function_and_class() {
        let src = "function greet(name) {\n  return `hi ${name}`;\n}\n\nclass Widget {\n  render() {}\n}\n";
        let out = extract(src).unwrap();
        assert!(out.symbols.iter().any(|s| s.name == "greet" && s.kind == SymbolKind::Fn));
        let render = out.symbols.iter().find(|s| s.name == "render").unwrap();
        assert_eq!(render.kind, SymbolKind::Method);
        assert_eq!(render.qualified_name, "Widget.render");
    }

    #[test]
    fn ts_only_kinds_are_not_recognized_in_plain_js() {
        // `interface` isn't valid JS syntax at all — this just documents
        // that js.rs never activates the ts_extras arms.
        let src = "const x = 1;\n";
        let out = extract(src).unwrap();
        assert_eq!(out.symbols.len(), 1);
        assert_eq!(out.symbols[0].kind, SymbolKind::Const);
    }

    #[test]
    fn method_call_and_plain_call_both_recorded() {
        let src = "function a(obj) {\n  b();\n  obj.c();\n}\n";
        let out = extract(src).unwrap();
        let names: Vec<_> = out.refs.iter().map(|r| r.to_name.as_str()).collect();
        assert_eq!(names, vec!["b", "c"]);
    }
}
