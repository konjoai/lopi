use anyhow::{bail, Result};
use glob::Pattern;

pub struct DiffChecker {
    allowed: Vec<Pattern>,
    forbidden: Vec<Pattern>,
    raw_allowed: Vec<String>,
    raw_forbidden: Vec<String>,
}

impl DiffChecker {
    pub fn new(allowed: Vec<String>, forbidden: Vec<String>) -> Self {
        let allowed_p = allowed.iter().filter_map(|s| compile(s)).collect();
        let forbidden_p = forbidden.iter().filter_map(|s| compile(s)).collect();
        Self { allowed: allowed_p, forbidden: forbidden_p, raw_allowed: allowed, raw_forbidden: forbidden }
    }

    /// Returns Ok if every path is inside an allowed dir/glob and not inside any forbidden one.
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
            || self.raw_allowed.iter().any(|prefix| p.starts_with(prefix))
    }

    fn is_forbidden(&self, p: &str) -> bool {
        self.forbidden.iter().any(|pat| pat.matches(p))
            || self.raw_forbidden.iter().any(|prefix| p.starts_with(prefix))
    }
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
}
