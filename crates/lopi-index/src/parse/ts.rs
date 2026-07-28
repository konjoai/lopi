//! TypeScript symbol/ref extraction (tree-sitter-typescript, `.ts`/`.tsx`
//! grammar). Shares its walk with `js.rs` via `js_common` — TypeScript's
//! grammar is JavaScript's plus `interface_declaration`/
//! `type_alias_declaration`, so this file only supplies the language +
//! `ts_extras = true`.

use super::js_common::walk_tree;
use super::{parse_tree, ParsedFile};
use crate::types::Language;
use anyhow::{Context, Result};

/// Extract symbols + refs from TypeScript source.
///
/// # Errors
/// Returns `Err` if the grammar can't be loaded or tree-sitter produces no
/// tree at all.
pub fn extract(source: &str) -> Result<ParsedFile> {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser
        .set_language(&lang)
        .context("loading typescript grammar")?;
    let tree = parse_tree(&mut parser, source)?;
    Ok(walk_tree(
        tree.root_node(),
        source.as_bytes(),
        Language::TypeScript,
        true,
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::extract;
    use crate::types::SymbolKind;

    #[test]
    fn extracts_exported_function_and_class_method() {
        let src = "export function add(a: number, b: number): number {\n  return a + b;\n}\n\nclass Box {\n  get(): number { return 1; }\n}\n";
        let out = extract(src).unwrap();
        let add = out.symbols.iter().find(|s| s.name == "add").unwrap();
        assert_eq!(add.kind, SymbolKind::Fn);
        assert!(add.is_public);

        let get = out.symbols.iter().find(|s| s.name == "get").unwrap();
        assert_eq!(get.kind, SymbolKind::Method);
        assert_eq!(get.qualified_name, "Box.get");
    }

    #[test]
    fn interface_and_type_alias_extracted() {
        let src = "interface Shape { area(): number; }\ntype Id = string;\n";
        let out = extract(src).unwrap();
        assert!(out
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Trait && s.name == "Shape"));
        assert!(out
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Type && s.name == "Id"));
    }

    #[test]
    fn exported_const_is_public() {
        let src = "export const MAX = 10;\nconst hidden = 1;\n";
        let out = extract(src).unwrap();
        let max = out.symbols.iter().find(|s| s.name == "MAX").unwrap();
        assert!(max.is_public);
        let hidden = out.symbols.iter().find(|s| s.name == "hidden").unwrap();
        assert!(!hidden.is_public);
    }

    #[test]
    fn call_expression_recorded() {
        let src = "function a() {\n  b();\n}\n";
        let out = extract(src).unwrap();
        assert_eq!(out.refs.len(), 1);
        assert_eq!(out.refs[0].to_name, "b");
    }
}
