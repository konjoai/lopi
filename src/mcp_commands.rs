//! `lopi mcp` — inspect a repo's configured MCP servers.
//!
//! MCP servers are declared as `[[mcp.servers]]` in `.lopi/loop.toml`; lopi
//! spawns them to discover and call external tools (see `lopi-mcp`). This
//! command surfaces what a repo has configured. The rendering is a pure
//! return-value function so the `main.rs` dispatch only prints, keeping the
//! output unit-testable without capturing stdout.

use anyhow::Result;
use clap::Subcommand;
use lopi_mcp::McpServerSpec;
use std::path::{Path, PathBuf};

/// `lopi mcp` subcommands.
#[derive(Subcommand)]
pub enum McpCmd {
    /// List the MCP servers configured in `<repo>/.lopi/loop.toml`.
    Servers {
        /// Repository whose loop config to read.
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
    },
}

/// List the MCP servers configured for `repo`, returning a printable summary.
///
/// # Errors
/// Returns `Err` if `<repo>/.lopi/loop.toml` exists but is malformed.
pub fn servers(repo: &Path) -> Result<String> {
    let specs = lopi_mcp::load_servers(repo)?;
    Ok(format_servers(repo, &specs))
}

/// Render the configured servers as a printable block. Pure — no IO.
#[must_use]
fn format_servers(repo: &Path, specs: &[McpServerSpec]) -> String {
    let mut out = format!("⟲ lopi mcp servers — {}\n", repo.display());
    if specs.is_empty() {
        out.push_str("  no MCP servers configured in .lopi/loop.toml\n");
        return out;
    }
    for s in specs {
        let args = if s.args.is_empty() {
            String::new()
        } else {
            format!(" {}", s.args.join(" "))
        };
        out.push_str(&format!("  {} → {}{}\n", s.name, s.command, args));
    }
    out.push_str(&format!("  ({} server(s))\n", specs.len()));
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::{format_servers, servers};
    use lopi_mcp::McpServerSpec;
    use std::path::Path;
    use tempfile::TempDir;

    fn spec(name: &str, command: &str, args: &[&str]) -> McpServerSpec {
        McpServerSpec {
            name: name.into(),
            command: command.into(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
        }
    }

    #[test]
    fn format_lists_servers_with_args() {
        let specs = vec![
            spec("fs", "mcp-server-filesystem", &["--root", "/srv"]),
            spec("gh", "mcp-server-github", &[]),
        ];
        let out = format_servers(Path::new("/repo"), &specs);
        assert!(out.contains("fs → mcp-server-filesystem --root /srv"));
        assert!(out.contains("gh → mcp-server-github\n"), "no trailing args");
        assert!(out.contains("(2 server(s))"));
    }

    #[test]
    fn format_says_none_when_empty() {
        let out = format_servers(Path::new("/repo"), &[]);
        assert!(out.contains("no MCP servers configured"));
        assert!(!out.contains("server(s)"));
    }

    #[test]
    fn servers_reads_repo_config() {
        let dir = TempDir::new().unwrap();
        let cfg = dir.path().join(".lopi");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(
            cfg.join("loop.toml"),
            "[[mcp.servers]]\nname = \"fs\"\ncommand = \"mcp-fs\"\n",
        )
        .unwrap();
        let out = servers(dir.path()).unwrap();
        assert!(out.contains("fs → mcp-fs"));
    }

    #[test]
    fn servers_empty_when_no_config() {
        let dir = TempDir::new().unwrap();
        assert!(servers(dir.path()).unwrap().contains("no MCP servers"));
    }
}
