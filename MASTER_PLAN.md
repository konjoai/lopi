# KonjoAI Agent Orchestrator — Master Plan
> High-performance, self-improving Claude Code agent orchestrator in Rust

---

## 1. Name Brainstorm

| Name | Rationale |
|------|-----------|
| **lopi** | Orchestrate + orca — a massive, intelligent, coordinating creature. 4 chars, punchy. **⭐ TOP PICK** |
| **ryku** | Synthetic KonjoAI-style syllables. Sounds fast and technical. |
| **ferru** | Latin for *iron* (ferrous → iron oxide → Rust). Direct language theme. |
| **nexo** | Nexus — the central connection point through which all agents flow. |
| **axon** | Neural axon — carries signals between neurons (agents). Concurrency-brain metaphor. |
| **kumo** | Japanese for *cloud* and *spider*. Both fit: cloud-scale + spider's web of agents. |
| **keiro** | Japanese for *path* or *route* — the agent's planned journey through a task. |
| **volta** | Electrical unit of potential + *turning* in Italian. Rust-era energy + iteration. |
| **forxa** | Forge + KonjoAI suffix aesthetic. You forge code, you forge intelligence. |
| **oxyn** | Oxidize → Rust. Oxygen → async breathing. Dual Rust/async metaphor. |

### Recommendation: `lopi`

`lopi` wins on all axes:
- **Meaning**: orchestrator × orca (apex, coordinating, powerful)
- **Length**: 4 chars — shortest in the KonjoAI family alongside `toki`
- **Sound**: hard stop consonants, punchy CLI invocation (`lopi run`, `lopi status`)
- **Domain availability**: likely clear, no major trademark conflicts
- **Vibe**: dominant, coordinated, fast — exactly what this system is

---

## 2. What Is Lopi?

Lopi is a production-grade, Rust-native agent orchestrator for Claude Code. It evolves the Python prototype (`claude-self-mod-agent`) into a concurrent, memory-augmented, remotely-controllable system where N Claude Code agents run on isolated git branches, self-improve through a scored retry loop, and report back via terminal UI, web dashboard, and your phone.

**Core loop:**
```
Phone / Webhook / TUI → Task Queue → Agent Pool (N parallel) →
  Plan → Implement → Test → Score → (Pass → PR) | (Fail → Retry | Rollback)
→ Memory Log → Next Attempt (smarter)
```

---

## 3. Repository Structure

```
lopi/
├── Cargo.toml                    # Workspace root
├── Cargo.lock
├── .env.example                  # Environment variable template
├── lopi.toml.example             # User config template
├── README.md
├── docs/
│   ├── architecture.md
│   ├── phone-control.md
│   └── safety.md
│
├── crates/
│   ├── lopi-core/                # Shared types, traits, errors
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models.rs         # Task, AgentRun, Attempt, MemoryLog, AgentState
│   │       ├── traits.rs         # Orchestrator, Agent, Memory, Remote traits
│   │       ├── error.rs          # LopiError enum
│   │       └── config.rs         # Config structs (TOML + env)
│   │
│   ├── lopi-orchestrator/        # Task queue + agent pool
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── queue.rs          # Priority task queue (tokio broadcast + mpsc)
│   │       ├── pool.rs           # Agent pool — spin up/down N agents
│   │       ├── scheduler.rs      # Rate limiting, concurrency caps, backoff
│   │       └── state.rs          # Shared Arc<RwLock<OrchestratorState>>
│   │
│   ├── lopi-agent/               # The agent loop itself
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── runner.rs         # Plan→Implement→Test→Score→Retry loop
│   │       ├── claude.rs         # tokio::process Claude Code CLI invocation
│   │       ├── sandbox.rs        # Workspace isolation per agent
│   │       └── scorer.rs         # Test pass rate + lint + diff size → score
│   │
│   ├── lopi-git/                 # Git branch isolation + safety
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── branch.rs         # Create/checkout/delete branches (git2)
│   │       ├── diff.rs           # Diff scope checker — off-limits files, size cap
│   │       ├── rollback.rs       # Hard rollback on failure
│   │       └── pr.rs             # GitHub PR creation via octocrab
│   │
│   ├── lopi-memory/              # SQLite persistence
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── store.rs          # sqlx SQLite pool, migrations
│   │       ├── attempts.rs       # Attempt history CRUD
│   │       ├── patterns.rs       # Pattern extraction — what worked, what failed
│   │       └── migrations/
│   │           ├── 001_initial.sql
│   │           └── 002_patterns.sql
│   │
│   ├── lopi-ui/                  # Terminal + web UI
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── tui/
│   │       │   ├── mod.rs
│   │       │   ├── app.rs        # TUI application state
│   │       │   ├── widgets.rs    # Agent panel, task queue, log stream
│   │       │   └── events.rs     # Keyboard input, terminal resize
│   │       └── web/
│   │           ├── mod.rs
│   │           ├── server.rs     # axum router setup
│   │           ├── ws.rs         # WebSocket broadcast — live agent state
│   │           ├── routes.rs     # REST: /tasks, /agents, /runs, /logs
│   │           └── static/       # Embedded HTML/JS dashboard (include_str!)
│   │               ├── index.html
│   │               └── dashboard.js
│   │
│   ├── lopi-remote/              # Phone remote control
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── telegram.rs       # teloxide Telegram bot
│   │       ├── whatsapp.rs       # Twilio WhatsApp webhook handler (axum route)
│   │       └── commands.rs       # Shared command parser (/task, /status, /approve)
│   │
│   └── lopi-webhook/             # GitHub + CI webhooks
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── github.rs         # GitHub webhook events → task injection
│           └── ci.rs             # CI failure hook → retry task injection
│
└── src/                          # Binary entry point
    └── main.rs                   # CLI: `lopi run`, `lopi tui`, `lopi web`, `lopi task add`
```

---

## 4. Cargo.toml (Workspace Root)

```toml
[workspace]
resolver = "2"
members = [
    "crates/lopi-core",
    "crates/lopi-orchestrator",
    "crates/lopi-agent",
    "crates/lopi-git",
    "crates/lopi-memory",
    "crates/lopi-ui",
    "crates/lopi-remote",
    "crates/lopi-webhook",
    ".",
]

[workspace.package]
version      = "0.1.0"
edition      = "2021"
authors      = ["KonjoAI"]
license      = "MIT"
rust-version = "1.78"

[workspace.dependencies]
# Async runtime
tokio        = { version = "1", features = ["full"] }
tokio-stream = "0.1"

# Web / networking
axum         = { version = "0.7", features = ["ws", "macros"] }
tower        = "0.4"
tower-http   = { version = "0.5", features = ["cors", "trace"] }
hyper        = { version = "1", features = ["full"] }

# Telegram bot
teloxide     = { version = "0.12", features = ["macros", "webhooks-axum"] }

# HTTP client (Twilio, GitHub)
reqwest      = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }

# GitHub API
octocrab     = "0.39"

# Git operations
git2         = "0.19"

# Database
sqlx         = { version = "0.7", features = ["runtime-tokio-rustls", "sqlite", "migrate", "chrono", "uuid"] }

# Serialization
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
toml         = "0.8"

# IDs and time
uuid         = { version = "1", features = ["v4", "serde"] }
chrono       = { version = "0.4", features = ["serde"] }
ulid         = "1"

# Error handling
anyhow       = "1"
thiserror    = "1"

# Logging / tracing
tracing           = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt", "json"] }

# Configuration
config       = "0.14"
dotenvy      = "0.15"

# Terminal UI
ratatui      = "0.26"
crossterm    = "0.27"

# Concurrency utilities
dashmap      = "5"
parking_lot  = "0.12"
arc-swap     = "1"

# Process management
which        = "6"

# Diff / file analysis
similar      = "2"
ignore       = "0.4"

# CLI
clap         = { version = "4", features = ["derive", "env"] }

# Misc utilities
futures      = "0.3"
async-trait  = "0.1"
bytes        = "1"
rand         = "0.8"
sha2         = "0.10"

[profile.release]
opt-level    = 3
lto          = true
codegen-units = 1
strip        = true
```

---

## 5. Data Models (`lopi-core/src/models.rs`)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Task ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskPriority { Low, Normal, High, Critical }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Assigned { agent_id: Uuid },
    Running,
    Succeeded,
    Failed,
    RolledBack,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id:           Uuid,
    pub title:        String,
    pub description:  String,
    pub target_path:  String,           // e.g. "src/retry.rs" or "src/"
    pub priority:     TaskPriority,
    pub status:       TaskStatus,
    pub source:       TaskSource,       // who created this task
    pub max_retries:  u8,               // default 3
    pub created_at:   DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
    pub tags:         Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskSource {
    Telegram { user_id: i64, username: Option<String> },
    WhatsApp { from: String },
    GitHubWebhook { repo: String, event: String },
    CiFailure { run_id: u64, job: String },
    Manual,
    Api,
}

// ── Agent ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Planning,
    Implementing,
    Testing,
    Scoring,
    Retrying { attempt: u8 },
    OpeningPr,
    RollingBack,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub id:           Uuid,
    pub task_id:      Option<Uuid>,
    pub status:       AgentStatus,
    pub branch:       Option<String>,
    pub attempt:      u8,
    pub last_score:   Option<f64>,
    pub log_tail:     Vec<String>,      // last 50 log lines
    pub started_at:   Option<DateTime<Utc>>,
    pub updated_at:   DateTime<Utc>,
}

// ── Attempt ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub id:            Uuid,
    pub task_id:       Uuid,
    pub agent_id:      Uuid,
    pub attempt_num:   u8,
    pub branch:        String,
    pub plan:          String,           // Claude's plan text
    pub diff:          String,           // git diff output
    pub diff_size:     u32,              // lines changed
    pub test_output:   String,
    pub lint_output:   String,
    pub score:         f64,              // 0.0–1.0
    pub outcome:       AttemptOutcome,
    pub pr_url:        Option<String>,
    pub started_at:    DateTime<Utc>,
    pub finished_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttemptOutcome {
    Accepted { pr_url: String },
    Retried  { reason: String },
    RolledBack { reason: String },
}

// ── Memory ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLog {
    pub id:          Uuid,
    pub task_id:     Uuid,
    pub summary:     String,
    pub patterns:    Vec<Pattern>,
    pub created_at:  DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub kind:        PatternKind,
    pub description: String,
    pub weight:      f32,   // how strongly to weight in future prompts
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternKind { Success, Failure, Warning }

// ── Score ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub test_pass_rate:  f64,   // 0.0–1.0  (weight: 0.50)
    pub lint_clean:      f64,   // 0.0–1.0  (weight: 0.25)
    pub diff_size_score: f64,   // 0.0–1.0  (weight: 0.25, penalizes bloat)
    pub total:           f64,   // weighted sum
}
```

---

## 6. Core Traits (`lopi-core/src/traits.rs`)

```rust
use async_trait::async_trait;
use crate::{models::*, error::LopiError};
use uuid::Uuid;

#[async_trait]
pub trait Orchestrator: Send + Sync {
    async fn enqueue(&self, task: Task) -> Result<Uuid, LopiError>;
    async fn cancel(&self, task_id: Uuid) -> Result<(), LopiError>;
    async fn status(&self) -> OrchestratorStatus;
    async fn agent_states(&self) -> Vec<AgentState>;
}

#[async_trait]
pub trait AgentRunner: Send + Sync {
    async fn run(&self, task: Task) -> Result<Attempt, LopiError>;
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn log_attempt(&self, attempt: &Attempt) -> Result<(), LopiError>;
    async fn get_patterns(&self, target_path: &str) -> Result<Vec<Pattern>, LopiError>;
    async fn get_history(&self, task_id: Uuid) -> Result<Vec<Attempt>, LopiError>;
}

#[async_trait]
pub trait RemoteGateway: Send + Sync {
    async fn notify(&self, msg: RemoteMessage) -> Result<(), LopiError>;
    async fn request_approval(&self, pr: &str) -> Result<bool, LopiError>;
}

#[async_trait]
pub trait DiffChecker: Send + Sync {
    async fn check(&self, diff: &str, config: &DiffPolicy) -> Result<DiffVerdict, LopiError>;
}

#[derive(Debug)]
pub enum DiffVerdict { Safe, TooLarge { lines: u32 }, TouchesOffLimits { files: Vec<String> } }

#[derive(Debug, Clone)]
pub struct OrchestratorStatus {
    pub queued:    usize,
    pub running:   usize,
    pub completed: usize,
    pub failed:    usize,
}

#[derive(Debug, Clone)]
pub struct RemoteMessage {
    pub task_id:  Option<Uuid>,
    pub text:     String,
    pub level:    MessageLevel,
}

#[derive(Debug, Clone)]
pub enum MessageLevel { Info, Success, Warning, Error }
```

---

## 7. Configuration (`lopi-core/src/config.rs`)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LopiConfig {
    pub agent:     AgentConfig,
    pub git:       GitConfig,
    pub memory:    MemoryConfig,
    pub ui:        UiConfig,
    pub remote:    RemoteConfig,
    pub safety:    SafetyConfig,
    pub ci:        CiConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    pub max_agents:      usize,   // default 4 — max concurrent Claude Code processes
    pub max_retries:     u8,      // default 3
    pub timeout_secs:    u64,     // default 300
    pub claude_bin:      String,  // default "claude" — path to Claude Code CLI
    pub score_threshold: f64,     // default 0.75 — minimum score to accept
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitConfig {
    pub repo_path:    String,
    pub base_branch:  String,   // default "main"
    pub remote:       String,   // default "origin"
    pub branch_prefix: String,  // default "lopi/"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryConfig {
    pub db_path:          String,  // default "lopi.db"
    pub max_history_per_path: u32, // default 50
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UiConfig {
    pub web_port:      u16,     // default 7070
    pub tui_enabled:   bool,    // default true
    pub web_enabled:   bool,    // default true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteConfig {
    pub telegram_token:  Option<String>,
    pub telegram_chat_id: Option<i64>,
    pub twilio_account_sid: Option<String>,
    pub twilio_auth_token:  Option<String>,
    pub twilio_from_number: Option<String>,
    pub allowed_phone_numbers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SafetyConfig {
    pub off_limits_files:    Vec<String>,  // globs: ["Cargo.lock", "*.pem", ".env*"]
    pub off_limits_dirs:     Vec<String>,  // ["scripts/", "infra/"]
    pub max_diff_lines:      u32,          // default 500
    pub require_tests_pass:  bool,         // default true
    pub allow_self_modify:   bool,         // default false — cannot modify lopi's own src/
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CiConfig {
    pub github_token:    Option<String>,
    pub github_repo:     Option<String>,  // "owner/repo"
    pub webhook_secret:  Option<String>,
    pub webhook_port:    u16,             // default 7071
}
```

---

## 8. The Agent Loop (`lopi-agent/src/runner.rs`)

```rust
use lopi_core::{models::*, traits::*, config::AgentConfig};
use lopi_git::GitManager;
use lopi_memory::MemoryStore;
use tracing::{info, warn, error};
use uuid::Uuid;

pub struct AgentRunner {
    pub id:      Uuid,
    pub config:  AgentConfig,
    pub git:     Arc<GitManager>,
    pub memory:  Arc<dyn MemoryStore>,
    pub state_tx: tokio::sync::watch::Sender<AgentState>,
}

impl AgentRunner {
    pub async fn run(&self, task: Task) -> Result<Attempt, LopiError> {
        let mut attempt_num = 0u8;

        loop {
            attempt_num += 1;
            info!(agent=%self.id, task=%task.id, attempt=attempt_num, "Starting attempt");

            // 1. Create isolated branch
            let branch = format!("lopi/{}/{}", task.id, attempt_num);
            self.git.create_branch(&branch).await?;
            self.emit_state(AgentStatus::Planning, &branch, attempt_num).await;

            // 2. Load memory context for this path
            let patterns = self.memory.get_patterns(&task.target_path).await?;
            let memory_ctx = self.format_memory_context(&patterns);

            // 3. PLAN — invoke Claude Code with plan prompt
            let plan = self.invoke_claude_plan(&task, &memory_ctx).await?;

            // 4. IMPLEMENT
            self.emit_state(AgentStatus::Implementing, &branch, attempt_num).await;
            self.invoke_claude_implement(&task, &plan).await?;

            // 5. TEST
            self.emit_state(AgentStatus::Testing, &branch, attempt_num).await;
            let test_output = self.run_tests(&task).await?;
            let lint_output = self.run_lint(&task).await?;
            let diff = self.git.get_diff(&branch).await?;

            // 6. SCORE
            self.emit_state(AgentStatus::Scoring, &branch, attempt_num).await;
            let score = self.score(&test_output, &lint_output, &diff);

            // 7. DIFF SAFETY CHECK
            let verdict = self.git.check_diff(&diff).await?;
            if matches!(verdict, DiffVerdict::TouchesOffLimits { .. } | DiffVerdict::TooLarge { .. }) {
                error!(agent=%self.id, ?verdict, "Diff failed safety check — rolling back");
                self.git.rollback(&branch).await?;
                let attempt = self.build_attempt(&task, &branch, attempt_num, &plan, &diff,
                    &test_output, &lint_output, score.total,
                    AttemptOutcome::RolledBack { reason: format!("{:?}", verdict) }).await;
                self.memory.log_attempt(&attempt).await?;
                return Err(LopiError::DiffSafetyViolation);
            }

            // 8. DECIDE
            if score.total >= self.config.score_threshold {
                // ACCEPT — open PR
                self.emit_state(AgentStatus::OpeningPr, &branch, attempt_num).await;
                let pr_url = self.git.open_pr(&branch, &task).await?;
                let attempt = self.build_attempt(&task, &branch, attempt_num, &plan, &diff,
                    &test_output, &lint_output, score.total,
                    AttemptOutcome::Accepted { pr_url: pr_url.clone() }).await;
                self.memory.log_attempt(&attempt).await?;
                return Ok(attempt);

            } else if attempt_num >= task.max_retries {
                // OUT OF RETRIES — rollback
                self.git.rollback(&branch).await?;
                let attempt = self.build_attempt(&task, &branch, attempt_num, &plan, &diff,
                    &test_output, &lint_output, score.total,
                    AttemptOutcome::RolledBack { reason: "max retries exceeded".into() }).await;
                self.memory.log_attempt(&attempt).await?;
                return Err(LopiError::MaxRetriesExceeded { score: score.total });

            } else {
                // RETRY
                warn!(agent=%self.id, score=score.total, "Score below threshold, retrying");
                self.git.rollback(&branch).await?;
                let attempt = self.build_attempt(&task, &branch, attempt_num, &plan, &diff,
                    &test_output, &lint_output, score.total,
                    AttemptOutcome::Retried { reason: format!("score {:.2} < {:.2}", score.total, self.config.score_threshold) }).await;
                self.memory.log_attempt(&attempt).await?;
                self.emit_state(AgentStatus::Retrying { attempt: attempt_num + 1 }, &branch, attempt_num).await;
                // loop continues — next iteration will pick up patterns from this failed attempt
            }
        }
    }

    async fn invoke_claude_plan(&self, task: &Task, memory_ctx: &str) -> Result<String, LopiError> {
        let prompt = format!(
            "You are working on: {}\n\nTarget: {}\n\nPrevious attempt context:\n{}\n\nCreate a concise implementation plan. Think step by step.",
            task.description, task.target_path, memory_ctx
        );
        self.invoke_claude_raw("--print", &prompt).await
    }

    async fn invoke_claude_implement(&self, task: &Task, plan: &str) -> Result<(), LopiError> {
        let prompt = format!(
            "Implement the following plan for {}:\n\n{}\n\nTask: {}",
            task.target_path, plan, task.description
        );
        self.invoke_claude_raw("", &prompt).await?;
        Ok(())
    }

    async fn invoke_claude_raw(&self, flags: &str, prompt: &str) -> Result<String, LopiError> {
        let mut cmd = tokio::process::Command::new(&self.config.claude_bin);
        if !flags.is_empty() { cmd.arg(flags); }
        cmd.arg("-p").arg(prompt)
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(self.config.timeout_secs),
            cmd.output()
        ).await
        .map_err(|_| LopiError::AgentTimeout)?
        .map_err(LopiError::Io)?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(LopiError::ClaudeError(String::from_utf8_lossy(&output.stderr).to_string()))
        }
    }

    fn score(&self, test_out: &str, lint_out: &str, diff: &str) -> ScoreBreakdown {
        let test_pass_rate = parse_test_pass_rate(test_out);
        let lint_clean = if lint_out.trim().is_empty() { 1.0 } else { 0.6 };
        let diff_lines = diff.lines().filter(|l| l.starts_with('+') || l.starts_with('-')).count() as f64;
        let diff_size_score = (1.0 - (diff_lines / 500.0)).max(0.0).min(1.0);
        let total = test_pass_rate * 0.50 + lint_clean * 0.25 + diff_size_score * 0.25;
        ScoreBreakdown { test_pass_rate, lint_clean, diff_size_score, total }
    }

    fn format_memory_context(&self, patterns: &[Pattern]) -> String {
        if patterns.is_empty() {
            return "No prior attempts for this path.".into();
        }
        patterns.iter().map(|p| format!("[{}] {}", match p.kind {
            PatternKind::Success => "✓",
            PatternKind::Failure => "✗",
            PatternKind::Warning => "⚠",
        }, p.description)).collect::<Vec<_>>().join("\n")
    }

    async fn emit_state(&self, status: AgentStatus, branch: &str, attempt: u8) {
        let _ = self.state_tx.send(AgentState {
            id: self.id,
            task_id: None, // set by orchestrator
            status,
            branch: Some(branch.to_string()),
            attempt,
            last_score: None,
            log_tail: vec![],
            started_at: None,
            updated_at: chrono::Utc::now(),
        });
    }
}

fn parse_test_pass_rate(output: &str) -> f64 {
    // Parse `cargo test` output: "test result: ok. 42 passed; 3 failed"
    // Also handles nextest, pytest-style output
    for line in output.lines() {
        if line.contains("passed") && line.contains("failed") {
            let passed: f64 = extract_num(line, "passed").unwrap_or(0.0);
            let failed: f64 = extract_num(line, "failed").unwrap_or(0.0);
            let total = passed + failed;
            if total > 0.0 { return passed / total; }
        }
        if line.contains("ok") && line.contains("passed") {
            return 1.0;
        }
    }
    0.5 // unknown — neutral score
}

fn extract_num(s: &str, before_keyword: &str) -> Option<f64> {
    s.split_whitespace()
        .zip(s.split_whitespace().skip(1))
        .find(|(_, b)| b.starts_with(before_keyword))
        .and_then(|(a, _)| a.parse().ok())
}
```

---

## 9. Concurrency Design

### Agent Pool (`lopi-orchestrator/src/pool.rs`)

```rust
use dashmap::DashMap;
use tokio::sync::mpsc;
use std::sync::Arc;

pub struct AgentPool {
    pub config:  AgentConfig,
    pub agents:  Arc<DashMap<Uuid, AgentState>>,     // live state, lock-free reads
    pub tx:      mpsc::Sender<Task>,                  // send tasks to worker loop
}

impl AgentPool {
    pub fn new(config: AgentConfig) -> (Self, mpsc::Receiver<Task>) {
        let (tx, rx) = mpsc::channel(256);
        let pool = Self {
            config,
            agents: Arc::new(DashMap::new()),
            tx,
        };
        (pool, rx)
    }

    /// Spawns the worker loop — consumes tasks from channel, spawns agents up to max_agents
    pub fn start(self: Arc<Self>, mut rx: mpsc::Receiver<Task>) {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.max_agents));

        tokio::spawn(async move {
            while let Some(task) = rx.recv().await {
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let pool   = self.clone();

                tokio::spawn(async move {
                    let agent_id = Uuid::new_v4();
                    pool.agents.insert(agent_id, AgentState::idle(agent_id));

                    let runner = AgentRunner::new(agent_id, pool.config.clone(), /* deps */);
                    match runner.run(task).await {
                        Ok(attempt)  => pool.on_success(agent_id, attempt).await,
                        Err(e)       => pool.on_failure(agent_id, e).await,
                    }

                    pool.agents.remove(&agent_id);
                    drop(permit); // releases semaphore slot — next task can start
                });
            }
        });
    }
}
```

### State Sharing Strategy

| State | Mechanism | Rationale |
|-------|-----------|-----------|
| Agent live states | `Arc<DashMap<Uuid, AgentState>>` | Lock-free concurrent reads for TUI/web |
| Task queue | `tokio::sync::mpsc::channel` | Back-pressure aware, natural async queue |
| Max concurrency | `tokio::sync::Semaphore` | Clean N-permit concurrency cap |
| Config | `Arc<LopiConfig>` + `arc_swap::ArcSwap` | Hot-reload config without restart |
| WebSocket broadcasts | `tokio::sync::broadcast::channel` | Fan-out to N connected browser clients |
| DB pool | `sqlx::SqlitePool` | Already async-safe, shared via `Arc` |

---

## 10. Git Module (`lopi-git/src/`)

### Branch Isolation (`branch.rs`)

```rust
use git2::{Repository, BranchType};

pub struct GitManager {
    pub repo_path:   String,
    pub base_branch: String,
    pub config:      GitConfig,
    pub diff_policy: DiffPolicy,
}

impl GitManager {
    pub async fn create_branch(&self, name: &str) -> Result<(), LopiError> {
        let repo_path = self.repo_path.clone();
        let base = self.base_branch.clone();
        let name = name.to_string();

        tokio::task::spawn_blocking(move || {
            let repo = Repository::open(&repo_path)?;
            let head = repo.head()?.peel_to_commit()?;
            // Ensure we branch from base, not wherever HEAD is
            let base_ref = repo.find_branch(&base, BranchType::Local)?;
            let base_commit = base_ref.get().peel_to_commit()?;
            repo.branch(&name, &base_commit, false)?;
            // Checkout
            let obj = repo.revparse_single(&format!("refs/heads/{}", name))?;
            repo.checkout_tree(&obj, None)?;
            repo.set_head(&format!("refs/heads/{}", name))?;
            Ok::<_, git2::Error>(())
        }).await??;
        Ok(())
    }

    pub async fn rollback(&self, branch: &str) -> Result<(), LopiError> {
        let repo_path = self.repo_path.clone();
        let branch = branch.to_string();
        let base = self.base_branch.clone();
        tokio::task::spawn_blocking(move || {
            let repo = Repository::open(&repo_path)?;
            // Hard reset to base branch HEAD
            let base_ref = repo.find_branch(&base, BranchType::Local)?;
            let base_commit = base_ref.get().peel_to_commit()?;
            repo.reset(base_commit.as_object(), git2::ResetType::Hard, None)?;
            // Checkout base
            let obj = repo.revparse_single(&format!("refs/heads/{}", base))?;
            repo.checkout_tree(&obj, None)?;
            repo.set_head(&format!("refs/heads/{}", base))?;
            // Delete the attempt branch
            if let Ok(mut b) = repo.find_branch(&branch, BranchType::Local) {
                let _ = b.delete();
            }
            Ok::<_, git2::Error>(())
        }).await??;
        Ok(())
    }

    pub async fn get_diff(&self, branch: &str) -> Result<String, LopiError> {
        // git diff base_branch..branch — via tokio::process for simplicity
        let output = tokio::process::Command::new("git")
            .args(["-C", &self.repo_path, "diff", &self.base_branch, branch])
            .output().await?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
```

### Diff Checker (`diff.rs`)

```rust
pub struct DiffPolicy {
    pub max_lines:       u32,           // e.g. 500
    pub off_limits:      Vec<glob::Pattern>, // compiled from config strings
}

impl DiffChecker for GitManager {
    async fn check(&self, diff: &str, policy: &DiffPolicy) -> Result<DiffVerdict, LopiError> {
        let changed_files: Vec<&str> = diff.lines()
            .filter(|l| l.starts_with("--- a/") || l.starts_with("+++ b/"))
            .filter_map(|l| l.split_once("b/").map(|(_, f)| f))
            .collect();

        // Check off-limits file patterns
        for file in &changed_files {
            for pattern in &policy.off_limits {
                if pattern.matches(file) {
                    return Ok(DiffVerdict::TouchesOffLimits {
                        files: vec![file.to_string()],
                    });
                }
            }
        }

        // Check diff size
        let lines_changed = diff.lines()
            .filter(|l| l.starts_with('+') || l.starts_with('-'))
            .filter(|l| !l.starts_with("+++") && !l.starts_with("---"))
            .count() as u32;

        if lines_changed > policy.max_lines {
            return Ok(DiffVerdict::TooLarge { lines: lines_changed });
        }

        Ok(DiffVerdict::Safe)
    }
}
```

---

## 11. Memory Module (`lopi-memory/`)

### Schema (`migrations/001_initial.sql`)

```sql
CREATE TABLE IF NOT EXISTS attempts (
    id            TEXT PRIMARY KEY,
    task_id       TEXT NOT NULL,
    agent_id      TEXT NOT NULL,
    attempt_num   INTEGER NOT NULL,
    branch        TEXT NOT NULL,
    plan          TEXT NOT NULL,
    diff          TEXT NOT NULL,
    diff_size     INTEGER NOT NULL,
    test_output   TEXT NOT NULL,
    lint_output   TEXT NOT NULL,
    score         REAL NOT NULL,
    outcome_kind  TEXT NOT NULL,    -- 'accepted' | 'retried' | 'rolled_back'
    outcome_data  TEXT NOT NULL,    -- JSON
    started_at    TEXT NOT NULL,
    finished_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS patterns (
    id          TEXT PRIMARY KEY,
    path_glob   TEXT NOT NULL,
    kind        TEXT NOT NULL,    -- 'success' | 'failure' | 'warning'
    description TEXT NOT NULL,
    weight      REAL NOT NULL DEFAULT 1.0,
    source_attempt TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE INDEX idx_attempts_task_id ON attempts(task_id);
CREATE INDEX idx_patterns_path ON patterns(path_glob);
```

### Pattern Learning (`patterns.rs`)

```rust
/// After each attempt, extract patterns to inform next attempt.
pub async fn extract_and_store_patterns(
    pool: &SqlitePool,
    attempt: &Attempt,
    task: &Task,
) -> Result<(), LopiError> {
    let patterns = match &attempt.outcome {
        AttemptOutcome::Accepted { .. } => vec![
            Pattern {
                kind: PatternKind::Success,
                description: format!(
                    "Score {:.2} achieved on '{}' with plan: {}",
                    attempt.score,
                    task.target_path,
                    &attempt.plan[..attempt.plan.len().min(200)]
                ),
                weight: 1.0,
            }
        ],
        AttemptOutcome::Retried { reason } => vec![
            Pattern {
                kind: PatternKind::Failure,
                description: format!("Retry needed: {}. Test output snippet: {}",
                    reason, &attempt.test_output[..attempt.test_output.len().min(300)]),
                weight: 0.8,
            }
        ],
        AttemptOutcome::RolledBack { reason } => vec![
            Pattern {
                kind: PatternKind::Failure,
                description: format!("Hard rollback: {}", reason),
                weight: 1.2,
            }
        ],
    };

    for pattern in patterns {
        sqlx::query!(
            "INSERT INTO patterns (id, path_glob, kind, description, weight, source_attempt, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            Uuid::new_v4().to_string(),
            task.target_path,
            format!("{:?}", pattern.kind).to_lowercase(),
            pattern.description,
            pattern.weight,
            attempt.id.to_string(),
            chrono::Utc::now().to_rfc3339()
        ).execute(pool).await?;
    }
    Ok(())
}
```

---

## 12. TUI (`lopi-ui/src/tui/`)

```rust
// widgets.rs — core layout
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Table, Row},
    Frame,
};

pub fn render(f: &mut Frame, state: &TuiAppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // Header
            Constraint::Min(0),      // Main area
            Constraint::Length(3),   // Status bar
        ])
        .split(f.size());

    render_header(f, chunks[0], state);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    render_agent_panel(f, main_chunks[0], state);
    render_log_panel(f, main_chunks[1], state);
    render_status_bar(f, chunks[2], state);
}

fn render_agent_panel(f: &mut Frame, area: ratatui::layout::Rect, state: &TuiAppState) {
    let rows: Vec<Row> = state.agents.iter().map(|agent| {
        let status_style = match &agent.status {
            AgentStatus::Implementing => Style::default().fg(Color::Yellow),
            AgentStatus::Done         => Style::default().fg(Color::Green),
            AgentStatus::Failed       => Style::default().fg(Color::Red),
            AgentStatus::RollingBack  => Style::default().fg(Color::Magenta),
            _                         => Style::default().fg(Color::Cyan),
        };
        Row::new(vec![
            agent.id.to_string()[..8].to_string(),
            format!("{:?}", agent.status),
            agent.branch.clone().unwrap_or_default(),
            format!("A{}", agent.attempt),
            agent.last_score.map(|s| format!("{:.2}", s)).unwrap_or_default(),
        ]).style(status_style)
    }).collect();

    let table = Table::new(rows, [
        Constraint::Length(10), Constraint::Length(16), Constraint::Min(20),
        Constraint::Length(4),  Constraint::Length(6),
    ])
    .header(Row::new(["ID", "Status", "Branch", "Try", "Score"])
        .style(Style::default().add_modifier(Modifier::BOLD)))
    .block(Block::default().borders(Borders::ALL).title("◉ Agents"));

    f.render_widget(table, area);
}
```

---

## 13. Web Dashboard (`lopi-ui/src/web/`)

```rust
// server.rs
use axum::{Router, routing::{get, post}};
use tower_http::cors::CorsLayer;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // REST API
        .route("/api/tasks",        get(routes::list_tasks).post(routes::create_task))
        .route("/api/tasks/:id",    get(routes::get_task).delete(routes::cancel_task))
        .route("/api/agents",       get(routes::list_agents))
        .route("/api/runs",         get(routes::list_runs))
        .route("/api/logs/:id",     get(routes::get_logs))
        // WebSocket — live state updates
        .route("/ws",               get(ws::ws_handler))
        // Static dashboard
        .route("/",                 get(serve_dashboard))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ws.rs — broadcast agent state changes to all connected clients
pub async fn ws_handler(
    ws: axum::extract::WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: axum::extract::ws::WebSocket, state: Arc<AppState>) {
    let mut rx = state.broadcast_tx.subscribe();
    let (mut sender, _) = socket.split();

    // Send initial snapshot
    let snapshot = serde_json::to_string(&state.current_snapshot()).unwrap();
    let _ = sender.send(axum::extract::ws::Message::Text(snapshot)).await;

    // Stream updates
    while let Ok(update) = rx.recv().await {
        let msg = serde_json::to_string(&update).unwrap();
        if sender.send(axum::extract::ws::Message::Text(msg)).await.is_err() {
            break;
        }
    }
}
```

---

## 14. Phone Remote Control

### Flow Diagram

```
User phone
  │
  │  /task Add GitHub retry logic to src/retry.rs
  ▼
Telegram Bot (teloxide)  ──or──  WhatsApp Webhook (Twilio → axum)
  │
  ▼
lopi-remote/src/commands.rs
  parse_command() → TaskRequest { title, target_path, priority }
  │
  ▼
Orchestrator::enqueue(task)
  │
  ├──► Agent Pool picks up task
  │      ↓ Plan → Implement → Test → Score → PR
  │
  └──► Remote gateway notified at each phase:
         "🔄 Agent starting attempt 1 on lopi/task-abc/1"
         "🧪 Tests running..."
         "✅ Score 0.89 — opening PR"
         "🎉 PR #42 opened: https://github.com/KonjoAI/lopi/pull/42"
         (with inline approve button via Telegram keyboard)
```

### Telegram Bot (`lopi-remote/src/telegram.rs`)

```rust
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Lopi remote commands")]
pub enum LopiCommand {
    #[command(description = "Add a task: /task <description> | <path>")]
    Task(String),
    #[command(description = "Show agent and queue status")]
    Status,
    #[command(description = "List recent runs")]
    Runs,
    #[command(description = "Cancel a task by ID")]
    Cancel(String),
    #[command(description = "Approve PR for a run")]
    Approve(String),
}

pub async fn start_telegram_bot(config: RemoteConfig, orchestrator: Arc<dyn Orchestrator>) {
    let bot = Bot::new(config.telegram_token.unwrap());
    let allowed = config.allowed_phone_numbers.clone();

    Command::repl(bot, move |bot: Bot, msg: Message, cmd: LopiCommand| {
        let orchestrator = orchestrator.clone();
        let allowed = allowed.clone();
        async move {
            // Auth check
            if !is_allowed(&msg, &allowed) {
                bot.send_message(msg.chat.id, "⛔ Not authorized").await?;
                return Ok(());
            }
            match cmd {
                LopiCommand::Task(args) => handle_task(bot, msg, args, orchestrator).await,
                LopiCommand::Status     => handle_status(bot, msg, orchestrator).await,
                LopiCommand::Runs       => handle_runs(bot, msg, orchestrator).await,
                LopiCommand::Cancel(id) => handle_cancel(bot, msg, id, orchestrator).await,
                LopiCommand::Approve(id)=> handle_approve(bot, msg, id, orchestrator).await,
            }
        }
    }).await;
}

async fn handle_task(bot: Bot, msg: Message, args: String, orch: Arc<dyn Orchestrator>)
    -> ResponseResult<()>
{
    // Parse "Add retry logic to src/retry.rs" or "Add retry logic | src/"
    let (description, path) = if let Some((d, p)) = args.split_once(" | ") {
        (d.trim().to_string(), p.trim().to_string())
    } else {
        (args.clone(), ".".to_string())
    };

    let task = Task {
        id: Uuid::new_v4(),
        title: description.clone(),
        description,
        target_path: path,
        priority: TaskPriority::Normal,
        status: TaskStatus::Queued,
        source: TaskSource::Telegram {
            user_id: msg.from().map(|u| u.id.0 as i64).unwrap_or(0),
            username: msg.from().and_then(|u| u.username.clone()),
        },
        max_retries: 3,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        tags: vec![],
    };

    let task_id = orch.enqueue(task).await?;
    bot.send_message(msg.chat.id,
        format!("✅ Task queued — ID: `{}`\nI'll update you as it runs.", &task_id.to_string()[..8])
    ).await?;
    Ok(())
}
```

---

## 15. Self-Modification Safety

### Rules

| Rule | Mechanism |
|------|-----------|
| Lopi cannot modify its own source | `off_limits_dirs: ["src/", "crates/"]` in safety config |
| Lock files are immutable | `off_limits_files: ["Cargo.lock", "*.lock", "*.pem", ".env*"]` |
| Max diff size | `max_diff_lines: 500` — prevents runaway rewrites |
| Tests must pass | `require_tests_pass: true` — score of 0 if any test fails |
| Hard rollback on violation | `DiffVerdict::TouchesOffLimits` → immediate rollback, no retry |
| Branch isolation | Each attempt is on its own `lopi/<task_id>/<n>` branch — base is never touched |
| PR gating | Lopi opens PRs, it never merges them — human must approve |
| Audit trail | Every diff, plan, test output stored in SQLite forever |

### Safety Check Flow

```
Diff generated
    │
    ├─► scan changed file list
    │       ├─► off-limits glob match? → ROLLBACK immediately
    │       └─► passes → continue
    │
    ├─► count ±lines
    │       ├─► > max_diff_lines? → ROLLBACK immediately
    │       └─► passes → continue
    │
    ├─► run tests
    │       ├─► require_tests_pass=true AND any failure? → score penalty (may cause retry)
    │       └─► passes → score bonus
    │
    └─► total score >= threshold? → open PR | retry | rollback
```

---

## 16. CI & GitHub Webhook Integration

### Webhook Handler (`lopi-webhook/src/github.rs`)

```rust
use axum::{Router, routing::post, extract::{State, Json}, http::HeaderMap};
use serde::Deserialize;
use sha2::Sha256;
use hmac::{Hmac, Mac};

#[derive(Deserialize)]
pub struct GitHubEvent {
    pub action:      Option<String>,
    pub workflow_run: Option<WorkflowRun>,
    pub pull_request: Option<PullRequest>,
}

#[derive(Deserialize)]
pub struct WorkflowRun {
    pub id:         u64,
    pub name:       String,
    pub conclusion: Option<String>,  // "failure" | "success" | null
    pub head_branch: String,
}

pub async fn github_webhook(
    headers: HeaderMap,
    State(state): State<Arc<WebhookState>>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    // Verify HMAC-SHA256 signature
    if !verify_signature(&headers, &body, &state.config.webhook_secret.as_deref().unwrap_or("")) {
        return (axum::http::StatusCode::UNAUTHORIZED, "bad signature").into_response();
    }

    let event: GitHubEvent = serde_json::from_slice(&body).unwrap_or_default();
    let event_kind = headers.get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok()).unwrap_or("").to_string();

    // CI failure → inject retry task
    if event_kind == "workflow_run" {
        if let Some(run) = event.workflow_run {
            if run.conclusion.as_deref() == Some("failure") {
                let task = Task {
                    title: format!("Fix CI failure in {} (run {})", run.name, run.id),
                    description: format!(
                        "CI workflow '{}' failed on branch '{}'. Investigate and fix.",
                        run.name, run.head_branch
                    ),
                    target_path: "src/".into(),
                    source: TaskSource::CiFailure { run_id: run.id, job: run.name },
                    priority: TaskPriority::High,
                    ..Task::default()
                };
                let _ = state.orchestrator.enqueue(task).await;
            }
        }
    }

    (axum::http::StatusCode::OK, "ok").into_response()
}

fn verify_signature(headers: &HeaderMap, body: &[u8], secret: &str) -> bool {
    let sig = headers.get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("sha256="))
        .unwrap_or("");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let expected = hex::encode(mac.finalize().into_bytes());
    sig == expected
}
```

### GitHub Actions — Lopi Trigger

```yaml
# .github/workflows/lopi-trigger.yml
name: Notify Lopi on Failure

on:
  workflow_run:
    workflows: ["CI"]
    types: [completed]

jobs:
  notify-lopi:
    if: ${{ github.event.workflow_run.conclusion == 'failure' }}
    runs-on: ubuntu-latest
    steps:
      - name: Send webhook to Lopi
        run: |
          curl -X POST ${{ secrets.LOPI_WEBHOOK_URL }}/webhooks/github \
            -H "Content-Type: application/json" \
            -H "X-Hub-Signature-256: $(echo -n '${{ toJson(github.event) }}' | \
                openssl dgst -sha256 -hmac '${{ secrets.LOPI_WEBHOOK_SECRET }}' | \
                awk '{print "sha256="$2}')" \
            -d '${{ toJson(github.event) }}'
```

---

## 17. Main Entry Point (`src/main.rs`)

```rust
use clap::{Parser, Subcommand};
use lopi_core::config::LopiConfig;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "lopi", about = "KonjoAI agent orchestrator", version)]
struct Cli {
    #[arg(short, long, default_value = "lopi.toml")]
    config: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start lopi with TUI + web dashboard
    Run,
    /// Open TUI only
    Tui,
    /// Start web server only
    Web,
    /// Inject a task from the CLI
    Task {
        #[arg(short, long)]
        title: String,
        #[arg(short, long, default_value = ".")]
        path: String,
        #[arg(short = 'P', long, default_value = "normal")]
        priority: String,
    },
    /// Show current status
    Status,
    /// Show recent attempt history
    History {
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load config
    let config: LopiConfig = config::Config::builder()
        .add_source(config::File::with_name(&cli.config).required(false))
        .add_source(config::Environment::with_prefix("LOPI"))
        .build()?
        .try_deserialize()?;

    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Init memory
    let memory = Arc::new(lopi_memory::SqliteMemory::connect(&config.memory.db_path).await?);

    // Init git manager
    let git = Arc::new(lopi_git::GitManager::new(config.git.clone()));

    // Init orchestrator + agent pool
    let (pool, rx) = lopi_orchestrator::AgentPool::new(config.agent.clone());
    let pool = Arc::new(pool);
    pool.clone().start(rx);

    // Broadcast channel for UI updates
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(1024);

    let app_state = Arc::new(AppState {
        config: config.clone(),
        pool: pool.clone(),
        memory: memory.clone(),
        broadcast_tx: broadcast_tx.clone(),
    });

    match cli.command {
        Commands::Run => {
            // Start everything concurrently
            tokio::select! {
                r = lopi_ui::web::serve(app_state.clone(), config.ui.web_port) => r?,
                r = lopi_ui::tui::run(app_state.clone()) => r?,
                r = start_remote(app_state.clone(), &config.remote) => r?,
                r = start_webhooks(app_state.clone(), &config.ci) => r?,
            }
        }
        Commands::Web => lopi_ui::web::serve(app_state, config.ui.web_port).await?,
        Commands::Tui => lopi_ui::tui::run(app_state).await?,
        Commands::Task { title, path, priority } => {
            let task = build_task(title, path, priority);
            let id = pool.enqueue(task).await?;
            println!("✅ Task queued: {}", id);
        }
        Commands::Status  => print_status(app_state).await?,
        Commands::History { limit } => print_history(memory, limit).await?,
    }

    Ok(())
}
```

---

## 18. Phase Breakdown

### Phase 1 — MVP (Weeks 1–3)

**Goal**: One agent, one task, working end-to-end loop with memory and git safety.

- `lopi-core`: models, traits, config, error types
- `lopi-git`: `create_branch`, `rollback`, `get_diff`, `check_diff`
- `lopi-agent`: `runner.rs` with `plan → implement → test → score → retry/rollback`
- `lopi-memory`: SQLite schema + `log_attempt` + `get_patterns`
- `lopi-orchestrator`: single-agent queue (no pool yet)
- `src/main.rs`: `lopi task` + `lopi run` (headless, logs to stdout)
- **Test**: port the Python prototype's `task-001.json` task and run it end-to-end

**Deliverables**: `lopi task --title "Add error handling to src/lib.rs" --path src/lib.rs` works, produces a git branch with code + a scored attempt in the DB.

---

### Phase 2 — Concurrency + UI (Weeks 4–6)

**Goal**: N parallel agents, live visibility.

- `lopi-orchestrator`: `AgentPool` with `Semaphore`, `DashMap`, full `tokio::spawn` loop
- `lopi-ui/tui`: ratatui dashboard — agents panel, task queue, log tail
- `lopi-ui/web`: axum server + WebSocket + basic HTML dashboard
- PR creation via `octocrab`
- Tracing → structured JSON logs → web log stream

**Deliverables**: Run 3 parallel agents from CLI, watch them in TUI and browser.

---

### Phase 3 — Phone + Webhooks (Weeks 7–9)

**Goal**: Fully remotely controllable from phone; auto-triggered by CI.

- `lopi-remote/telegram`: `teloxide` bot with `/task`, `/status`, `/approve`
- `lopi-remote/whatsapp`: Twilio webhook handler
- `lopi-webhook/github`: CI failure → task injection with HMAC verification
- Auth: allowlist of Telegram chat IDs / phone numbers
- Notification flow: bot messages at Planning, Testing, PR-opened, Failure

**Deliverables**: Send `/task Fix the flaky auth tests | src/auth/` from Telegram, get back PR link 10 minutes later.

---

### Phase 4 — Self-Improvement + Advanced Memory (Weeks 10–12)

**Goal**: System gets measurably better over time.

- Pattern learning: after N attempts on a path, auto-adjust prompts based on `patterns` table
- Scoring evolution: weight past success patterns in plan prompt
- Hot config reload: `arc_swap` — change `score_threshold` without restart
- Advanced diff analysis: semantic diff (not just line count) using `similar` crate
- Task dependency graph: `task A` must succeed before `task B` starts
- Scheduled tasks: cron-style recurring checks (e.g., nightly lint sweep)

**Deliverables**: On path `src/auth/`, after 5 prior attempts, Lopi's plans visibly incorporate lessons (shown in TUI memory panel).

---

### Phase 5 — Production Hardening (Ongoing)

- Docker image + `compose.yml` (lopi + SQLite volume)
- GitHub App (not PAT) for webhook auth + PR creation
- Rate limiting on agent pool (avoid GitHub API abuse)
- Encrypted SQLite (sqlcipher feature)
- Multi-repo support (multiple `GitManager` instances)
- Lopi self-hosting: run Lopi against the Lopi repo itself (meta!)

---

## 19. lopi.toml Example

```toml
[agent]
max_agents      = 4
max_retries     = 3
timeout_secs    = 300
claude_bin      = "claude"
score_threshold = 0.75

[git]
repo_path    = "/path/to/your/repo"
base_branch  = "main"
remote       = "origin"
branch_prefix = "lopi/"

[memory]
db_path              = "lopi.db"
max_history_per_path = 50

[ui]
web_port    = 7070
tui_enabled = true
web_enabled = true

[remote]
telegram_token    = ""  # set via LOPI_REMOTE_TELEGRAM_TOKEN env var
telegram_chat_id  = 0
allowed_phone_numbers = ["+1234567890"]

[safety]
off_limits_files = ["Cargo.lock", "*.pem", ".env*", "*.key", "*.secret"]
off_limits_dirs  = ["infra/", "scripts/", ".github/workflows/"]
max_diff_lines   = 500
require_tests_pass = true
allow_self_modify  = false

[ci]
github_repo    = "KonjoAI/lopi"
webhook_port   = 7071
```

---

## 20. Environment Variables (`.env.example`)

```bash
# Claude Code CLI path (if not in PATH)
LOPI_AGENT__CLAUDE_BIN=claude

# Remote control
LOPI_REMOTE__TELEGRAM_TOKEN=your_telegram_bot_token
LOPI_REMOTE__TELEGRAM_CHAT_ID=123456789
LOPI_REMOTE__TWILIO_ACCOUNT_SID=ACxxxxx
LOPI_REMOTE__TWILIO_AUTH_TOKEN=xxxxx
LOPI_REMOTE__TWILIO_FROM_NUMBER=+1555000000

# GitHub
LOPI_CI__GITHUB_TOKEN=ghp_xxxxx
LOPI_CI__WEBHOOK_SECRET=your_hmac_secret

# Logging
RUST_LOG=lopi=debug,tower_http=info
```

---

## 21. Key Design Decisions (Justifications)

**Why Rust?** Claude Code agent processes are I/O-bound waiting on Claude, tests, git. Tokio's async model means N agents costs N lightweight tasks, not N OS threads. Memory safety means no data races across the shared `DashMap` state. Binary ships as a single executable with zero runtime dependencies.

**Why tokio::process for Claude Code?** Claude Code is a CLI tool — invoking it via `tokio::process::Command` is the cleanest, most future-proof interface. It avoids any coupling to Claude Code's internals and works regardless of how Claude Code is installed.

**Why SQLite over Postgres?** Local-first. Users run Lopi on their dev machine or a single server. SQLite with WAL mode handles the write load of N concurrent agents logging attempts with zero ops overhead. Upgrade path to Postgres is a sqlx driver swap.

**Why DashMap for agent state?** The TUI and WebSocket both need to read agent state at 10Hz with minimal latency. `DashMap` is a lock-free concurrent hashmap — reads never block even when an agent is writing its state update.

**Why `arc_swap` for config?** Agents read config on every attempt. A global `Arc<LopiConfig>` forces a clone on read. `ArcSwap` gives atomic pointer swap for hot-reload with zero-cost reads.

**Why Git2 + tokio::process for git?** `git2` handles complex operations (branch creation, reset) with proper error handling. For simple operations like `git diff`, `tokio::process::Command::new("git")` is faster to write and sufficient.

---

*This document was generated as the master plan for `lopi` — KonjoAI's Rust agent orchestrator.*
*Know the problem. Outline the solution. Nail the build. Justify the claims. Optimize the output.*
