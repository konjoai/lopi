use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LopiConfig {
    pub lopi: CoreConfig,
    pub claude: ClaudeConfig,
    pub git: GitConfig,
    #[serde(default)]
    pub remote: RemoteConfig,
    #[serde(default)]
    pub web: WebConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    #[serde(default = "default_max_agents")]
    pub max_agents: usize,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeConfig {
    #[serde(default = "default_claude_cli")]
    pub cli_path: String,
    #[serde(default = "default_claude_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    #[serde(default = "default_allowed")]
    pub default_allowed_dirs: Vec<String>,
    #[serde(default = "default_forbidden")]
    pub default_forbidden_dirs: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_pr: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteConfig {
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub whatsapp: WhatsappConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub token: Option<String>,
    pub chat_id: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WhatsappConfig {
    pub account_sid: Option<String>,
    pub auth_token: Option<String>,
    pub from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_host")]
    pub host: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self { port: default_port(), host: default_host() }
    }
}

fn default_max_agents() -> usize { 4 }
fn default_log_level() -> String { "info".into() }
fn default_db_path() -> PathBuf { PathBuf::from("~/.lopi/lopi.db") }
fn default_claude_cli() -> String { "claude".into() }
fn default_claude_timeout() -> u64 { 300 }
fn default_allowed() -> Vec<String> { vec!["src/".into(), "tests/".into()] }
fn default_forbidden() -> Vec<String> { vec![".github/".into(), "infra/".into(), "Cargo.toml".into()] }
fn default_true() -> bool { true }
fn default_port() -> u16 { 3000 }
fn default_host() -> String { "127.0.0.1".into() }

impl LopiConfig {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&text)?;
        Ok(cfg)
    }
}
