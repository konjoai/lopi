use anyhow::{bail, Result};
use glob::Pattern;

/// Validates changed file paths against configured allow and deny glob patterns.
pub struct DiffChecker {
    allowed: Vec<Pattern>,
    forbidden: Vec<Pattern>,
    raw_allowed: Vec<String>,
    raw_forbidden: Vec<String>,
}

impl DiffChecker {
    /// Construct a checker with the given allowed and forbidden glob patterns.
    #[must_use]
    pub fn new(allowed: Vec<String>, forbidden: Vec<String>) -> Self {
        let allowed_p = allowed.iter().filter_map(|s| compile(s)).collect();
        let forbidden_p = forbidden.iter().filter_map(|s| compile(s)).collect();
        Self {
            allowed: allowed_p,
            forbidden: forbidden_p,
            raw_allowed: allowed,
            raw_forbidden: forbidden,
        }
    }

    /// Returns Ok if every path is inside an allowed dir/glob and not inside any forbidden one.
    ///
    /// # Errors
    /// Returns `Err` if any path touches a forbidden directory or lies outside the allowed scope.
    pub fn validate(&self, paths: &[String]) -> Result<()> {
        for p in paths {
            if self.is_forbidden(p) {
                bail!("diff touches forbidden path: {p}");
            }
            if !self.is_allowed(p) {
                bail!(
                    "diff touches path outside allowed scope: {p} (allowed: {:?})",
                    self.raw_allowed
                );
            }
        }
        Ok(())
    }

    fn is_allowed(&self, p: &str) -> bool {
        if self.allowed.is_empty() && self.raw_allowed.is_empty() {
            return true;
        }
        self.allowed.iter().any(|pat| pat.matches(p))
            || self
                .raw_allowed
                .iter()
                .any(|prefix| path_has_prefix(p, prefix))
    }

    fn is_forbidden(&self, p: &str) -> bool {
        self.forbidden.iter().any(|pat| pat.matches(p))
            || self
                .raw_forbidden
                .iter()
                .any(|prefix| path_has_prefix(p, prefix))
    }
}

/// True when `p` is exactly `prefix`, or lies inside the `prefix` directory —
/// a plain `p.starts_with(prefix)` would incorrectly treat `"src2/evil.rs"`
/// as inside `"src"`, since `"src2/..."` textually starts with `"src"` with
/// no path-separator boundary between them.
fn path_has_prefix(p: &str, prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches('/');
    p == prefix || p.starts_with(&format!("{prefix}/"))
}

fn compile(s: &str) -> Option<Pattern> {
    // Treat trailing-slash dir prefixes as `prefix/**`.
    let pat = if s.ends_with('/') {
        format!("{s}**")
    } else {
        s.to_string()
    };
    Pattern::new(&pat).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_src_paths() {
        let c = DiffChecker::new(vec!["src/".into()], vec![".github/".into()]);
        assert!(c.validate(&["src/main.rs".into()]).is_ok());
    }

    #[test]
    fn rejects_forbidden() {
        let c = DiffChecker::new(vec!["src/".into()], vec![".github/".into()]);
        assert!(c.validate(&[".github/workflows/ci.yml".into()]).is_err());
    }

    #[test]
    fn rejects_outside_scope() {
        let c = DiffChecker::new(vec!["src/".into()], vec![]);
        assert!(c.validate(&["infra/terraform/main.tf".into()]).is_err());
    }

    #[test]
    fn empty_allowed_and_forbidden_permits_anything() {
        let c = DiffChecker::new(vec![], vec![]);
        assert!(c.validate(&["any/path/file.rs".into()]).is_ok());
    }

    #[test]
    fn forbidden_overrides_allowed_for_same_prefix() {
        let c = DiffChecker::new(vec!["src/".into()], vec!["src/generated/".into()]);
        assert!(c.validate(&["src/generated/proto.rs".into()]).is_err());
    }

    #[test]
    fn multiple_paths_one_outside_fails_entire_batch() {
        let c = DiffChecker::new(vec!["src/".into()], vec![]);
        let paths = vec!["src/lib.rs".into(), "README.md".into()];
        assert!(c.validate(&paths).is_err());
    }

    #[test]
    fn empty_path_list_always_passes() {
        let c = DiffChecker::new(vec!["src/".into()], vec![".github/".into()]);
        assert!(c.validate(&[]).is_ok());
    }

    #[test]
    fn glob_matches_nested_paths() {
        let c = DiffChecker::new(vec!["src/".into()], vec![]);
        assert!(c.validate(&["src/a/b/deep/file.rs".into()]).is_ok());
    }

    /// Regression test: a `"src"` allow prefix (no trailing slash) must not
    /// let a sibling directory like `"src2/"` through just because it
    /// textually starts with the same characters.
    #[test]
    fn prefix_without_trailing_slash_respects_path_boundary() {
        let c = DiffChecker::new(vec!["src".into()], vec![]);
        assert!(c.validate(&["src/main.rs".into()]).is_ok());
        assert!(c.validate(&["src".into()]).is_ok());
        assert!(c.validate(&["src2/evil.rs".into()]).is_err());
        assert!(c.validate(&["src-legacy/main.rs".into()]).is_err());
    }

    /// Same boundary requirement on the forbidden side: a `"secrets"` deny
    /// prefix must not also catch `"secrets-backup/"`.
    #[test]
    fn forbidden_prefix_without_trailing_slash_respects_path_boundary() {
        let c = DiffChecker::new(vec![], vec!["secrets".into()]);
        assert!(c.validate(&["secrets/keys.pem".into()]).is_err());
        assert!(c.validate(&["secrets".into()]).is_err());
        assert!(c.validate(&["secrets-backup/keys.pem".into()]).is_ok());
    }
}
