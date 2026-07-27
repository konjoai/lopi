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
    // pytest's default collection only matches `test_*` (python_functions =
    // "test_*"); a bare `def test (...)` never actually runs as a pytest
    // test, so matching it here just produced a permanent, unfillable
    // coverage gap.
    let stripped = line.trim_start_matches("async").trim();
    stripped.starts_with("def test_")
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
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// A per-process atomic nonce used to be enough to keep concurrent
    /// `write_temp` calls from colliding on the same path — true under
    /// `cargo test`'s threaded parallelism, false under `cargo nextest`'s
    /// one-process-per-test model, where every test's nonce counter starts
    /// back at 0. Two unrelated tests running in concurrent processes could
    /// both compute nonce 0 and race on `/tmp/lopi-spec-py-0.py`, one
    /// clobbering the other's content before it was read (confirmed live:
    /// CI failure under `cargo nextest run`, the actual G2 test runner —
    /// see `rust_extractor.rs`'s identical fix, applied here to match).
    /// `tempfile::NamedTempFile` sidesteps this — genuinely unique per call,
    /// any runner, any process model — and self-deletes on drop.
    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
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
