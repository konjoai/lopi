//! lopi-spec — spec surface extractor.
//!
//! Scans a repository's test files to produce a machine-readable inventory
//! of what the repo claims to do. The spec surface is the ground truth for
//! the self-improvement loop: coverage gap detection, synthetic user
//! exercise, and planning context injection all operate against it.
//!
//! ## What counts as a spec item
//! - **Rust**: `#[test]`, `#[tokio::test]`, `#[async_std::test]` functions.
//!   The item name is the function name; the description is extracted from
//!   the preceding doc comment if present.
//! - **Python**: `def test_*` or `async def test_*` functions. Class-level
//!   `TestCase` methods are also captured.
//!
//! ## Output
//! `SpecSurface` is JSON-serialisable and can be written to
//! `.lopi/spec_surface.json` for caching. The planning prompt injector in
//! `lopi-agent` reads this file to seed the task context.

mod python_extractor;
mod rust_extractor;
pub mod test_runner;

pub use python_extractor::extract_python;
pub use rust_extractor::extract_rust;
pub use test_runner::{coverage_gaps, run_tests, TestRunResult};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A single verifiable claim extracted from the test suite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecItem {
    /// Short identifier: the test function name.
    pub name: String,
    /// One-line description extracted from a doc comment, or derived from the name.
    pub description: String,
    /// Kind of spec item.
    pub kind: SpecKind,
    /// Relative path from repo root to the file containing this test.
    pub file: String,
    /// Line number of the test function definition.
    pub line: u32,
}

/// The category of spec evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpecKind {
    /// `#[test]` or `#[tokio::test]` in Rust.
    RustTest,
    /// `def test_*` in Python (pytest-style).
    PythonTest,
}

impl SpecKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RustTest => "rust_test",
            Self::PythonTest => "python_test",
        }
    }
}

/// The complete spec surface for a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecSurface {
    pub repo_path: String,
    pub items: Vec<SpecItem>,
    pub extracted_at: DateTime<Utc>,
    pub rust_files_scanned: u32,
    pub python_files_scanned: u32,
}

impl SpecSurface {
    /// Extract the spec surface from all test files under `repo_path`.
    ///
    /// # Errors
    ///
    /// Returns an error only if the root path cannot be read. Individual
    /// file parse errors are logged as warnings and skipped.
    pub fn extract(repo_path: impl AsRef<Path>) -> Result<Self> {
        let root = repo_path.as_ref();
        let mut items = Vec::new();
        let mut rust_count = 0u32;
        let mut python_count = 0u32;

        for entry in walkdir(root) {
            if should_skip(&entry) {
                continue;
            }
            let rel = entry.strip_prefix(root).unwrap_or(&entry).to_string_lossy().to_string();
            let (found, rc, pc) = scan_entry(&entry, &rel);
            items.extend(found);
            rust_count += rc;
            python_count += pc;
        }

        items.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        Ok(Self {
            repo_path: root.to_string_lossy().to_string(),
            items,
            extracted_at: Utc::now(),
            rust_files_scanned: rust_count,
            python_files_scanned: python_count,
        })
    }

    /// Total number of spec items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when no spec items were found.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Write to `.lopi/spec_surface.json` under `repo_path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, repo_path: impl AsRef<Path>) -> Result<PathBuf> {
        let dir = repo_path.as_ref().join(".lopi");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("spec_surface.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Load from `.lopi/spec_surface.json`. Returns None if the file does not exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be parsed.
    pub fn load(repo_path: impl AsRef<Path>) -> Result<Option<Self>> {
        let path = repo_path.as_ref().join(".lopi").join("spec_surface.json");
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&raw)?))
    }

    /// Return the top `n` items as single-line strings for prompt injection.
    pub fn top_descriptions(&self, n: usize) -> Vec<String> {
        self.items
            .iter()
            .take(n)
            .map(|i| format!("[{}] {}", i.kind.as_str(), i.description))
            .collect()
    }
}

/// Dispatch a single file to the correct extractor.
///
/// Returns (items, rust_increment, python_increment).
fn scan_entry(entry: &Path, rel: &str) -> (Vec<SpecItem>, u32, u32) {
    match entry.extension().and_then(|e| e.to_str()) {
        Some("rs") => {
            let found = match extract_rust(entry) {
                Ok(v) => v.into_iter().map(|mut i| { i.file = rel.to_string(); i }).collect(),
                Err(e) => { tracing::warn!(file = %rel, "spec extract error: {e}"); vec![] }
            };
            (found, 1, 0)
        }
        Some("py") => {
            let found = match extract_python(entry) {
                Ok(v) => v.into_iter().map(|mut i| { i.file = rel.to_string(); i }).collect(),
                Err(e) => { tracing::warn!(file = %rel, "spec extract error: {e}"); vec![] }
            };
            (found, 0, 1)
        }
        _ => (vec![], 0, 0),
    }
}

/// Walk a directory, yielding `.rs` and `.py` files. Skips hidden dirs and
/// common non-source directories.
fn walkdir(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    collect_files(root, &mut result);
    result
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if should_skip(&path) { continue; }
            collect_files(&path, out);
        } else if path.is_file() && !should_skip(&path) {
            out.push(path);
        }
    }
}

fn should_skip(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return true };
    // Skip hidden, build artifacts, and vendored code.
    name.starts_with('.')
        || matches!(name, "target" | "node_modules" | "vendor" | "__pycache__" | "dist" | "build")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    fn write_temp(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn extract_empty_dir_returns_empty_surface() {
        let dir = tempdir();
        let surface = SpecSurface::extract(&dir).unwrap();
        assert!(surface.is_empty());
        assert_eq!(surface.rust_files_scanned, 0);
        assert_eq!(surface.python_files_scanned, 0);
    }

    #[test]
    fn extract_rust_test_functions() {
        let dir = tempdir();
        write_temp(&dir, "lib.rs", "#[test]\nfn it_works() {}\n#[test]\nfn another() {}\n");
        let surface = SpecSurface::extract(&dir).unwrap();
        assert_eq!(surface.items.len(), 2);
        assert_eq!(surface.rust_files_scanned, 1);
        let names: Vec<_> = surface.items.iter().map(|i| &i.name).collect();
        assert!(names.contains(&&"it_works".to_string()));
        assert!(names.contains(&&"another".to_string()));
    }

    #[test]
    fn extract_python_test_functions() {
        let dir = tempdir();
        write_temp(&dir, "test_foo.py", "def test_hello():\n    pass\n\ndef test_world():\n    pass\n");
        let surface = SpecSurface::extract(&dir).unwrap();
        assert_eq!(surface.items.len(), 2);
        assert_eq!(surface.python_files_scanned, 1);
    }

    #[test]
    fn extract_skips_target_dir() {
        let dir = tempdir();
        let target = dir.join("target");
        fs::create_dir(&target).unwrap();
        write_temp(&target, "lib.rs", "#[test]\nfn should_be_skipped() {}\n");
        let surface = SpecSurface::extract(&dir).unwrap();
        assert!(surface.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempdir();
        write_temp(&dir, "lib.rs", "#[test]\nfn roundtrip_test() {}\n");
        let surface = SpecSurface::extract(&dir).unwrap();
        surface.save(&dir).unwrap();
        let loaded = SpecSurface::load(&dir).unwrap().unwrap();
        assert_eq!(loaded.items.len(), surface.items.len());
        assert_eq!(loaded.items[0].name, surface.items[0].name);
    }

    #[test]
    fn load_returns_none_when_no_cache() {
        let dir = tempdir();
        assert!(SpecSurface::load(&dir).unwrap().is_none());
    }

    #[test]
    fn top_descriptions_caps_at_n() {
        let dir = tempdir();
        // Write 15 tests
        let content = (0..15).map(|i| format!("#[test]\nfn test_{i}() {{}}\n")).collect::<String>();
        write_temp(&dir, "lib.rs", &content);
        let surface = SpecSurface::extract(&dir).unwrap();
        assert_eq!(surface.top_descriptions(5).len(), 5);
        assert_eq!(surface.top_descriptions(100).len(), 15);
    }

    #[test]
    fn is_empty_and_len_consistent() {
        let dir = tempdir();
        let empty = SpecSurface::extract(&dir).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        write_temp(&dir, "t.rs", "#[test]\nfn foo() {}\n");
        let with_item = SpecSurface::extract(&dir).unwrap();
        assert!(!with_item.is_empty());
        assert_eq!(with_item.len(), 1);
    }

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("lopi-spec-test-{pid}-{id}"));
        if path.exists() { std::fs::remove_dir_all(&path).unwrap(); }
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
