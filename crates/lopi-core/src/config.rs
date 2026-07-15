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
    /// Trust level governing how far this scheduled loop may act without a
    /// human (L1 report-only … L4 auto-merge). Defaults to L2 (draft PR).
    #[serde(default)]
    pub autonomy_level: crate::loop_config::AutonomyLevel,
    /// Report on Finish (Loop Engineering primitive 6) — channel a completed
    /// run's summary is sent to, e.g. `"telegram"`. `None` (the default)
    /// leaves the run's L1 report-only hook logging locally only, as before
    /// this field existed. Validated by [`Self::validate_report`] at
    /// config-load time — an unknown or currently-unreachable channel name
    /// (e.g. `"whatsapp"`, which has no outbound-send path) is a loud load
    /// error, never a silent no-op.
    #[serde(default)]
    pub report: Option<String>,
}

impl ScheduleEntry {
    /// Validate the `report` channel name, if set. `None` is always valid —
    /// report-on-finish is opt-in.
    ///
    /// # Errors
    /// Returns [`crate::report::ReportChannelError`] when `report` is set to
    /// a channel lopi doesn't recognize, or recognizes but can't reach yet
    /// (`"whatsapp"` — inbound-only, no send path).
    pub fn validate_report(&self) -> Result<(), crate::report::ReportChannelError> {
        self.report.as_deref().map_or(Ok(()), |name| {
            crate::report::ReportChannel::parse(name).map(|_| ())
        })
    }
}

/// An Anthropic account rate-limit window MAXX's headroom gate can check.
/// Mirrors the `limit_type` values the CLI reports on `rate_limit_event`
/// (`AgentEvent::ApiRetry::limit_type`) — kept as a closed enum here (rather
/// than the raw `String` `ApiRetry` uses) so `/api/maxx` can reject a typo'd
/// window name at the API boundary instead of silently never matching.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LimitWindow {
    /// The rolling five-hour window (rolls from first use, not wall-clock fixed).
    FiveHour,
    /// The rolling seven-day window.
    SevenDay,
}

impl LimitWindow {
    /// The wire/storage tag, matching `ApiRetry::limit_type` exactly.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FiveHour => "five_hour",
            Self::SevenDay => "seven_day",
        }
    }

    /// Parse a wire tag. `None` for anything other than `five_hour`/`seven_day`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "five_hour" => Some(Self::FiveHour),
            "seven_day" => Some(Self::SevenDay),
            _ => None,
        }
    }
}

/// MAXX — opportunistic backlog dispatch entry. Mirrors [`ScheduleEntry`]'s
/// shared fields (everything but `cron`) plus the conditions that decide
/// when it's favorable to fire: quiet hours, and/or comfortable quota
/// headroom on the configured windows. Unlike a schedule, a `MaxxEntry`
/// never fires on a fixed cadence — `lopi_orchestrator::maxx_loop` ticks
/// on an interval and fires it only when [`LimitWindow`] conditions say so.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaxxEntry {
    /// Human-readable name shown in the dashboard.
    pub name: String,
    /// Absolute path to the git repo this task runs against.
    pub repo: PathBuf,
    /// The agent goal (passed to `lopi run --goal`).
    pub goal: String,
    /// Optional priority override ("low", "normal", "high", "critical").
    #[serde(default = "default_priority_str")]
    pub priority: String,
    /// Allowed dirs override (falls back to global git config if empty).
    #[serde(default)]
    pub allowed_dirs: Vec<String>,
    /// Forbidden dirs override.
    #[serde(default)]
    pub forbidden_dirs: Vec<String>,
    /// Trust level governing how far this loop may act without a human.
    #[serde(default)]
    pub autonomy_level: crate::loop_config::AutonomyLevel,
    /// Report on Finish channel, e.g. `"telegram"`.
    #[serde(default)]
    pub report: Option<String>,
    /// Local hours `(start, end)` treated as quiet hours, e.g. `Some((23, 7))`
    /// for 11PM-7AM. `end < start` wraps past midnight. `None` disables this
    /// condition.
    #[serde(default)]
    pub quiet_hours: Option<(u8, u8)>,
    /// Whether to also fire when quota headroom on `windows` is favorable
    /// ("nearing quota reset with high headroom").
    #[serde(default)]
    pub headroom_gate: bool,
    /// Which windows `headroom_gate` checks. Ignored when `headroom_gate`
    /// is `false`. Empty means the gate can never be satisfied even if
    /// `headroom_gate` is `true` — a misconfiguration, not "always favorable".
    #[serde(default)]
    pub windows: Vec<LimitWindow>,
}

impl MaxxEntry {
    /// Validate the `report` channel name, if set. `None` is always valid.
    ///
    /// # Errors
    /// Returns [`crate::report::ReportChannelError`] when `report` is set to
    /// a channel lopi doesn't recognize or can't reach yet.
    pub fn validate_report(&self) -> Result<(), crate::report::ReportChannelError> {
        self.report.as_deref().map_or(Ok(()), |name| {
            crate::report::ReportChannel::parse(name).map(|_| ())
        })
    }
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
    /// Returns `Err` if the file cannot be read, if TOML parsing fails, or if
    /// any `[[schedules]]` entry's `report` channel fails
    /// [`ScheduleEntry::validate_report`] — a typo'd or unreachable channel
    /// (e.g. `"whatsapp"`) fails the load loudly rather than silently never
    /// sending a report.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&text)?;
        for entry in &cfg.schedules {
            entry
                .validate_report()
                .map_err(|e| anyhow::anyhow!("schedule `{}`: {e}", entry.name))?;
        }
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
