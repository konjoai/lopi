//! GitHub identity for a local checkout — the `owner/name` behind a repo path.
//!
//! The launch dropdowns list repo *paths* (a path is the only thing that
//! identifies a run target: `CreateTaskRequest.repo` reaches `GitManager::new`
//! → `git2::Repository::open`, and lopi never clones). A path is a poor label
//! though, so each repo is decorated with the `owner`/`name` of its `origin`
//! remote, read straight off disk.
//!
//! Every failure here degrades to "unlabelled", never to "missing": a repo
//! whose config is absent, unreadable or hand-mangled loses its label, not its
//! place in the list.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// A repo as the dropdowns need it: the path to run against, plus the labelling
/// facts. Ordering, grouping and label text are the clients' business — this is
/// deliberately data, not presentation.
///
/// `Deserialize` exists only under `cfg(test)`, so the golden-fixture test can
/// prove the fixture both clients read is exactly this wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub(super) struct RepoEntry {
    /// Absolute path — the value a launch actually uses.
    pub path: String,
    /// The `origin` remote's GitHub owner. `None` when the repo has no origin,
    /// or its origin is not GitHub.
    pub owner: Option<String>,
    /// The GitHub repo name, or the directory basename when there is no GitHub
    /// origin — so every repo renders as something.
    pub name: String,
}

/// Decorate scanned repo paths with their GitHub identity.
pub(super) fn describe_repos(paths: Vec<String>) -> Vec<RepoEntry> {
    paths
        .into_iter()
        .map(|path| {
            let owner_name = origin_url(Path::new(&path))
                .as_deref()
                .and_then(github_owner_name);
            let (owner, name) = match owner_name {
                Some((owner, name)) => (Some(owner), name),
                None => (None, basename(Path::new(&path))),
            };
            RepoEntry { path, owner, name }
        })
        .collect()
}

/// The directory name — the fallback label for a repo with no GitHub origin.
fn basename(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The `origin` URL from `repo`'s git config, read straight off disk.
///
/// `git remote get-url origin` is the obvious implementation and the wrong one:
/// this list runs to the hundreds on a working machine, and one process spawn
/// per repo on every `GET /api/repos` would cost more than the scan that found
/// them. The config file is small and its format is stable.
fn origin_url(repo: &Path) -> Option<String> {
    let text = std::fs::read_to_string(git_dir(repo)?.join("config")).ok()?;
    parse_origin_url(&text)
}

/// Resolve `repo`'s git directory: `<repo>/.git` normally, and for a linked
/// worktree the shared common directory that actually holds `config`.
///
/// A worktree's `.git` is a *file* holding a `gitdir:` pointer, and the
/// directory it names carries no `config` of its own — git keeps a worktree's
/// remotes in the main repo, named by `commondir`. The scan already lists
/// worktrees (`.git` exists, as a file), so a reader that only ever opened
/// `<repo>/.git/config` would blank their owner for no visible reason.
fn git_dir(repo: &Path) -> Option<PathBuf> {
    let dot_git = repo.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let pointer = std::fs::read_to_string(&dot_git).ok()?;
    let target = pointer.trim().strip_prefix("gitdir:")?.trim();
    // Joining an absolute `target` replaces the base and a relative one resolves
    // against the worktree — exactly `Path::join`'s contract, either way.
    let worktree_dir = repo.join(target);
    let common = std::fs::read_to_string(worktree_dir.join("commondir")).ok()?;
    Some(worktree_dir.join(common.trim()))
}

/// The `[remote "origin"]` URL within a git config file's text.
///
/// Scans for the section rather than taking the first `url =`: a checkout can
/// carry `upstream` and contributor remotes too, and git does not order the
/// sections — grabbing the wrong one would file a fork under the *upstream's*
/// owner, the exact confusion that labelling by owner exists to remove.
fn parse_origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // git normalizes this header on write; compare loosely anyway, since
            // a hand-edited config may space or case it differently.
            in_origin = line.replace(' ', "").eq_ignore_ascii_case("[remote\"origin\"]");
        } else if in_origin {
            // `strip_prefix("url")` alone would also swallow `urlx = …`; the `=`
            // is what proves it's the key rather than a prefix of one.
            if let Some(rest) = line.strip_prefix("url") {
                if let Some(value) = rest.trim_start().strip_prefix('=') {
                    return Some(value.trim().to_string());
                }
            }
        }
    }
    None
}

/// Split a URL into its authority and path, for either form git writes:
/// `scheme://authority/path`, or scp-like `user@host:path` (no scheme, and the
/// first `:` — not `/` — ends the authority).
fn split_authority(url: &str) -> Option<(&str, &str)> {
    match url.split_once("://") {
        Some((_scheme, rest)) => rest.split_once('/'),
        None => url.split_once(':'),
    }
}

/// The host of an authority, minus any `user[:pass]@` prefix and `:port` suffix.
fn host_of(authority: &str) -> &str {
    // `rsplit_once`, because a password may itself contain '@'.
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    host.split_once(':').map_or(host, |(h, _)| h)
}

/// The `owner`/`name` of a GitHub `origin` URL, or `None` for any other host.
///
/// Handles every form git writes: scp-like SSH (`git@github.com:o/n.git`),
/// `ssh://`/`git://`/`https://`, with or without the `.git` suffix, credentials,
/// a port, or a trailing slash.
///
/// The host is matched exactly rather than by `contains("github.com")`, and the
/// path must be exactly two segments. Both guards are load-bearing on real
/// data: a tree holding `https://huggingface.co/datasets/roneneldan/TinyStories`
/// would otherwise be filed under a GitHub owner `roneneldan` that does not
/// exist. The `.git` suffix is stripped from the *end* only — `foo.github.io.git`
/// is the repo `foo.github.io`.
fn github_owner_name(url: &str) -> Option<(String, String)> {
    let (authority, path) = split_authority(url.trim())?;
    if !host_of(authority).eq_ignore_ascii_case("github.com") {
        return None;
    }
    let mut segments = path.trim_end_matches('/').split('/');
    let owner = segments.next()?;
    let name = segments.next()?;
    if segments.next().is_some() || owner.is_empty() || name.is_empty() {
        return None;
    }
    let name = name.strip_suffix(".git").unwrap_or(name);
    if name.is_empty() {
        return None;
    }
    Some((owner.to_string(), name.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Every URL form git writes, plus the two shapes this operator's real tree
    /// proved a naive parser gets wrong.
    #[test]
    fn github_urls_parse_to_owner_and_name() {
        let cases = [
            ("git@github.com:konjoai/lopi.git", Some(("konjoai", "lopi"))),
            ("https://github.com/konjoai/lopi.git", Some(("konjoai", "lopi"))),
            ("https://github.com/konjoai/lopi", Some(("konjoai", "lopi"))),
            ("https://github.com/konjoai/lopi/", Some(("konjoai", "lopi"))),
            ("ssh://git@github.com/konjoai/lopi.git", Some(("konjoai", "lopi"))),
            ("ssh://git@github.com:22/konjoai/lopi.git", Some(("konjoai", "lopi"))),
            ("git://github.com/konjoai/lopi.git", Some(("konjoai", "lopi"))),
            ("https://user:p@ss@github.com/konjoai/lopi.git", Some(("konjoai", "lopi"))),
            ("https://GitHub.com/konjoai/lopi.git", Some(("konjoai", "lopi"))),
            // `.git` comes off the end only — the name is genuinely dotted.
            (
                "https://github.com/wesleyscholl/wesleyscholl.github.io.git",
                Some(("wesleyscholl", "wesleyscholl.github.io")),
            ),
            // A real entry in this operator's tree: three path segments on a
            // non-GitHub host. "last two segments win" would invent the GitHub
            // owner `roneneldan`.
            ("https://huggingface.co/datasets/roneneldan/TinyStories", None),
            // Host is matched exactly, never by substring.
            ("https://github.com.evil.test/konjoai/lopi.git", None),
            ("https://notgithub.com/konjoai/lopi.git", None),
            // Too few / too many segments.
            ("https://github.com/konjoai", None),
            ("https://github.com/a/b/c", None),
            ("https://github.com", None),
            ("", None),
        ];
        for (url, want) in cases {
            let got = github_owner_name(url);
            let want = want.map(|(o, n)| (o.to_string(), n.to_string()));
            assert_eq!(got, want, "parsing {url:?}");
        }
    }

    #[test]
    fn origin_section_wins_over_other_remotes() {
        // `upstream` deliberately precedes `origin`: git does not order sections,
        // and taking the first `url =` would file this fork under the upstream.
        let config = r#"
[core]
	bare = false
[remote "upstream"]
	url = https://github.com/upstream-org/thing.git
	fetch = +refs/heads/*:refs/remotes/upstream/*
[remote "origin"]
	url = https://github.com/wesleyscholl/thing.git
	fetch = +refs/heads/*:refs/remotes/origin/*
[branch "main"]
	remote = origin
"#;
        assert_eq!(
            parse_origin_url(config).as_deref(),
            Some("https://github.com/wesleyscholl/thing.git")
        );
    }

    #[test]
    fn a_key_merely_starting_with_url_is_not_the_url() {
        let config = "[remote \"origin\"]\n\turlx = https://github.com/a/b.git\n";
        assert_eq!(parse_origin_url(config), None);
    }

    #[test]
    fn missing_origin_section_yields_none() {
        let config = "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = https://github.com/o/n.git\n";
        assert_eq!(parse_origin_url(config), None);
    }

    /// A repo with no GitHub identity keeps its path and falls back to its
    /// directory name — losing a label must never lose a repo.
    #[test]
    fn unlabelled_repos_survive_with_a_basename() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("no-origin-repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git").join("config"), "[core]\n\tbare = false\n").unwrap();

        let got = describe_repos(vec![repo.display().to_string()]);

        assert_eq!(got.len(), 1, "the repo is still listed");
        assert_eq!(got[0].owner, None);
        assert_eq!(got[0].name, "no-origin-repo", "falls back to the directory name");
        assert_eq!(got[0].path, repo.display().to_string(), "the path is untouched");
    }

    /// The golden fixture's `repos` array must be exactly what this server
    /// emits — it is the input both clients' rule tests build their expected
    /// rows from (`web/src/lib/stores/repoMenu.test.ts`,
    /// `macos/LopiTests/RepoMenuTests.swift`). If a field is renamed here and
    /// the fixture isn't updated, both surfaces would drift together and their
    /// own tests would still pass. `deny_unknown_fields` + the re-serialize
    /// round-trip is what makes that impossible.
    #[test]
    fn golden_fixture_matches_the_wire_shape() {
        let raw = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/repo_menu_golden.json"
        ))
        .expect("golden fixture is readable");
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("golden fixture is JSON");

        let repos_json = doc.get("repos").expect("fixture has a `repos` array");
        let entries: Vec<RepoEntry> =
            serde_json::from_value(repos_json.clone()).expect("`repos` decodes as RepoEntry");

        assert_eq!(
            serde_json::to_value(&entries).unwrap(),
            *repos_json,
            "the fixture round-trips through RepoEntry unchanged"
        );
        assert!(
            entries.iter().any(|e| e.owner.is_none()),
            "the fixture must keep covering a repo with no GitHub identity"
        );
    }

    /// A linked worktree's `.git` is a file pointing at a gitdir that holds no
    /// `config` of its own — the remotes live in the main repo, via `commondir`.
    #[test]
    fn linked_worktree_resolves_its_owner_through_commondir() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("squish");
        let wt_meta = main.join(".git").join("worktrees").join("feature");
        std::fs::create_dir_all(&wt_meta).unwrap();
        std::fs::write(
            main.join(".git").join("config"),
            "[remote \"origin\"]\n\turl = https://github.com/konjoai/squish.git\n",
        )
        .unwrap();
        // `commondir` points back to the main repo's `.git`.
        std::fs::write(wt_meta.join("commondir"), "../..\n").unwrap();

        let worktree = tmp.path().join("squish-feature");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", wt_meta.display()),
        )
        .unwrap();

        let got = describe_repos(vec![worktree.display().to_string()]);

        assert_eq!(got[0].owner.as_deref(), Some("konjoai"));
        assert_eq!(got[0].name, "squish");
        // ...and it is a DIFFERENT run target from the main checkout, despite
        // resolving to the same owner/name. The clients disambiguate the label;
        // the path is what keeps them apart.
        assert_eq!(got[0].path, worktree.display().to_string());
    }
}
