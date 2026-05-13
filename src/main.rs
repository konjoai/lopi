//! lopi — high-performance Rust orchestrator for concurrent Claude Code agents.
#![allow(clippy::print_stdout, clippy::print_stderr)]
mod gap_fill_commands;
mod learn_commands;
mod remote;
mod run_command;
mod sail_commands;
mod schedule_commands;
mod spec_commands;
mod task_commands;
#[cfg(test)]
mod tests;
mod trust_commands;
mod util;
mod webhook_commands;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use lopi_memory::MemoryStore;
use std::path::PathBuf;
use tracing_subscriber::prelude::*;
pub(crate) use util::{db_path, fmt_status, is_self_modify_attempt, load_config, status_label};

#[derive(Parser)]
#[command(
    name = "lopi",
    version,
    about = "⛵ Konjo agent orchestrator — lopi run, lopi sail, lopi dock"
)]
struct Cli {
    /// Path to config file (default: ./lopi.toml, then ~/.lopi/lopi.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
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
        /// Run the Layer 5 stability gate before implementation: generate N plan samples, measure
        /// pairwise variance, and block if variance exceeds the unstable threshold. Requires
        /// ANTHROPIC_API_KEY. Records every assessment to the stability ledger (`lopi stability`).
        #[arg(long)]
        stability_gate: bool,
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
        /// Tasks submitted via /api/tasks with a `repo` field are routed to
        /// the matching pool slot.
        #[arg(long, value_delimiter = ',')]
        repos: Vec<PathBuf>,
    },
    /// Cancel a running task by ID prefix
    Cancel {
        #[arg()]
        task_id: String,
    },
    /// Browse the mined pattern library — what worked, what didn't, what
    /// post-mortems learned. The pattern miner runs after every completed
    /// task; post-mortems run after every fully-failed task when adaptive
    /// retry is enabled.
    #[command(subcommand)]
    Learn(LearnCmd),
    /// Manage scheduled tasks
    #[command(subcommand)]
    Schedules(ScheduleCmd),
    /// Browse the Layer 5 patch stability ledger — model-output variance
    /// scores recorded before each self-modification attempt.
    #[command(subcommand)]
    Stability(StabilityCmd),
    /// Continuously run gap-fill on a cadence — the Kitchen Loop daemon.
    ///
    /// Runs `gap-fill` every `--interval` minutes. On each iteration:
    /// persists quality results, logs trend, and queues fix tasks for gaps.
    WatchGapFill {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        /// Interval in minutes between gap-fill runs (default 60).
        #[arg(long, default_value = "60")]
        interval: u64,
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        sail_url: String,
        /// Run once immediately on start (in addition to the loop).
        #[arg(long)]
        run_now: bool,
    },
    /// Show trust calibration stats — approved vs rejected pattern signals,
    /// current score weight adjustments, and reliability metrics.
    Trust,
    /// Start the GitHub App OAuth + Stripe webhook server.
    ///
    /// Reads credentials from environment variables:
    ///   GITHUB_APP_ID, GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET,
    ///   GITHUB_WEBHOOK_SECRET, GITHUB_REDIRECT_URI, STRIPE_WEBHOOK_SECRET
    ServeApp {
        #[arg(short, long, default_value = "3002")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    /// Run tests, find failing spec items, and queue fix tasks into a running
    /// lopi sail server. Use --dry-run to see gaps without queuing.
    GapFill {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        /// Base URL of the running lopi sail server.
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        sail_url: String,
        /// Report gaps without queuing fix tasks.
        #[arg(long)]
        dry_run: bool,
    },
    /// Extract the spec surface — what this repo claims to do — from test files.
    Spec {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        /// Print raw JSON instead of the table view.
        #[arg(long)]
        export: bool,
        /// Save the spec surface to .lopi/spec_surface.json for future `lopi check`.
        #[arg(long)]
        save: bool,
    },
    /// Run KCQF quality analysis: file-size gate + spec surface drift check.
    Check {
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
        /// Exit with code 1 if violations are found (for CI pipelines).
        #[arg(long)]
        fail_on_violations: bool,
    },
    /// Start a dedicated GitHub webhook server.
    ///
    /// Receives GitHub events, triages issues via Haiku, posts comments,
    /// and auto-queues fix tasks into the lopi agent pool.
    ServeWebhooks {
        #[arg(short, long, default_value = "3001")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// GitHub webhook secret for HMAC verification (optional but recommended).
        #[arg(long, env = "LOPI_WEBHOOK_SECRET")]
        webhook_secret: Option<String>,
        /// GitHub personal access token for posting comments and labels.
        /// When omitted, issue triage comments are skipped.
        #[arg(long, env = "GITHUB_TOKEN")]
        github_token: Option<String>,
        /// Anthropic API key for issue classification via Haiku.
        /// When omitted, issue triage is skipped.
        #[arg(long, env = "ANTHROPIC_API_KEY")]
        anthropic_key: Option<String>,
    },
}

#[derive(Subcommand)]
enum LearnCmd {
    /// List patterns sorted by success rate. Mined > post-mortem-derived.
    List {
        #[arg(short, long, default_value = "20")]
        limit: i64,
        /// Show only post-mortem-derived patterns
        #[arg(long)]
        postmortem_only: bool,
    },
    /// Show full detail for a single pattern by id prefix.
    Show {
        /// Id or id prefix (uuid)
        id: String,
    },
    /// Export all patterns to JSON for analytics. Pipes to stdout.
    Export {
        #[arg(short, long, default_value = "100")]
        limit: i64,
    },
    /// Annotate a pattern as approved or rejected to tune future scoring.
    Annotate {
        /// Pattern id or id prefix (uuid)
        id: String,
        /// Annotation: 'approved' or 'rejected'
        #[arg(value_parser = ["approved", "rejected"])]
        annotation: String,
    },
}

#[derive(Subcommand)]
enum ScheduleCmd {
    /// List all configured schedules with next run times
    List,
}

#[derive(Subcommand)]
enum StabilityCmd {
    /// List the most recent stability assessments.
    List {
        #[arg(short, long, default_value = "20")]
        limit: i64,
        /// Show only unstable assessments (variance above warning threshold).
        #[arg(long)]
        unstable_only: bool,
    },
    /// Show a summary of all-time verdict counts.
    Summary,
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<()> {
    /* mutants::skip — binary entry point: all branches require live services */
    // Initialize tracing with optional OpenTelemetry OTLP export.
    // Set OTEL_EXPORTER_OTLP_ENDPOINT (e.g. http://localhost:4317) to enable export.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));

    let fmt_layer = tracing_subscriber::fmt::layer();

    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        let otlp_exporter = opentelemetry_otlp::new_exporter().tonic();
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(otlp_exporter)
            .with_trace_config(opentelemetry_sdk::trace::config().with_resource(
                opentelemetry_sdk::Resource::new(vec![opentelemetry::KeyValue::new(
                    "service.name",
                    "lopi",
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

    let cli = Cli::parse();
    let cfg = load_config(cli.config.as_ref());

    match cli.command {
        // ── lopi run ────────────────────────────────────────────
        Commands::Run {
            goal,
            repo,
            dry_run,
            speculative,
            adaptive_retry,
            stability_gate,
        } => {
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

        // ── lopi watch ──────────────────────────────────────────
        Commands::Watch { remote, local } => task_commands::watch(remote, local).await?,
        Commands::Tail { task_id, history } => task_commands::tail(task_id, history).await?,
        Commands::Dock => task_commands::dock().await?,
        Commands::Sail {
            port,
            host,
            max_agents,
            repo,
            repos,
        } => {
            sail_commands::run(max_agents, repo, repos, host, port, cfg.as_ref()).await?;
        }
        Commands::Cancel { task_id } => task_commands::cancel(task_id).await?,

        // ── lopi learn ──────────────────────────────────────────
        Commands::WatchGapFill {
            repo,
            interval,
            sail_url,
            run_now,
        } => {
            gap_fill_commands::watch_loop(repo, interval, &sail_url, run_now).await?;
        }

        Commands::Trust => trust_commands::show().await?,

        Commands::ServeApp { port, host } => {
            let addr: std::net::SocketAddr = format!("{host}:{port}")
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid address: {e}"))?;
            let cfg = lopi_app::AppConfig::from_env();
            println!("🔐 lopi serve-app on {addr}");
            println!(
                "   GitHub OAuth: {}",
                if cfg.github_configured() {
                    "✅ configured"
                } else {
                    "⚠️  missing (set GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET, GITHUB_REDIRECT_URI)"
                }
            );
            println!(
                "   Stripe:       {}",
                if cfg.stripe_configured() {
                    "✅ configured"
                } else {
                    "⚠️  missing (set STRIPE_WEBHOOK_SECRET)"
                }
            );
            println!();
            let store = MemoryStore::open(db_path()).await?;
            let state = lopi_app::AppState { cfg, store };
            lopi_app::serve(state, addr).await?;
        }

        Commands::GapFill {
            repo,
            sail_url,
            dry_run,
        } => {
            gap_fill_commands::run(repo, &sail_url, dry_run, false).await?;
        }

        Commands::Spec { repo, export, save } => {
            spec_commands::run_spec(repo, export, save).await?;
        }
        Commands::Check {
            repo,
            fail_on_violations,
        } => {
            spec_commands::run_check(repo, fail_on_violations).await?;
        }

        Commands::Learn(cmd) => learn_commands::run(cmd, db_path()).await?,

        Commands::ServeWebhooks {
            port,
            host,
            webhook_secret,
            github_token,
            anthropic_key,
        } => {
            webhook_commands::run(port, host, webhook_secret, github_token, anthropic_key).await?;
        }

        Commands::Schedules(ScheduleCmd::List) => {
            let schedules = cfg
                .as_ref()
                .map(|c| c.schedules.clone())
                .unwrap_or_default();
            schedule_commands::list(schedules).await?;
        }

        // ── lopi stability ──────────────────────────────────────
        Commands::Stability(cmd) => match cmd {
            StabilityCmd::List {
                limit,
                unstable_only,
            } => {
                let store = MemoryStore::open(db_path()).await?;
                let entries = store.load_stability_entries(limit).await?;
                let filtered: Vec<_> = if unstable_only {
                    entries
                        .into_iter()
                        .filter(|e| e.verdict == "unstable")
                        .collect()
                } else {
                    entries
                };

                println!("🔬 lopi stability — {} assessment(s)\n", filtered.len());
                if filtered.is_empty() {
                    if unstable_only {
                        println!("  No unstable assessments in the ledger.");
                    } else {
                        println!("  No stability assessments yet.");
                        println!("  Enable with `AgentRunner::with_stability_gate()` or `lopi run --stability-gate`.");
                    }
                    return Ok(());
                }

                println!(
                    "  {:<8}  {:<36}  {:<9}  {:>8}  {:>8}  Verdict",
                    "Id", "Goal prefix", "Model", "Variance", "Samples"
                );
                println!("  {}", "─".repeat(90));
                for e in &filtered {
                    let id = &e.id[..8.min(e.id.len())];
                    let goal = if e.task_goal_pfx.len() > 36 {
                        format!("{}…", &e.task_goal_pfx[..35])
                    } else {
                        e.task_goal_pfx.clone()
                    };
                    let model_short = e.model.split('-').next_back().unwrap_or(&e.model);
                    let verdict_icon = match e.verdict.as_str() {
                        "stable" => "✅ stable",
                        "warning" => "⚠️  warning",
                        "unstable" => "🚫 UNSTABLE",
                        other => other,
                    };
                    println!(
                        "  {id:<8}  {goal:<36}  {model_short:<9}  {:>8.3}  {:>8}  {verdict_icon}",
                        e.variance_score, e.n_samples
                    );
                }
            }

            StabilityCmd::Summary => {
                let store = MemoryStore::open(db_path()).await?;
                let (stable, warning, unstable) = store.stability_verdict_counts().await?;
                let total = stable + warning + unstable;
                println!("🔬 lopi stability summary\n");
                println!("  Total assessments:  {total}");
                println!("  ✅ Stable:          {stable}");
                println!("  ⚠️  Warning:         {warning}");
                println!("  🚫 Unstable:        {unstable}");
                if total > 0 {
                    let block_rate = unstable as f64 / total as f64 * 100.0;
                    println!("  Block rate:         {block_rate:.1}%");
                }
            }
        },
    }

    Ok(())
}
