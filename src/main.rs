#![allow(clippy::print_stdout, clippy::print_stderr)]
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use lopi_agent::AgentRunner;
use lopi_core::{AgentEvent, EventBus, LopiConfig, RepoProfile, Task, TaskStatus};
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
    /// Show mined patterns and their success rates
    Learn {
        #[arg(short, long, default_value = "20")]
        limit: i64,
    },
    /// Manage scheduled tasks
    #[command(subcommand)]
    Schedules(ScheduleCmd),
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
            let task_id = task.id;
            let id_short = &task_id.0.to_string()[..8];
            store.save_task(&task, "queued").await.ok();

            println!("   task id: {id_short}…");
            println!("   use `lopi watch` in another terminal for the TUI");
            println!();

            let (mut runner, bus) = AgentRunner::standalone(task.clone(), repo);
            runner.store = Some(store.clone());
            runner.dry_run = dry_run;
            runner.speculative = speculative;

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
                watch_remote(ws_url).await?;
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

            let auth_token = cfg.as_ref().and_then(|c| c.web.auth_token.clone());
            lopi_ui::web::serve(store, bus, queue, pool, &host, port, auth_token).await?;
        }

        // ── lopi cancel ─────────────────────────────────────────
        Commands::Cancel { task_id } => {
            let url = format!("http://127.0.0.1:3000/api/tasks/{task_id}");
            if let Ok(msg) = reqwest_cancel(&url).await {
                println!("{msg}");
            } else {
                println!("⚠️  No running lopi sail server on :3000.");
                println!("   Start `lopi sail` first or use the web dashboard.");
            }
        }

        // ── lopi learn ──────────────────────────────────────────
        Commands::Learn { limit } => {
            let store = MemoryStore::open(db_path()).await?;
            let patterns = store.load_patterns(limit).await?;
            println!("🧠 lopi learn — {} pattern(s)\n", patterns.len());
            if patterns.is_empty() {
                println!("  No patterns yet. Patterns are mined after each completed task.");
                return Ok(());
            }
            println!(
                "  {:<40}  {:>10}  {:>10}  Last seen",
                "Keywords", "Avg Att.", "Success%"
            );
            println!("  {}", "─".repeat(80));
            for p in patterns {
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
                let ts = &p.last_seen[..10.min(p.last_seen.len())];
                println!("  {kw:<40}  {avg:>10}  {sr:>10}  {ts}");
            }
        }

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
                let next = next_run_times(&s.cron, 1)
                    .into_iter()
                    .next()
                    .map_or_else(|| "invalid cron".to_string(), |t| t.format("%Y-%m-%d %H:%M UTC").to_string());
                println!("  {:<20}  {:<w$}  {:<14}  {}", s.name, goal, s.cron, next);
            }
        }
    }

    Ok(())
}

/// Connect to a running lopi sail WebSocket and drive the TUI from network events.
async fn watch_remote(ws_url: String) -> Result<()> {
    use futures::StreamExt;
    use tokio_tungstenite::tungstenite::Message as WsMsg;

    let bus: EventBus<AgentEvent> = EventBus::new(512);
    let bus_tx = bus.clone();

    // Try to connect; if it fails immediately, fall back to local mode.
    let (mut ws, _) = match tokio_tungstenite::connect_async(&ws_url).await {
        Ok(pair) => pair,
        Err(e) => {
            println!("⚠️  Could not connect to {ws_url}: {e}");
            println!("   Falling back to local bus. Run `lopi sail` to get live events.");
            let local_bus: EventBus<AgentEvent> = EventBus::new(512);
            return lopi_ui::tui::run(local_bus).await;
        }
    };

    println!("   connected — starting TUI (q to quit)");

    // Pump WebSocket messages into the local bus on a background task.
    let pump = tokio::spawn(async move {
        while let Some(msg) = ws.next().await {
            match msg {
                Ok(WsMsg::Text(text)) => {
                    if let Ok(ev) = serde_json::from_str::<AgentEvent>(&text) {
                        bus_tx.send(ev);
                    } else if let Ok(snap) = serde_json::from_str::<serde_json::Value>(&text) {
                        // Handle snapshot message: synthesise TaskQueued events for each task.
                        if snap.get("type").and_then(|v| v.as_str()) == Some("snapshot") {
                            if let Some(tasks) = snap.get("tasks").and_then(|v| v.as_array()) {
                                for t in tasks {
                                    let id_str = t.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                    let goal = t
                                        .get("goal")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    if let Ok(uuid) = id_str.parse::<uuid::Uuid>() {
                                        bus_tx.send(AgentEvent::TaskQueued {
                                            task_id: lopi_core::TaskId(uuid),
                                            goal,
                                            priority: lopi_core::Priority::Normal,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(WsMsg::Close(_)) | Err(_) => break,
                _ => {}
            }
        }
    });

    lopi_ui::tui::run(bus).await?;
    pump.abort();
    Ok(())
}

async fn reqwest_cancel(url: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .delete(url)
        .send()
        .await
        .context("HTTP DELETE failed")?;
    let body = resp.json::<serde_json::Value>().await?;
    if body
        .get("cancelled")
        .and_then(|v: &serde_json::Value| v.as_bool())
        .unwrap_or(false)
    {
        Ok("⛔ Task cancelled.".into())
    } else {
        Ok(format!(
            "ℹ️  {}",
            body.get("reason")
                .and_then(|v: &serde_json::Value| v.as_str())
                .unwrap_or("unknown")
        ))
    }
}
