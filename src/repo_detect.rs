//! Git-root auto-detection for the REPL and quick-run invocations.
//!
//! Walks upward from the current working directory looking for a `.git`
//! directory. Returns the nearest ancestor that contains one, falling back
//! to the current directory if none is found.
use std::path::PathBuf;

/// Walk up from `start` to find the nearest directory containing `.git`.
/// Returns `start` if no git root is found.
pub fn find_git_root(start: &std::path::Path) -> PathBuf {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return current;
        }
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None => return start.to_path_buf(),
        }
    }
}

/// Auto-detect the repo root for the REPL or quick-run.
///
/// 1. If cwd is inside a git repo → return the git root.
/// 2. Otherwise → return cwd.
pub fn detect_repo() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    find_git_root(&cwd)
}

/// Short display name for the repo: the directory's file name component.
pub fn repo_display_name(repo: &std::path::Path) -> String {
    repo.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".")
        .to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir(suffix: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("lopi-repo-detect-{suffix}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn finds_git_root_in_parent() {
        let root = tmp_dir("parent");
        fs::create_dir(root.join(".git")).unwrap();
        let subdir = root.join("src").join("lib");
        fs::create_dir_all(&subdir).unwrap();

        let found = find_git_root(&subdir);
        assert_eq!(found, root);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn returns_start_when_no_git_root() {
        let dir = tmp_dir("nogit");
        let found = find_git_root(&dir);
        assert_eq!(found, dir);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn repo_display_name_extracts_last_segment() {
        let p = PathBuf::from("/home/user/projects/myapp");
        assert_eq!(repo_display_name(&p), "myapp");
    }
}
