//! lopi — high-performance Rust orchestrator for concurrent Claude Code agents.
#![allow(clippy::print_stdout, clippy::print_stderr)]
mod gap_fill_commands;
mod learn_commands;
mod loop_commands;
mod remote;
mod repl;
mod replay_commands;
mod repo_detect;
mod run_command;
mod sail_commands;
mod schedule_commands;
mod spec_commands;
mod stability_commands;
mod task_commands;
mod trust_commands;
mod util;
mod webhook_commands;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::prelude::*;
use util::load_config;

#[derive(Parser)]
#[command(
    name = "lopi",
    version,
    about = "⛵ Konjo agent orchestrator — beautiful, excellent, provably correct."
)]
struct Cli {
    /// Path to config file (default: ./lopi.toml, then ~/.lopi/lopi.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
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
enum LearnCmd {
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
enum ScheduleCmd {
    List,
}

#[derive(Subcommand)]
enum LoopCmd {
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
enum StabilityCmd {
    List {
        #[arg(short, long, default_value = "20")]
        limit: i64,
        #[arg(long)]
        unstable_only: bool,
    },
    Summary,
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let fmt_layer = tracing_subscriber::fmt::layer();

    #[cfg(feature = "otel")]
    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        let service_name =
            std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "lopi".to_string());
        let otlp_exporter = opentelemetry_otlp::new_exporter().tonic();
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(otlp_exporter)
            .with_trace_config(opentelemetry_sdk::trace::config().with_resource(
                opentelemetry_sdk::Resource::new(vec![opentelemetry::KeyValue::new(
                    "service.name",
                    service_name,
                )]),
            ))
            .install_batch(opentelemetry_sdk::runtime::Tokio)
            .map_err(|e| anyhow::anyhow!("failed to install OTel tracer: {e}"))?;
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(otel_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }

    #[cfg(not(feature = "otel"))]
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();

    let cli = Cli::parse();
    let cfg = load_config(cli.config.as_ref());

    match cli.command {
        // ── bare `lopi` → interactive REPL ─────────────────────
        None => {
            let repo = repo_detect::detect_repo();
            let model = lopi_agent::MODEL_SONNET.to_string();
            repl::run_repl(repo, model, cfg).await?;
        }

        // ── lopi bypass <goal> ──────────────────────────────────
        Some(Commands::Bypass { goal_args }) => {
            let goal = goal_args.join(" ");
            let repo = repo_detect::detect_repo();
            repl::run_inline(goal, repo, true, cfg.as_ref()).await?;
        }

        // ── lopi run ────────────────────────────────────────────
        Some(Commands::Run {
            goal,
            repo,
            dry_run,
            speculative,
            adaptive_retry,
            stability_gate,
        }) => {
            run_command::run(
                goal,
                repo,
                dry_run,
                speculative,
                adaptive_retry,
                stability_gate,
                cfg.as_ref(),
            )
            .await?;
        }

        // ── lopi watch / tail / dock / sail / cancel / resume ───
        Some(Commands::Watch { remote, local }) => task_commands::watch(remote, local).await?,
        Some(Commands::Tail { task_id, history }) => task_commands::tail(task_id, history).await?,
        Some(Commands::Dock) => task_commands::dock().await?,
        Some(Commands::Sail {
            port,
            host,
            max_agents,
            repo,
            repos,
        }) => {
            sail_commands::run(max_agents, repo, repos, host, port, cfg.as_ref()).await?;
        }
        Some(Commands::Cancel { task_id }) => task_commands::cancel(task_id).await?,
        Some(Commands::Resume { agent_id }) => task_commands::resume(agent_id).await?,

        // ── lopi watch-gap-fill ─────────────────────────────────
        Some(Commands::WatchGapFill {
            repo,
            interval,
            sail_url,
            run_now,
        }) => {
            gap_fill_commands::watch_loop(repo, interval, &sail_url, run_now).await?;
        }

        Some(Commands::Trust) => trust_commands::show().await?,
        Some(Commands::Replay {
            task,
            from,
            dry_run,
        }) => replay_commands::run(task, from, dry_run).await?,

        Some(Commands::ServeApp { port, host }) => {
            let addr: std::net::SocketAddr = format!("{host}:{port}")
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid address: {e}"))?;
            let app_cfg = lopi_app::AppConfig::from_env();
            println!("🔐 lopi serve-app on {addr}");
            println!(
                "   GitHub OAuth: {}",
                if app_cfg.github_configured() {
                    "✅ configured"
                } else {
                    "⚠️  missing (GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET, GITHUB_REDIRECT_URI)"
                }
            );
            println!(
                "   Stripe:       {}",
                if app_cfg.stripe_configured() {
                    "✅ configured"
                } else {
                    "⚠️  missing (STRIPE_WEBHOOK_SECRET)"
                }
            );
            println!();
            let store = lopi_memory::MemoryStore::open(util::db_path()).await?;
            let state = lopi_app::AppState {
                cfg: app_cfg,
                store,
            };
            lopi_app::serve(state, addr).await?;
        }

        Some(Commands::GapFill {
            repo,
            sail_url,
            dry_run,
        }) => {
            gap_fill_commands::run(repo, &sail_url, dry_run, false).await?;
        }

        Some(Commands::Spec { repo, export, save }) => {
            spec_commands::run_spec(repo, export, save).await?;
        }
        Some(Commands::Check {
            repo,
            fail_on_violations,
        }) => {
            spec_commands::run_check(repo, fail_on_violations).await?;
        }

        Some(Commands::Learn(cmd)) => learn_commands::run(cmd, util::db_path()).await?,

        Some(Commands::ServeWebhooks {
            port,
            host,
            webhook_secret,
            github_token,
            anthropic_key,
        }) => {
            webhook_commands::run(port, host, webhook_secret, github_token, anthropic_key).await?;
        }

        Some(Commands::Schedules(ScheduleCmd::List)) => {
            let schedules = cfg
                .as_ref()
                .map(|c| c.schedules.clone())
                .unwrap_or_default();
            schedule_commands::list(schedules).await?;
        }

        Some(Commands::Loop(LoopCmd::Validate { repo })) => {
            loop_commands::validate(&repo)?;
        }
        Some(Commands::Loop(LoopCmd::Show { repo })) => {
            loop_commands::show(&repo)?;
        }

        // ── lopi stability ──────────────────────────────────────
        Some(Commands::Stability(StabilityCmd::List {
            limit,
            unstable_only,
        })) => stability_commands::list(limit, unstable_only).await?,

        Some(Commands::Stability(StabilityCmd::Summary)) => stability_commands::summary().await?,
    }

    Ok(())
}
