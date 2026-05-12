//! Extract spec items from Rust source files.
//!
//! Parses line-by-line: no full AST required. Looks for lines that carry
//! a test attribute immediately followed by a `fn` declaration. Captures
//! any preceding doc comment as the description.

use anyhow::Result;
use std::path::Path;

use crate::{SpecItem, SpecKind};

const TEST_ATTRS: &[&str] = &[
    "#[test]",
    "#[tokio::test]",
    "#[async_std::test]",
    "#[rstest]",
    "#[proptest]",
];

/// Extract all test functions from a Rust source file.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub fn extract_rust(path: impl AsRef<Path>) -> Result<Vec<SpecItem>> {
    let source = std::fs::read_to_string(path)?;
    let mut items = Vec::new();
    let mut doc_buf: Vec<String> = Vec::new();
    let mut next_is_test = false;

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = (idx + 1) as u32;

        // Accumulate doc comments above any item.
        if trimmed.starts_with("///") {
            let text = trimmed.trim_start_matches("///").trim().to_string();
            doc_buf.push(text);
            continue;
        }

        // Detect test attribute.
        if TEST_ATTRS.iter().any(|a| trimmed.starts_with(a)) {
            next_is_test = true;
            continue;
        }

        // Detect function definition following a test attribute.
        if next_is_test && trimmed.starts_with("fn ") || trimmed.starts_with("async fn ") {
            next_is_test = false;
            if let Some(name) = parse_fn_name(trimmed) {
                items.push(build_item(name, &doc_buf, line_num));
            }
        } else {
            next_is_test = false;
        }

        // Clear doc buffer on non-doc, non-attr lines.
        if !trimmed.starts_with('#') {
            doc_buf.clear();
        }
    }

    Ok(items)
}

fn build_item(name: String, doc_buf: &[String], line_num: u32) -> SpecItem {
    let description = if doc_buf.is_empty() {
        name_to_description(&name)
    } else {
        doc_buf.join(" ")
    };
    SpecItem {
        name,
        description,
        kind: SpecKind::RustTest,
        file: String::new(),
        line: line_num,
    }
}

fn parse_fn_name(line: &str) -> Option<String> {
    let after_fn = line
        .trim_start_matches("async ")
        .trim_start_matches("fn ")
        .trim();
    let name: String = after_fn
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Convert a snake_case test name to a readable description.
fn name_to_description(name: &str) -> String {
    name.trim_start_matches("test_").replace('_', " ")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn write_temp(content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("lopi-spec-rust-{}.rs", nonce()));
        fs::write(&path, content).unwrap();
        path
    }

    fn nonce() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static C: AtomicU64 = AtomicU64::new(0);
        C.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn finds_plain_test() {
        let f = write_temp("#[test]\nfn it_works() {}\n");
        let items = extract_rust(&f).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "it_works");
        assert_eq!(items[0].kind, SpecKind::RustTest);
    }

    #[test]
    fn finds_tokio_test() {
        let f = write_temp("#[tokio::test]\nasync fn async_test() {}\n");
        let items = extract_rust(&f).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "async_test");
    }

    #[test]
    fn captures_doc_comment_as_description() {
        let f =
            write_temp("/// Verify that addition works correctly.\n#[test]\nfn addition() {}\n");
        let items = extract_rust(&f).unwrap();
        assert_eq!(
            items[0].description,
            "Verify that addition works correctly."
        );
    }

    #[test]
    fn falls_back_to_name_as_description() {
        let f = write_temp("#[test]\nfn score_weighted_lint_penalty() {}\n");
        let items = extract_rust(&f).unwrap();
        assert_eq!(items[0].description, "score weighted lint penalty");
    }

    #[test]
    fn skips_non_test_functions() {
        let f = write_temp("fn helper() {}\nfn also_not_a_test() {}\n");
        let items = extract_rust(&f).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn multiple_tests_in_file() {
        let f = write_temp("#[test]\nfn a() {}\n#[test]\nfn b() {}\n#[test]\nfn c() {}\n");
        let items = extract_rust(&f).unwrap();
        assert_eq!(items.len(), 3);
        let names: Vec<_> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn records_line_numbers() {
        let f = write_temp("#[test]\nfn first() {}\n\n#[test]\nfn second() {}\n");
        let items = extract_rust(&f).unwrap();
        assert_eq!(items[0].line, 2);
        assert_eq!(items[1].line, 5);
    }

    #[test]
    fn empty_file_returns_empty() {
        let f = write_temp("");
        assert!(extract_rust(&f).unwrap().is_empty());
    }
}
