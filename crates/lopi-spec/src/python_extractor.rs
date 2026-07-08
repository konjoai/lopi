//! Extract spec items from Python test files (pytest-style).
//!
//! Parses line-by-line for `def test_*` and `async def test_*` at any
//! indent level (class-level test methods included). Captures preceding
//! docstrings (triple-quoted on the next line) as the description.

use anyhow::Result;
use std::path::Path;

use crate::{name_to_description, SpecItem, SpecKind};

/// Extract all test functions from a Python source file.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub fn extract_python(path: impl AsRef<Path>) -> Result<Vec<SpecItem>> {
    let source = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = source.lines().collect();
    let mut items = Vec::new();

    for (idx, &line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !is_test_def(trimmed) {
            continue;
        }
        let line_num = (idx + 1) as u32;
        let Some(name) = parse_py_fn_name(trimmed) else {
            continue;
        };

        // Look for a docstring on the next non-empty line.
        let description = lines
            .get(idx + 1)
            .map(|l| l.trim())
            .and_then(parse_docstring)
            .unwrap_or_else(|| name_to_description(&name));

        items.push(SpecItem {
            name,
            description,
            kind: SpecKind::PythonTest,
            file: String::new(),
            line: line_num,
        });
    }

    Ok(items)
}

fn is_test_def(line: &str) -> bool {
    let stripped = line.trim_start_matches("async").trim();
    stripped.starts_with("def test_") || stripped.starts_with("def test ")
}

fn parse_py_fn_name(line: &str) -> Option<String> {
    let after_def = line
        .trim_start_matches("async ")
        .trim_start_matches("def ")
        .trim();
    let name: String = after_def
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn parse_docstring(line: &str) -> Option<String> {
    for q in &[r#"""""#, "'''"] {
        if line.starts_with(q) {
            let inner = line.trim_start_matches(q).trim_end_matches(q).trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn write_temp(content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("lopi-spec-py-{}.py", nonce()));
        fs::write(&path, content).unwrap();
        path
    }

    fn nonce() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static C: AtomicU64 = AtomicU64::new(0);
        C.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn finds_simple_test() {
        let f = write_temp("def test_hello():\n    pass\n");
        let items = extract_python(&f).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "test_hello");
        assert_eq!(items[0].kind, SpecKind::PythonTest);
    }

    #[test]
    fn finds_async_test() {
        let f = write_temp("async def test_async_fetch():\n    pass\n");
        let items = extract_python(&f).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "test_async_fetch");
    }

    #[test]
    fn captures_inline_docstring() {
        let f = write_temp(
            "def test_addition():\n    \"\"\"Addition returns the correct sum.\"\"\"\n    pass\n",
        );
        let items = extract_python(&f).unwrap();
        assert_eq!(items[0].description, "Addition returns the correct sum.");
    }

    #[test]
    fn falls_back_to_name_description() {
        let f = write_temp("def test_score_weighted():\n    pass\n");
        let items = extract_python(&f).unwrap();
        assert_eq!(items[0].description, "score weighted");
    }

    #[test]
    fn skips_non_test_functions() {
        let f = write_temp("def helper():\n    pass\ndef setup():\n    pass\n");
        assert!(extract_python(&f).unwrap().is_empty());
    }

    #[test]
    fn class_method_tests() {
        let f = write_temp("class TestFoo:\n    def test_bar(self):\n        pass\n");
        let items = extract_python(&f).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "test_bar");
    }

    #[test]
    fn multiple_tests() {
        let f = write_temp("def test_a():\n    pass\ndef test_b():\n    pass\n");
        let items = extract_python(&f).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn line_numbers_recorded() {
        let f = write_temp("def test_first():\n    pass\n\ndef test_second():\n    pass\n");
        let items = extract_python(&f).unwrap();
        assert_eq!(items[0].line, 1);
        assert_eq!(items[1].line, 4);
    }
}
