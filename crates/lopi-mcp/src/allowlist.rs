//! Deny-by-default allowlist gating which MCP servers [`McpServerSpec::connect`]
//! is willing to spawn.
//!
//! Sprint S10, Phase 5 — `.lopi/loop.toml`'s `[[mcp.servers]]` entries are
//! repo-supplied (the same trust class as Phase 0's `gate`/`until`): before
//! this module, `connect()` spawned whatever `command`/`args` a repo's
//! `.lopi/loop.toml` named — including one added by a pull request under
//! evaluation — with no allowlist, no pinning, no signature check (confirmed
//! by grepping this crate for `allowlist|trusted|verify|signature` before
//! this module existed: no matches). Fifteen clean releases then one
//! malicious line (the postmark-mcp incident) is the threat model this
//! closes: pinning the exact approved invocation matters more than vetting a
//! point-in-time release, since vetting doesn't survive the next
//! `npm install`/binary refresh.
//!
//! Mirrors the shape of the deleted `lopi-remote::egress` module (Sprint S2,
//! removed in Sprint S10 Phase 4 along with the Telegram transport that was
//! its only caller): an empty allowlist denies everything, never falls
//! through to "unrestricted" — the same fail-closed posture
//! `crates/lopi-ui/src/web/auth_policy.rs` uses for the web dashboard's auth
//! token.
//!
//! The allowlist itself lives outside any repo checkout, in the operator's
//! own home directory (`~/.lopi/mcp_allowlist.toml`) — the same
//! "can't have arrived via a branch under evaluation" property Phase 0's
//! `LoopConfig::load_operator_overrides` relies on for `~/.lopi/loop.toml`.

use crate::config::McpServerSpec;
use serde::Deserialize;

/// `~/.lopi/mcp_allowlist.toml`'s shape: a flat list of exactly-approved
/// server specs. Matching is exact on `name` + `command` + `args` together
/// (not just `name`/`command`) — an operator approving a server is approving
/// one specific invocation, not merely a binary name a repo's
/// `.lopi/loop.toml` could otherwise smuggle different flags into (e.g.
/// widening a `--root` path) while keeping `name`/`command` unchanged.
#[derive(Debug, Default, Deserialize)]
struct AllowlistFile {
    #[serde(default)]
    allowed: Vec<McpServerSpec>,
}

/// Whether `spec` exactly matches an entry in `allowlist`. Pure — the
/// deny-by-default property this whole module exists for: `allowlist.is_empty()`
/// returns `false` for every `spec`, never "unrestricted."
#[must_use]
pub fn is_allowed(allowlist: &[McpServerSpec], spec: &McpServerSpec) -> bool {
    allowlist.contains(spec)
}

/// [`is_allowed`] plus structured denial logging on a dedicated
/// `lopi_mcp::security` target, so a denied spawn is queryable/alertable
/// separately from ordinary warnings — mirrors the deleted
/// `lopi_remote::egress::check_egress`'s logging shape.
#[must_use]
pub fn check_mcp_server(allowlist: &[McpServerSpec], spec: &McpServerSpec) -> bool {
    if is_allowed(allowlist, spec) {
        return true;
    }
    tracing::warn!(
        target: "lopi_mcp::security",
        server = %spec.name,
        command = %spec.command,
        args = ?spec.args,
        "mcp server spawn denied: not in the operator allowlist (~/.lopi/mcp_allowlist.toml)"
    );
    false
}

/// Load the operator's MCP server allowlist from `~/.lopi/mcp_allowlist.toml`.
///
/// Absent, unreadable, or malformed all yield an empty allowlist — which
/// denies every server, per [`is_allowed`]'s contract. A missing or broken
/// allowlist file is never treated as "no policy configured, allow
/// anything"; the fail-closed direction matches
/// [`crate::config::LoopConfig`]-adjacent Phase 0 precedent
/// (`LoopConfig::load_operator_overrides`) rather than the *opposite*
/// direction a missing auth token or webhook secret takes elsewhere in this
/// codebase (those fail closed by refusing to *start*; this fails closed by
/// refusing to *spawn*, since an MCP-less lopi is still fully usable).
#[must_use]
pub fn load_operator_allowlist() -> Vec<McpServerSpec> {
    let Ok(home) = std::env::var("HOME") else {
        return Vec::new();
    };
    let path = std::path::Path::new(&home)
        .join(".lopi")
        .join("mcp_allowlist.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match toml::from_str::<AllowlistFile>(&text) {
        Ok(f) => f.allowed,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                "mcp allowlist exists but failed to parse ({e}); denying all servers"
            );
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn spec(name: &str, command: &str, args: &[&str]) -> McpServerSpec {
        McpServerSpec {
            name: name.into(),
            command: command.into(),
            args: args.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn empty_allowlist_denies_rather_than_permits() {
        let s = spec("fs", "mcp-server-filesystem", &["--root", "/srv"]);
        assert!(!is_allowed(&[], &s));
        assert!(!check_mcp_server(&[], &s));
    }

    #[test]
    fn exact_match_is_allowed() {
        let s = spec("fs", "mcp-server-filesystem", &["--root", "/srv"]);
        let allowlist = vec![s.clone()];
        assert!(is_allowed(&allowlist, &s));
        assert!(check_mcp_server(&allowlist, &s));
    }

    /// The rejecting test: matching `name`+`command` but different `args`
    /// (an attacker widening `--root` while keeping the approved binary
    /// name) must still be denied.
    #[test]
    fn same_name_and_command_but_different_args_is_denied() {
        let allowed = spec("fs", "mcp-server-filesystem", &["--root", "/srv"]);
        let widened = spec("fs", "mcp-server-filesystem", &["--root", "/"]);
        let allowlist = vec![allowed];
        assert!(!is_allowed(&allowlist, &widened));
    }

    #[test]
    fn different_name_same_command_is_denied() {
        let allowed = spec("fs", "mcp-server-filesystem", &[]);
        let renamed = spec("filesystem2", "mcp-server-filesystem", &[]);
        let allowlist = vec![allowed];
        assert!(!is_allowed(&allowlist, &renamed));
    }

    #[test]
    fn load_operator_allowlist_absent_file_is_empty() {
        let original = std::env::var("HOME").ok();
        let home = std::env::temp_dir().join("lopi_mcp_allowlist_absent_test");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        assert!(load_operator_allowlist().is_empty());

        match original {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn load_operator_allowlist_reads_configured_servers() {
        let original = std::env::var("HOME").ok();
        let home = std::env::temp_dir().join("lopi_mcp_allowlist_present_test");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".lopi")).unwrap();
        std::fs::write(
            home.join(".lopi").join("mcp_allowlist.toml"),
            "[[allowed]]\nname = \"fs\"\ncommand = \"mcp-server-filesystem\"\nargs = []\n",
        )
        .unwrap();
        std::env::set_var("HOME", &home);

        let allowlist = load_operator_allowlist();
        assert_eq!(allowlist, vec![spec("fs", "mcp-server-filesystem", &[])]);

        match original {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn load_operator_allowlist_malformed_toml_is_empty_not_a_panic() {
        let original = std::env::var("HOME").ok();
        let home = std::env::temp_dir().join("lopi_mcp_allowlist_malformed_test");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".lopi")).unwrap();
        std::fs::write(
            home.join(".lopi").join("mcp_allowlist.toml"),
            "not valid toml {{{",
        )
        .unwrap();
        std::env::set_var("HOME", &home);

        assert!(load_operator_allowlist().is_empty());

        match original {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
