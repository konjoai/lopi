use crate::agent::ScoreWeights;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Root configuration loaded from `lopi.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LopiConfig {
    /// Core orchestrator settings.
    pub lopi: CoreConfig,
    /// Claude CLI / API settings.
    pub claude: ClaudeConfig,
    /// Git branch and directory policy settings.
    pub git: GitConfig,
    #[serde(default)]
    /// Remote control integrations (Telegram, WhatsApp).
    pub remote: RemoteConfig,
    #[serde(default)]
    /// Web dashboard server settings.
    pub web: WebConfig,
    #[serde(default)]
    /// Score weighting configuration.
    pub scoring: ScoringConfig,
    #[serde(default)]
    /// Cron-scheduled task entries.
    pub schedules: Vec<ScheduleEntry>,
}

/// Core orchestrator settings (`[lopi]` table in `lopi.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    /// Maximum number of agents to run concurrently (default `4`).
    #[serde(default = "default_max_agents")]
    pub max_agents: usize,
    /// Log verbosity level, e.g. `"info"` or `"debug"` (default `"info"`).
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Path to the SQLite memory database (default `~/.lopi/lopi.db`).
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
    /// When `true`, agents may open self-modification PRs on this repo.
    #[serde(default)]
    pub allow_self_modify: bool,
    /// When `true`, directory access restrictions are disabled for this session.
    /// Equivalent to `lopi bypass` — use only in trusted environments.
    #[serde(default)]
    pub bypass_permissions: bool,
}

/// Claude CLI invocation settings (`[claude]` table in `lopi.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeConfig {
    /// Path or name of the `claude` CLI binary (default `"claude"`).
    #[serde(default = "default_claude_cli")]
    pub cli_path: String,
    /// Maximum seconds to wait for a single Claude invocation (default `300`).
    #[serde(default = "default_claude_timeout")]
    pub timeout_secs: u64,
}

/// Git branch and directory policy settings (`[git]` table in `lopi.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    /// Directories agents are permitted to modify by default.
    #[serde(default = "default_allowed")]
    pub default_allowed_dirs: Vec<String>,
    /// Directories agents are never permitted to modify.
    #[serde(default = "default_forbidden")]
    pub default_forbidden_dirs: Vec<String>,
    /// When `true`, agents automatically open a PR after a successful run.
    #[serde(default = "default_true")]
    pub auto_pr: bool,
}

/// Remote control integration configuration (`[remote]` table in `lopi.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteConfig {
    /// Telegram bot configuration.
    #[serde(default)]
    pub telegram: TelegramConfig,
    /// WhatsApp (Twilio) configuration.
    #[serde(default)]
    pub whatsapp: WhatsappConfig,
}

/// Telegram bot settings (`[remote.telegram]` table in `lopi.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Telegram bot API token (set via env or config).
    pub token: Option<String>,
    /// Default chat ID to send proactive notifications to.
    pub chat_id: Option<i64>,
    /// Allowlist of Telegram chat IDs permitted to issue commands.
    /// Empty = allow all chats (dev mode).
    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
}

/// WhatsApp (Twilio) settings (`[remote.whatsapp]` table in `lopi.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WhatsappConfig {
    /// Twilio account SID.
    pub account_sid: Option<String>,
    /// Twilio auth token.
    pub auth_token: Option<String>,
    /// Twilio `From` WhatsApp number (e.g. `"whatsapp:+14155238886"`).
    pub from: Option<String>,
    /// Twilio signing secret for HMAC-SHA1 webhook signature verification.
    #[serde(default)]
    pub signing_secret: Option<String>,
}

/// Web dashboard server settings (`[web]` table in `lopi.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// TCP port the web server binds to (default `3000`).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Host address the web server listens on (default `"127.0.0.1"`).
    #[serde(default = "default_host")]
    pub host: String,
    /// Bearer token required on all /api/* routes.
    /// None = auth disabled (dev mode).
    #[serde(default)]
    pub auth_token: Option<String>,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            host: default_host(),
            auth_token: None,
        }
    }
}

/// Score weighting configuration (`[scoring]` table in `lopi.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoringConfig {
    /// Penalty weights used when computing composite scores.
    #[serde(default)]
    pub weights: ScoreWeights,
}

/// A single cron-scheduled lopi task entry from `lopi.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleEntry {
    /// Human-readable name shown in `lopi schedules list`.
    pub name: String,
    /// Absolute path to the git repo this task runs against.
    pub repo: PathBuf,
    /// The agent goal (passed to `lopi run --goal`).
    pub goal: String,
    /// Standard 5-field cron expression, e.g. `"0 2 * * *"` (2am daily).
    pub cron: String,
    /// Optional priority override ("low", "normal", "high", "critical").
    #[serde(default = "default_priority_str")]
    pub priority: String,
    /// Allowed dirs override (falls back to global git config if empty).
    #[serde(default)]
    pub allowed_dirs: Vec<String>,
    /// Forbidden dirs override.
    #[serde(default)]
    pub forbidden_dirs: Vec<String>,
}

/// Per-repo profile loaded from `<repo>/.lopi.toml`.
/// Fields present here override the global config for that repo.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoProfile {
    /// Override allowed dirs for this repo.
    #[serde(default)]
    pub allowed_dirs: Vec<String>,
    /// Override forbidden dirs for this repo.
    #[serde(default)]
    pub forbidden_dirs: Vec<String>,
    /// Command used to run tests (default: cargo test / npm test detection).
    pub test_command: Option<String>,
    /// Command used for linting (default: cargo clippy detection).
    pub lint_command: Option<String>,
    /// Extra constraints always injected into the planning prompt for this repo.
    #[serde(default)]
    pub default_constraints: Vec<String>,
    /// Max retries override.
    pub max_retries: Option<u8>,
}

impl RepoProfile {
    /// Load `.lopi.toml` from the repo root. Returns `Default` if not found.
    #[must_use]
    pub fn load_from_repo(repo_path: &std::path::Path) -> Self {
        let p = repo_path.join(".lopi.toml");
        if !p.exists() {
            return Self::default();
        }
        std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Apply this profile's overrides onto a `Task`, filling in non-default values.
    pub fn apply(&self, task: &mut crate::task::Task) {
        if !self.allowed_dirs.is_empty() {
            task.allowed_dirs.clone_from(&self.allowed_dirs);
        }
        if !self.forbidden_dirs.is_empty() {
            task.forbidden_dirs.clone_from(&self.forbidden_dirs);
        }
        if !self.default_constraints.is_empty() {
            task.constraints.extend(self.default_constraints.clone());
        }
        if let Some(r) = self.max_retries {
            task.max_retries = r;
        }
    }
}

fn default_max_agents() -> usize {
    4
}
fn default_log_level() -> String {
    "info".into()
}
fn default_db_path() -> PathBuf {
    PathBuf::from("~/.lopi/lopi.db")
}
fn default_claude_cli() -> String {
    "claude".into()
}
fn default_claude_timeout() -> u64 {
    300
}
fn default_allowed() -> Vec<String> {
    vec!["src/".into(), "tests/".into()]
}
fn default_forbidden() -> Vec<String> {
    vec![".github/".into(), "infra/".into(), "Cargo.toml".into()]
}
fn default_true() -> bool {
    true
}
fn default_port() -> u16 {
    3000
}
fn default_host() -> String {
    "127.0.0.1".into()
}
fn default_priority_str() -> String {
    "normal".into()
}

impl LopiConfig {
    /// Load and parse a `lopi.toml` config file from `path`.
    ///
    /// # Errors
    /// Returns `Err` if the file cannot be read or if TOML parsing fails.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&text)?;
        Ok(cfg)
    }

    /// Try loading from `./lopi.toml` then `~/.lopi/lopi.toml`. Returns `None` if neither exists.
    #[must_use]
    pub fn find_and_load() -> Option<Self> {
        let candidates = [
            PathBuf::from("lopi.toml"),
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join(".lopi")
                .join("lopi.toml"),
        ];
        for p in &candidates {
            if p.exists() {
                return Self::load(p).ok();
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
