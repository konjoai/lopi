//! CLI surface — argument parsing only. Handlers live in the `*_commands`
//! modules; `main.rs` just matches `Commands` and dispatches.
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::skill_commands::SkillCmd;
use crate::worktree_commands::WorktreeCmd;

#[derive(Parser)]
#[command(
    name = "lopi",
    version,
    about = "⛵ Konjo agent orchestrator — beautiful, excellent, provably correct."
)]
pub(crate) struct Cli {
    /// Path to config file (default: ./lopi.toml, then ~/.lopi/lopi.toml)
    #[arg(long, global = true)]
    pub(crate) config: Option<PathBuf>,

    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Run an agent task immediately and stream status to stdout
    Run {
        #[arg(short, long)]
        goal: String,
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        /// Print the plan and exit without making any changes
        #[arg(long)]
        dry_run: bool,
        /// Apply plan steps speculatively as they stream (reduces wall-clock time)
        #[arg(long)]
        speculative: bool,
        /// Enable Reflexion-style adaptive retry: inject previous attempt's error into the next planning prompt
        #[arg(long)]
        adaptive_retry: bool,
        /// Run the Layer 5 stability gate: generate N plan samples and block if variance is too high.
        #[arg(long)]
        stability_gate: bool,
        /// One-off per-`claude -p` session USD cap for this run, overriding
        /// the repo's `.lopi/loop.toml` budget (e.g. `--budget 5`). `0`
        /// disables the cap.
        #[arg(long)]
        budget: Option<f64>,
        /// One-off named budget preset for this run (quick/standard/deep/
        /// unlimited), overriding the repo's `.lopi/loop.toml` preset.
        #[arg(long)]
        budget_preset: Option<String>,
        /// One-off per-run token budget for this run, overriding the repo's
        /// `.lopi/loop.toml` budget. `0` disables the cap.
        #[arg(long)]
        budget_tokens: Option<u64>,
    },
    /// Run with directory restrictions disabled — use in trusted environments only.
    ///
    /// Equivalent to `claude --dangerously-skip-permissions`.
    /// All allowed_dirs / forbidden_dirs policies are bypassed for this run.
    Bypass {
        /// Goal to execute. Enclose in quotes or pass as separate words.
        #[arg(num_args = 1.., trailing_var_arg = true)]
        goal_args: Vec<String>,
    },
    /// Watch live agent status (TUI). Use --remote to connect to a running sail server.
    Watch {
        /// Connect to a running lopi sail server WebSocket instead of a local bus.
        #[arg(long, default_value = "ws://127.0.0.1:3000/ws")]
        remote: Option<String>,
        /// Use a local bus only (ignore any running sail server).
        #[arg(long)]
        local: bool,
    },
    /// Tail agent events (history or live)
    Tail {
        #[arg(short, long)]
        task_id: Option<String>,
        #[arg(long)]
        history: bool,
    },
    /// List all tasks and their status from the database
    Dock,
    /// Start the web dashboard + agent pool (single or multi-repo).
    Sail {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "4")]
        max_agents: usize,
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        /// Additional repo paths to watch concurrently (multi-repo mode).
        #[arg(long, value_delimiter = ',')]
        repos: Vec<PathBuf>,
    },
    /// Cancel a running task by ID prefix
    Cancel {
        #[arg()]
        task_id: String,
    },
    /// Load the most-recent checkpoint for an agent and print its stored state.
    Resume {
        #[arg(long)]
        agent_id: String,
    },
    /// Browse the mined pattern library
    #[command(subcommand)]
    Learn(LearnCmd),
    /// Manage scheduled tasks
    #[command(subcommand)]
    Schedules(ScheduleCmd),
    /// Loop engineering — inspect and validate a repo's `.lopi/loop.toml`.
    #[command(subcommand)]
    Loop(LoopCmd),
    /// Manage per-task git worktrees — list live ones, gc the leftovers.
    #[command(subcommand)]
    Worktree(WorktreeCmd),
    /// Skills — promote recurring lessons into reviewable skill drafts.
    #[command(subcommand)]
    Skill(SkillCmd),
    /// Browse the Layer 5 patch stability ledger
    #[command(subcommand)]
    Stability(StabilityCmd),
    /// Continuously run gap-fill on a cadence — the Kitchen Loop daemon.
    WatchGapFill {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "60")]
        interval: u64,
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        sail_url: String,
        #[arg(long)]
        run_now: bool,
    },
    /// Show trust calibration stats
    Trust,
    /// Inspect a task's DAG trace and show the partial-restart replay plan.
    Replay {
        /// Task ID (full UUID) to replay.
        #[arg(long)]
        task: String,
        /// Restart from this pipeline stage (plan/implement/test/score/verify/diff/pr).
        #[arg(long)]
        from: Option<String>,
        /// Show the plan without re-executing (the current default behaviour).
        #[arg(long)]
        dry_run: bool,
    },
    /// Start the GitHub App OAuth + Stripe webhook server.
    ServeApp {
        #[arg(short, long, default_value = "3002")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    /// Run tests, find failing spec items, and queue fix tasks.
    GapFill {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        sail_url: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Extract the spec surface from test files.
    Spec {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        export: bool,
        #[arg(long)]
        save: bool,
    },
    /// Run KCQF quality analysis: file-size gate + spec surface drift check.
    Check {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        fail_on_violations: bool,
    },
    /// Export a diagnostic snapshot (tasks, logs, audit, stability, quota)
    /// from the local SQLite store into a committable JSON directory —
    /// so Claude chat or other agents without local filesystem access
    /// can see it.
    Diag {
        /// Output directory; a timestamped subdirectory is created inside it.
        #[arg(short, long, default_value = "artifacts/diagnostics")]
        out: PathBuf,
        /// Max task rows to include.
        #[arg(long, default_value = "200")]
        task_limit: i64,
        /// Max task-log lines to include.
        #[arg(long, default_value = "2000")]
        log_limit: i64,
        /// Max audit-log rows to include.
        #[arg(long, default_value = "500")]
        audit_limit: i64,
    },
    /// Start a dedicated GitHub webhook server.
    ServeWebhooks {
        #[arg(short, long, default_value = "3001")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, env = "LOPI_WEBHOOK_SECRET")]
        webhook_secret: Option<String>,
        #[arg(long, env = "GITHUB_TOKEN")]
        github_token: Option<String>,
        #[arg(long, env = "ANTHROPIC_API_KEY")]
        anthropic_key: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum LearnCmd {
    List {
        #[arg(short, long, default_value = "20")]
        limit: i64,
        #[arg(long)]
        postmortem_only: bool,
    },
    Show {
        id: String,
    },
    Export {
        #[arg(short, long, default_value = "100")]
        limit: i64,
    },
    Annotate {
        id: String,
        #[arg(value_parser = ["approved", "rejected"])]
        annotation: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum ScheduleCmd {
    List,
}

#[derive(Subcommand)]
pub(crate) enum LoopCmd {
    /// Validate `<repo>/.lopi/loop.toml` against the repo on disk.
    Validate {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
    },
    /// Print the effective loop config for a repo (defaults shown when absent).
    Show {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
    },
}

#[derive(Subcommand)]
pub(crate) enum StabilityCmd {
    List {
        #[arg(short, long, default_value = "20")]
        limit: i64,
        #[arg(long)]
        unstable_only: bool,
    },
    Summary,
}
