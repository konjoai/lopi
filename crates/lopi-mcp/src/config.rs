//! MCP server configuration, parsed from a repo's `.lopi/loop.toml`.
//!
//! A repo declares the tool servers its agents may use as `[[mcp.servers]]`
//! entries. This module reads just that section — ignoring the rest of the
//! loop config, which `lopi-core` owns — so `lopi-mcp` stays decoupled from it.
//! [`McpServerSpec::connect`] then launches a configured server.

use crate::client::StdioClient;
use crate::McpClient;
use serde::Deserialize;
use std::path::Path;

/// Location of the loop config inside a repo (mirrors `lopi-core`'s constant,
/// duplicated to avoid a dependency on it).
const LOOP_TOML_REL: &str = ".lopi/loop.toml";

/// One MCP server lopi can connect to — a `[[mcp.servers]]` entry.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct McpServerSpec {
    /// Stable identifier, used to namespace the server's tools.
    pub name: String,
    /// Executable to launch.
    pub command: String,
    /// Arguments passed to the executable.
    #[serde(default)]
    pub args: Vec<String>,
}

impl McpServerSpec {
    /// Spawn and connect to this server over its stdio.
    ///
    /// # Errors
    /// Returns `Err` if the process cannot be spawned or its stdio not captured.
    pub fn connect(&self) -> anyhow::Result<StdioClient> {
        McpClient::spawn(&self.command, &self.args)
    }
}

/// `.lopi/loop.toml` viewed through just its `[mcp]` section; all other fields
/// are ignored, so this parses cleanly alongside the full loop config.
#[derive(Debug, Default, Deserialize)]
struct McpView {
    #[serde(default)]
    mcp: McpSection,
}

#[derive(Debug, Default, Deserialize)]
struct McpSection {
    #[serde(default)]
    servers: Vec<McpServerSpec>,
}

/// Parse the `[[mcp.servers]]` entries out of a loop.toml document.
///
/// # Errors
/// Returns `Err` if the TOML is malformed or an entry is missing a required
/// field (`name`/`command`).
pub fn parse_servers(loop_toml: &str) -> Result<Vec<McpServerSpec>, toml::de::Error> {
    Ok(toml::from_str::<McpView>(loop_toml)?.mcp.servers)
}

/// Load the MCP servers configured for `repo` from `<repo>/.lopi/loop.toml`.
///
/// Returns an empty vec when the file is absent; errors only when it exists but
/// cannot be read or parsed.
///
/// # Errors
/// Returns `Err` if the file exists but is unreadable or malformed.
pub fn load_servers(repo: &Path) -> anyhow::Result<Vec<McpServerSpec>> {
    let path = repo.join(LOOP_TOML_REL);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)?;
    parse_servers(&text).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::{load_servers, parse_servers, McpServerSpec};
    use tempfile::TempDir;

    const LOOP_TOML: &str = r#"
autonomy_level = "draft_pr"
max_iterations = 25

[[mcp.servers]]
name = "filesystem"
command = "mcp-server-filesystem"
args = ["--root", "/srv"]

[[mcp.servers]]
name = "github"
command = "mcp-server-github"
"#;

    #[test]
    fn parses_servers_and_ignores_other_loop_fields() {
        let servers = parse_servers(LOOP_TOML).unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "filesystem");
        assert_eq!(servers[0].command, "mcp-server-filesystem");
        assert_eq!(servers[0].args, vec!["--root", "/srv"]);
        // args default to empty when omitted.
        assert_eq!(servers[1].name, "github");
        assert!(servers[1].args.is_empty());
    }

    #[test]
    fn no_mcp_section_yields_empty() {
        assert!(parse_servers("autonomy_level = \"draft_pr\"\n")
            .unwrap()
            .is_empty());
        assert!(parse_servers("").unwrap().is_empty());
    }

    #[test]
    fn missing_required_field_errors() {
        // An entry without `command` is rejected.
        let bad = "[[mcp.servers]]\nname = \"x\"\n";
        assert!(parse_servers(bad).is_err());
    }

    #[test]
    fn malformed_toml_errors() {
        assert!(parse_servers("[[mcp.servers]\nname = ").is_err());
    }

    #[test]
    fn load_servers_missing_file_is_empty() {
        let dir = TempDir::new().unwrap();
        assert!(load_servers(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn load_servers_reads_the_repo_config() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(".lopi");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join("loop.toml"), LOOP_TOML).unwrap();
        let servers = load_servers(dir.path()).unwrap();
        assert_eq!(
            servers,
            vec![
                McpServerSpec {
                    name: "filesystem".into(),
                    command: "mcp-server-filesystem".into(),
                    args: vec!["--root".into(), "/srv".into()],
                },
                McpServerSpec {
                    name: "github".into(),
                    command: "mcp-server-github".into(),
                    args: vec![],
                },
            ]
        );
    }

    #[tokio::test]
    async fn connect_spawns_a_live_process() {
        // `cat` blocks reading stdin, so it stays alive long enough to observe a
        // pid; `kill_on_drop` reaps it when the client drops. Exercises the
        // production spawn path end-to-end.
        let spec = McpServerSpec {
            name: "noop".into(),
            command: "cat".into(),
            args: vec![],
        };
        let client = spec.connect().unwrap();
        assert!(client.server_pid().is_some(), "spawned server has a pid");
    }
}
