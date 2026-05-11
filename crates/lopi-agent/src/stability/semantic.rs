//! Semantic diff checker — flags files modified by a patch that fall outside
//! the task's declared allowed directories.
//!
//! This is Layer 5 check 7: "does the patch touch files not mentioned in the
//! task description?" A clean patch only modifies files inside `allowed_dirs`.
//! Any file outside that set is flagged and logged to the stability ledger.
//!
//! The check is intentionally conservative: if `allowed_dirs` is empty we
//! return no flags (the task has no declared scope, so any file is fair game).

/// Parse unified diff output and return all modified file paths.
///
/// Matches `+++ b/<path>` lines from `git diff` output. The `b/` prefix
/// is always present in git diffs for the new version of a modified file;
/// `/dev/null` appears for deleted files (excluded from the output).
fn diff_file_paths(diff: &str) -> Vec<String> {
    diff.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("+++ b/") {
                return Some(rest.to_string());
            }
            None
        })
        .filter(|path| path != "/dev/null")
        .collect()
}

/// Check whether `path` is covered by any entry in `allowed_dirs`.
///
/// A path is covered if it starts with `dir` (with the trailing `/` stripped
/// from the pattern). Exact matches are also accepted (e.g. `src/lib.rs`
/// against `src/lib.rs`).
fn covered(path: &str, allowed_dirs: &[String]) -> bool {
    allowed_dirs.iter().any(|dir| {
        let prefix = dir.trim_end_matches('/');
        path == prefix || path.starts_with(&format!("{prefix}/"))
    })
}

/// Return the list of files in `diff` that are **not** covered by any entry
/// in `allowed_dirs`.
///
/// Empty `allowed_dirs` → no flags (no declared scope = no violation).
/// Empty diff → no flags.
pub fn flag_out_of_scope(diff: &str, allowed_dirs: &[String]) -> Vec<String> {
    if allowed_dirs.is_empty() {
        return vec![];
    }
    diff_file_paths(diff)
        .into_iter()
        .filter(|path| !covered(path, allowed_dirs))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const SAMPLE_DIFF: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
index abc..def 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,4 @@
-fn old() {}
+fn new() {}
diff --git a/Cargo.toml b/Cargo.toml
index 111..222 100644
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,2 +1,3 @@
+version = \"0.2.0\"
diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml
index 333..444 100644
--- a/.github/workflows/ci.yml
+++ b/.github/workflows/ci.yml
@@ -5,3 +5,4 @@
+    - run: cargo test
";

    #[test]
    fn flags_file_outside_allowed() {
        let allowed = vec!["src/".to_string()];
        let flags = flag_out_of_scope(SAMPLE_DIFF, &allowed);
        assert!(flags.contains(&"Cargo.toml".to_string()), "{flags:?}");
        assert!(
            flags.contains(&".github/workflows/ci.yml".to_string()),
            "{flags:?}"
        );
        assert!(!flags.contains(&"src/lib.rs".to_string()), "{flags:?}");
    }

    #[test]
    fn no_flags_when_all_in_allowed() {
        let allowed = vec![
            "src/".to_string(),
            "Cargo.toml".to_string(),
            ".github/".to_string(),
        ];
        let flags = flag_out_of_scope(SAMPLE_DIFF, &allowed);
        assert!(flags.is_empty(), "expected no flags, got {flags:?}");
    }

    #[test]
    fn no_flags_when_allowed_empty() {
        let flags = flag_out_of_scope(SAMPLE_DIFF, &[]);
        assert!(
            flags.is_empty(),
            "empty allowed_dirs → no scope declared → no flags"
        );
    }

    #[test]
    fn empty_diff_produces_no_flags() {
        let allowed = vec!["src/".to_string()];
        assert!(flag_out_of_scope("", &allowed).is_empty());
    }

    #[test]
    fn exact_path_match_is_covered() {
        let diff = "+++ b/Cargo.toml\n";
        let allowed = vec!["Cargo.toml".to_string()];
        let flags = flag_out_of_scope(diff, &allowed);
        assert!(flags.is_empty(), "exact path match should not be flagged");
    }

    #[test]
    fn nested_path_covered_by_dir_prefix() {
        let diff = "+++ b/crates/lopi-agent/src/runner.rs\n";
        let allowed = vec!["crates/".to_string()];
        let flags = flag_out_of_scope(diff, &allowed);
        assert!(flags.is_empty(), "nested path should be covered by prefix");
    }

    #[test]
    fn partial_prefix_does_not_match() {
        // "src" should not cover "src2/lib.rs"
        let diff = "+++ b/src2/lib.rs\n";
        let allowed = vec!["src/".to_string()];
        let flags = flag_out_of_scope(diff, &allowed);
        assert!(flags.contains(&"src2/lib.rs".to_string()), "{flags:?}");
    }
}
