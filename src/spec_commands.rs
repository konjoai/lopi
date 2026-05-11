//! `lopi spec` and `lopi check` command handlers.
//!
//! spec   — extract the spec surface from a repo's test files.
//! check  — run KCQF quality analysis and report violations.

use anyhow::Result;
use lopi_spec::SpecSurface;
use std::path::{Path, PathBuf};

/// `lopi spec [--repo .] [--export]`
///
/// Walks test files under `repo`, extracts spec items, prints a summary,
/// and optionally writes `.lopi/spec_surface.json`.
pub async fn run_spec(repo: PathBuf, export: bool, save: bool) -> Result<()> {
    println!("🔍 lopi spec — scanning {}", repo.display());
    let surface = SpecSurface::extract(&repo)?;
    println!(
        "   {} spec items — {} Rust files · {} Python files",
        surface.len(),
        surface.rust_files_scanned,
        surface.python_files_scanned,
    );
    println!();

    if surface.is_empty() {
        println!("  No test functions found. Add #[test] or def test_* to define the spec.");
        return Ok(());
    }

    if export {
        println!("{}", serde_json::to_string_pretty(&surface)?);
        return Ok(());
    }

    if save {
        let path = surface.save(&repo)?;
        println!("   cached → {}", path.display());
        println!();
    }

    // Print a table.
    let w = 60usize;
    println!("  {:<8}  {:<12}  {:<w$}  File", "Line", "Kind", "Description");
    println!("  {}", "─".repeat(8 + 2 + 12 + 2 + w + 2 + 40));
    for item in &surface.items {
        let desc = if item.description.len() > w {
            format!("{}…", &item.description[..w - 1])
        } else {
            item.description.clone()
        };
        println!(
            "  {:>8}  {:<12}  {:<w$}  {}",
            item.line,
            item.kind.as_str(),
            desc,
            item.file
        );
    }
    Ok(())
}

/// `lopi check [--repo .]`
///
/// Runs KCQF quality analysis: file size violations, and spec coverage
/// (whether a cached spec surface exists and all prior tests still appear).
pub async fn run_check(repo: PathBuf) -> Result<()> {
    println!("🔎 lopi check — {}", repo.display());
    println!();

    // 1. File-size gate.
    let violations = check_file_sizes(&repo);
    if violations.is_empty() {
        println!("  ✅ file size gate — all files within limits");
    } else {
        println!("  ⚠️  file size violations ({}):", violations.len());
        for v in &violations {
            println!("     {} — {} lines (limit 500)", v.0, v.1);
        }
    }
    println!();

    // 2. Spec surface.
    match SpecSurface::load(&repo)? {
        Some(cached) => {
            let live = SpecSurface::extract(&repo)?;
            compare_spec_surfaces(&cached, &live);
        }
        None => {
            let live = SpecSurface::extract(&repo)?;
            println!(
                "  📋 spec surface — not cached. {} item(s) found.",
                live.len()
            );
            println!("     Run `lopi spec --save` to establish the baseline.");
        }
    }
    println!();

    if violations.is_empty() {
        println!("✅ lopi check passed");
    } else {
        println!("⚠️  lopi check: {} file-size violation(s)", violations.len());
    }
    Ok(())
}

fn compare_spec_surfaces(cached: &SpecSurface, live: &SpecSurface) {
    let cached_names: std::collections::HashSet<_> = cached.items.iter().map(|i| &i.name).collect();
    let live_names: std::collections::HashSet<_> = live.items.iter().map(|i| &i.name).collect();
    let added: Vec<_> = live_names.difference(&cached_names).collect();
    let removed: Vec<_> = cached_names.difference(&live_names).collect();
    println!("  📋 spec surface — {} cached · {} live", cached.items.len(), live.items.len());
    if !added.is_empty() {
        println!("     + {} new test(s)", added.len());
    }
    if !removed.is_empty() {
        println!("     - {} removed test(s) (spec regression risk)", removed.len());
        for name in &removed {
            println!("       - {name}");
        }
    }
    if added.is_empty() && removed.is_empty() {
        println!("     ✅ spec stable — no additions or removals");
    }
}

/// Returns (relative_path, line_count) for every Rust/Python source file
/// that exceeds the 500-line budget.
fn check_file_sizes(repo: &Path) -> Vec<(String, usize)> {
    let mut violations = Vec::new();
    collect_size_violations(repo, repo, &mut violations);
    violations.sort();
    violations
}

fn collect_size_violations(root: &Path, dir: &Path, out: &mut Vec<(String, usize)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if matches!(name, "target" | "node_modules" | ".git" | "vendor" | ".claude") {
                continue;
            }
            collect_size_violations(root, &path, out);
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("rs" | "py")) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let lines = content.lines().count();
                if lines > 500 {
                    let rel = path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    out.push((rel, lines));
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static C: AtomicU64 = AtomicU64::new(0);
        let id = C.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let p = std::env::temp_dir().join(format!("lopi-check-test-{pid}-{id}"));
        if p.exists() { fs::remove_dir_all(&p).unwrap(); }
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn no_violations_on_empty_dir() {
        let dir = tempdir();
        assert!(check_file_sizes(&dir).is_empty());
    }

    #[test]
    fn detects_oversized_rs_file() {
        let dir = tempdir();
        let content = "fn x() {}\n".repeat(501);
        fs::write(dir.join("big.rs"), content).unwrap();
        let v = check_file_sizes(&dir);
        assert_eq!(v.len(), 1);
        assert!(v[0].0.ends_with("big.rs"));
        assert!(v[0].1 > 500);
    }

    #[test]
    fn small_file_no_violation() {
        let dir = tempdir();
        fs::write(dir.join("small.rs"), "#[test]\nfn ok() {}\n").unwrap();
        assert!(check_file_sizes(&dir).is_empty());
    }

    #[test]
    fn skips_target_dir() {
        let dir = tempdir();
        let target = dir.join("target");
        fs::create_dir(&target).unwrap();
        let content = "fn x() {}\n".repeat(600);
        fs::write(target.join("big.rs"), content).unwrap();
        assert!(check_file_sizes(&dir).is_empty());
    }
}
