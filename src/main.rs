//! lopi — high-performance Rust orchestrator for concurrent Claude Code agents.
#![allow(clippy::print_stdout, clippy::print_stderr)]
mod remote;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use lopi_agent::AgentRunner;
use lopi_core::{AgentEvent, EventBus, LopiConfig, RepoProfile, Task, TaskSource, TaskStatus};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{boot_scheduler, next_run_times, AgentPool, TaskQueue};
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::prelude::*;

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
    /// Start the web dashboard + agent pool
    Sail {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "4")]
        max_agents: usize,
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
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
}

#[derive(Subcommand)]
enum ScheduleCmd {
    /// List all configured schedules with next run times
    List,
}

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".lopi").join("lopi.db")
}

fn fmt_status(s: &str) -> &str {
    match s {
        "queued" => "⏳ queued",
        "planning" => "📋 planning",
        "implementing" => "🔨 implementing",
        "testing" => "🧪 testing",
        "scoring" => "📊 scoring",
        "success" => "✅ success",
        "failed" => "❌ failed",
        "rolled_back" => "⏪ rolled back",
        _ => s,
    }
}

fn status_label(s: &TaskStatus) -> String {
    match s {
        TaskStatus::Queued => "queued".into(),
        TaskStatus::Planning => "planning".into(),
        TaskStatus::Implementing => "implementing".into(),
        TaskStatus::Testing => "testing".into(),
        TaskStatus::Scoring => "scoring".into(),
        TaskStatus::Retrying { attempt } => format!("retrying (attempt {attempt})"),
        TaskStatus::Success { branch, pr_url } => format!(
            "success ✅ branch={branch}{}",
            pr_url
                .as_deref()
                .map(|u| format!(", pr={u}"))
                .unwrap_or_default()
        ),
        TaskStatus::Failed { reason } => format!("failed ❌ {reason}"),
        TaskStatus::RolledBack => "rolled back".into(),
    }
}

fn load_config(path: Option<&PathBuf>) -> Option<LopiConfig> {
    if let Some(p) = path {
        LopiConfig::load(p).ok()
    } else {
        LopiConfig::find_and_load()
    }
}

fn is_self_modify_attempt(repo: &std::path::Path) -> bool {
    if let Ok(exe) = std::env::current_exe() {
        if let (Some(parent), Ok(repo_canonical)) = (exe.parent().and_then(|p| p.parent()), repo.canonicalize()) {
            if let Ok(exe_canonical) = parent.canonicalize() {
                return repo_canonical.starts_with(&exe_canonical);
            }
        }
    }
    false
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<()> {
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
        } => {
            println!("🚢 lopi run{}", if dry_run { " (dry-run)" } else { "" });
            println!("   goal: {goal}");
            println!("   repo: {}", repo.display());

            // Apply per-repo profile.
            let profile = RepoProfile::load_from_repo(&repo);
            let has_profile = !profile.allowed_dirs.is_empty()
                || profile.max_retries.is_some()
                || !profile.default_constraints.is_empty();
            if has_profile {
                println!("   profile: .lopi.toml found — applying overrides");
            }
            println!();

            let store = MemoryStore::open(db_path()).await?;
            let mut task = Task::new(goal);
            profile.apply(&mut task);

            // Check for self-modification attempt.
            if is_self_modify_attempt(&repo) {
                let allow_self_modify = cfg.as_ref().is_some_and(|c| c.lopi.allow_self_modify);
                if !allow_self_modify {
                    eprintln!("❌ self-modification blocked: lopi cannot modify itself");
                    eprintln!("   to enable, set `allow_self_modify = true` in [lopi] section of lopi.toml");
                    return Err(anyhow::anyhow!("self-modification not allowed"));
                }
                task.source = TaskSource::SelfModify { approved_by: "config".into() };
                task.allowed_dirs = vec!["crates/".into(), "src/".into()];
                task.forbidden_dirs = vec![".github/".into(), "Cargo.lock".into()];
            }

            let task_id = task.id;
            let id_short = &task_id.0.to_string()[..8];
            store.save_task(&task, "queued").await.ok();

            println!("   task id: {id_short}…");
            println!("   use `lopi watch` in another terminal for the TUI");
            println!();

            let mut runner = AgentRunner::standalone(task.clone(), repo).0;
            if adaptive_retry {
                runner = runner.with_adaptive_retry();
            }
            runner.store = Some(store.clone());
            runner.dry_run = dry_run;
            runner.speculative = speculative;
            let bus = runner.bus.clone();

            let mut rx = bus.subscribe();
            let print_task = tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(AgentEvent::StatusChanged {
                            status, attempt, ..
                        }) => {
                            println!("  [{attempt}] → {}", status_label(&status));
                        }
                        Ok(AgentEvent::LogLine { line, .. }) => {
                            println!("       {line}");
                        }
                        Ok(AgentEvent::ScoreUpdated {
                            test_pass_rate,
                            lint_errors,
                            ..
                        }) => {
                            println!(
                                "       score: {:.0}% pass, {} lint errors",
                                test_pass_rate * 100.0,
                                lint_errors
                            );
                        }
                        Ok(AgentEvent::TaskCompleted { .. }) | Err(_) => break,
                        _ => {}
                    }
                }
            });

            let outcome = runner.run().await?;
            print_task.abort();
            store
                .mark_completed(&task_id, &status_label(&outcome))
                .await
                .ok();
            store.mine_patterns(&task_id, &task.goal).await.ok();

            println!();
            println!("⚓ {}", status_label(&outcome));
        }

        // ── lopi watch ──────────────────────────────────────────
        Commands::Watch { remote, local } => {
            if local {
                let bus: EventBus<AgentEvent> = EventBus::new(512);
                println!("👁  lopi watch (local bus — no running sail server)");
                lopi_ui::tui::run(bus).await?;
            } else {
                let ws_url = remote.unwrap_or_else(|| "ws://127.0.0.1:3000/ws".into());
                println!("👁  lopi watch — connecting to {ws_url}");
                remote::watch_remote(ws_url).await?;
            }
        }

        // ── lopi tail ───────────────────────────────────────────
        Commands::Tail { task_id, history } => {
            let store = MemoryStore::open(db_path()).await?;
            if history || task_id.is_some() {
                let rows = store.load_history(50).await?;
                println!("⚓ lopi tail — {} task(s) in history", rows.len());
                for t in rows
                    .iter()
                    .filter(|t| task_id.as_deref().is_none_or(|id| t.id.starts_with(id)))
                {
                    println!(
                        "  [{}] {}… — {}",
                        fmt_status(&t.status),
                        &t.id[..8.min(t.id.len())],
                        t.goal
                    );
                }
            } else {
                println!("📋 lopi tail — use --history or run `lopi sail` for a live server");
                tokio::signal::ctrl_c().await?;
            }
        }

        // ── lopi dock ───────────────────────────────────────────
        Commands::Dock => {
            let store = MemoryStore::open(db_path()).await?;
            let history = store.load_history(50).await?;
            println!("⚓ lopi dock — {} task(s)\n", history.len());
            if history.is_empty() {
                println!("  No tasks yet. Try: lopi run --goal \"write a test\"");
                return Ok(());
            }
            let w = 50usize;
            println!("  {:<8}  {:<w$}  Status", "ID", "Goal");
            println!("  {}", "─".repeat(8 + 2 + w + 2 + 20));
            for t in history {
                let goal = if t.goal.len() > w {
                    format!("{}…", &t.goal[..w - 1])
                } else {
                    t.goal.clone()
                };
                println!(
                    "  {:<8}  {:<w$}  {}",
                    &t.id[..8.min(t.id.len())],
                    goal,
                    fmt_status(&t.status)
                );
            }
        }

        // ── lopi sail ───────────────────────────────────────────
        Commands::Sail {
            port,
            host,
            max_agents,
            repo,
        } => {
            let store = MemoryStore::open(db_path()).await?;
            let bus: EventBus<AgentEvent> = EventBus::new(512);
            let queue = TaskQueue::new();
            let pool = Arc::new(
                AgentPool::new(max_agents, repo.clone(), queue.clone(), bus.clone())
                    .with_store(store.clone()),
            );

            println!("🚢 lopi sail");
            println!("   agents:    up to {max_agents} concurrent");
            println!("   repo:      {}", repo.display());
            println!("   dashboard: http://{host}:{port}");
            println!("   api:       http://{host}:{port}/api/tasks");
            println!("   ws:        ws://{host}:{port}/ws");

            // Boot schedules from config.
            let schedules = cfg
                .as_ref()
                .map(|c| c.schedules.clone())
                .unwrap_or_default();
            if !schedules.is_empty() {
                println!("   schedules: {} configured", schedules.len());
                let pool_sched = (*pool).clone();
                tokio::spawn(async move {
                    match boot_scheduler(schedules, pool_sched).await {
                        Ok(_sched) => {
                            // Keep the scheduler alive — dropping it stops all jobs.
                            // Block forever so the task (and scheduler) stays alive.
                            tokio::signal::ctrl_c().await.ok();
                        }
                        Err(e) => tracing::error!("scheduler boot failed: {e}"),
                    }
                });
            }
            println!();

            let pool_for_dispatch = (*pool).clone();
            tokio::spawn(async move {
                if let Err(e) = pool_for_dispatch.run().await {
                    tracing::error!("pool error: {e}");
                }
            });

            // Boot Telegram bot if token is configured.
            if let Ok(token) = std::env::var("TELOXIDE_TOKEN") {
                let allowed_chat_ids = cfg
                    .as_ref()
                    .map(|c| c.remote.telegram.allowed_chat_ids.clone())
                    .unwrap_or_default();
                let store_telegram = store.clone();
                let queue_telegram = queue.clone();
                tokio::spawn(async move {
                    if let Err(e) = lopi_remote::telegram::run(
                        token,
                        queue_telegram,
                        store_telegram,
                        allowed_chat_ids,
                    )
                    .await
                    {
                        tracing::error!("telegram bot error: {e}");
                    }
                });
            }

            let auth_token = cfg.as_ref().and_then(|c| c.web.auth_token.clone());
            lopi_ui::web::serve(store, bus, queue, pool, &host, port, auth_token).await?;
        }

        // ── lopi cancel ─────────────────────────────────────────
        Commands::Cancel { task_id } => {
            let url = format!("http://127.0.0.1:3000/api/tasks/{task_id}");
            if let Ok(msg) = remote::reqwest_cancel(&url).await {
                println!("{msg}");
            } else {
                println!("⚠️  No running lopi sail server on :3000.");
                println!("   Start `lopi sail` first or use the web dashboard.");
            }
        }

        // ── lopi learn ──────────────────────────────────────────
        Commands::Learn(cmd) => match cmd {
            LearnCmd::List {
                limit,
                postmortem_only,
            } => {
                let store = MemoryStore::open(db_path()).await?;
                let patterns = store.load_patterns(limit).await?;
                let filtered: Vec<_> = if postmortem_only {
                    patterns
                        .into_iter()
                        .filter(|p| p.derived_from_postmortem == 1)
                        .collect()
                } else {
                    patterns
                };

                println!("🧠 lopi learn — {} pattern(s)\n", filtered.len());
                if filtered.is_empty() {
                    if postmortem_only {
                        println!("  No post-mortem patterns yet. Enable with `lopi run --adaptive-retry` on a task that fails.");
                    } else {
                        println!("  No patterns yet. Patterns are mined after each completed task.");
                    }
                    return Ok(());
                }

                let headers = ("Id", "Keywords", "Avg Att.", "Success%", "Source");
                println!(
                    "  {:<8}  {:<40}  {:>9}  {:>9}  {}",
                    headers.0, headers.1, headers.2, headers.3, headers.4
                );
                println!("  {}", "─".repeat(90));
                for p in filtered {
                    let id_short = &p.id[..8.min(p.id.len())];
                    let kw = if p.goal_keywords.len() > 40 {
                        format!("{}…", &p.goal_keywords[..39])
                    } else {
                        p.goal_keywords.clone()
                    };
                    let avg = p
                        .avg_attempts
                        .map_or_else(|| "-".to_string(), |a| format!("{a:.1}"));
                    let sr = p
                        .success_rate
                        .map_or_else(|| "-".to_string(), |s| format!("{:.0}%", s * 100.0));
                    let source = if p.derived_from_postmortem == 1 {
                        "🧠 post-mortem"
                    } else {
                        "📊 mined"
                    };
                    println!("  {id_short:<8}  {kw:<40}  {avg:>9}  {sr:>9}  {source}");
                }
            }

            LearnCmd::Show { id } => {
                let store = MemoryStore::open(db_path()).await?;
                let Some(p) = store.find_pattern_by_id_prefix(&id).await? else {
                    eprintln!("❌ no pattern matches id prefix '{id}'");
                    std::process::exit(1);
                };

                println!("🧠 Pattern {}\n", p.id);
                println!("  Keywords:    {}", p.goal_keywords);
                println!(
                    "  Source:      {}",
                    if p.derived_from_postmortem == 1 {
                        "🧠 post-mortem-derived (Claude reflection over a failed run)"
                    } else {
                        "📊 mined from completed-task statistics"
                    }
                );
                println!(
                    "  Avg attempts: {}",
                    p.avg_attempts
                        .map_or_else(|| "-".to_string(), |a| format!("{a:.2}"))
                );
                println!(
                    "  Success:     {}",
                    p.success_rate
                        .map_or_else(|| "-".to_string(), |s| format!("{:.0}%", s * 100.0))
                );
                println!("  Last seen:   {}", p.last_seen);
                if let Some(c) = p.successful_constraints.as_deref() {
                    println!("\n  Constraint:");
                    println!("    {c}");
                } else {
                    println!("\n  Constraint:  (none captured yet)");
                }
            }

            LearnCmd::Export { limit } => {
                let store = MemoryStore::open(db_path()).await?;
                let patterns = store.load_patterns(limit).await?;
                let json = serde_json::json!({
                    "exported_at": chrono::Utc::now().to_rfc3339(),
                    "count": patterns.len(),
                    "patterns": patterns.iter().map(|p| serde_json::json!({
                        "id": p.id,
                        "goal_keywords": p.goal_keywords,
                        "successful_constraints": p.successful_constraints,
                        "avg_attempts": p.avg_attempts,
                        "success_rate": p.success_rate,
                        "last_seen": p.last_seen,
                        "derived_from_postmortem": p.derived_from_postmortem == 1,
                    })).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
        },

        // ── lopi schedules ──────────────────────────────────────
        Commands::Schedules(ScheduleCmd::List) => {
            let schedules = cfg
                .as_ref()
                .map(|c| c.schedules.clone())
                .unwrap_or_default();
            if schedules.is_empty() {
                println!("⏰ lopi schedules — none configured");
                println!();
                println!("  Add [[schedules]] entries to lopi.toml:");
                println!();
                println!("  [[schedules]]");
                println!("  name = \"nightly-lint\"");
                println!("  repo = \"/path/to/repo\"");
                println!("  goal = \"Fix all clippy warnings\"");
                println!("  cron = \"0 2 * * *\"");
                return Ok(());
            }

            println!("⏰ lopi schedules — {} configured\n", schedules.len());
            let w = 30usize;
            println!(
                "  {:<20}  {:<w$}  {:<14}  Next run (UTC)",
                "Name", "Goal", "Cron"
            );
            println!("  {}", "─".repeat(20 + 2 + w + 2 + 14 + 2 + 26));
            for s in &schedules {
                let goal = if s.goal.len() > w {
                    format!("{}…", &s.goal[..w - 1])
                } else {
                    s.goal.clone()
                };
                let next = next_run_times(&s.cron, 1).into_iter().next().map_or_else(
                    || "invalid cron".to_string(),
                    |t| t.format("%Y-%m-%d %H:%M UTC").to_string(),
                );
                println!("  {:<20}  {:<w$}  {:<14}  {}", s.name, goal, s.cron, next);
            }
        }
    }

    Ok(())
}
