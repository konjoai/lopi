use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use lopi_agent::AgentRunner;
use lopi_core::{AgentEvent, EventBus, Task, TaskStatus};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use std::path::PathBuf;
use std::sync::Arc;

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
    },
    /// Watch live agent status in a full ratatui TUI
    Watch,
    /// Tail agent events (history or live)
    Tail {
        /// Filter by task ID prefix
        #[arg(short, long)]
        task_id: Option<String>,
        /// Show history from DB instead of waiting for live events
        #[arg(long)]
        history: bool,
    },
    /// List all tasks and their status from the database
    Dock,
    /// Start the web dashboard + agent pool (lopi sail --port 3000)
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
}

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".lopi").join("lopi.db")
}

fn fmt_status(s: &str) -> &str {
    match s {
        "queued"      => "⏳ queued",
        "planning"    => "📋 planning",
        "implementing"=> "🔨 implementing",
        "testing"     => "🧪 testing",
        "scoring"     => "📊 scoring",
        "success"     => "✅ success",
        "failed"      => "❌ failed",
        "rolled_back" => "⏪ rolled back",
        _             => s,
    }
}

fn status_label(s: &TaskStatus) -> String {
    match s {
        TaskStatus::Queued        => "queued".into(),
        TaskStatus::Planning      => "planning".into(),
        TaskStatus::Implementing  => "implementing".into(),
        TaskStatus::Testing       => "testing".into(),
        TaskStatus::Scoring       => "scoring".into(),
        TaskStatus::Retrying { attempt } => format!("retrying (attempt {attempt})"),
        TaskStatus::Success { branch, pr_url } => format!(
            "success ✅ branch={branch}{}",
            pr_url.as_deref().map(|u| format!(", pr={u}")).unwrap_or_default()
        ),
        TaskStatus::Failed { reason } => format!("failed ❌ {reason}"),
        TaskStatus::RolledBack    => "rolled back".into(),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        // ── lopi run ────────────────────────────────────────────
        Commands::Run { goal, repo } => {
            println!("🚢 lopi run");
            println!("   goal: {goal}");
            println!("   repo: {}", repo.display());
            println!();

            let store = MemoryStore::open(db_path()).await?;
            let task = Task::new(goal);
            let task_id = task.id;
            let id_short = &task_id.0.to_string()[..8];
            store.save_task(&task, "queued").await.ok();

            println!("   task id: {id_short}…");
            println!("   use `lopi watch` in another terminal for the TUI");
            println!();

            let (mut runner, bus) = AgentRunner::standalone(task.clone(), repo);
            runner.store = Some(store.clone());

            let mut rx = bus.subscribe();
            // Stream events to stdout while agent runs.
            let print_task = tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(AgentEvent::StatusChanged { status, attempt, .. }) => {
                            println!("  [{attempt}] → {}", status_label(&status));
                        }
                        Ok(AgentEvent::LogLine { line, .. }) => {
                            println!("       {line}");
                        }
                        Ok(AgentEvent::ScoreUpdated { test_pass_rate, lint_errors, .. }) => {
                            println!("       score: {:.0}% pass, {} lint errors",
                                test_pass_rate * 100.0, lint_errors);
                        }
                        Ok(AgentEvent::TaskCompleted { .. }) => break,
                        Err(_) => break,
                        _ => {}
                    }
                }
            });

            let outcome = runner.run().await?;
            print_task.abort();
            store.mark_completed(&task_id, &status_label(&outcome)).await.ok();
            store.mine_patterns(&task_id, &task.goal).await.ok();

            println!();
            println!("⚓ {}", status_label(&outcome));
        }

        // ── lopi watch ──────────────────────────────────────────
        Commands::Watch => {
            // Standalone watch — creates a local bus (events only visible to this session).
            // In a real deployment, watch would connect to a running `lopi sail` via WebSocket.
            let bus: EventBus<AgentEvent> = EventBus::new(512);
            println!("👁  lopi watch — TUI starting (q to quit)");
            println!("   Tip: run `lopi sail` first to see live agent events here.");
            lopi_ui::tui::run(bus).await?;
        }

        // ── lopi tail ───────────────────────────────────────────
        Commands::Tail { task_id, history } => {
            let store = MemoryStore::open(db_path()).await?;
            if history || task_id.is_some() {
                let rows = store.load_history(50).await?;
                println!("⚓ lopi tail — {} task(s) in history", rows.len());
                for t in rows.iter().filter(|t| {
                    task_id.as_deref().is_none_or(|id| t.id.starts_with(id))
                }) {
                    println!("  [{}] {}… — {}",
                        fmt_status(&t.status),
                        &t.id[..8.min(t.id.len())],
                        t.goal
                    );
                }
            } else {
                println!("📋 lopi tail — waiting for live events (Ctrl-C to stop)");
                println!("   Use --history to view past tasks, or run `lopi sail` for a live server.");
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
            }
            let w = 50usize;
            println!("  {:<8}  {:<w$}  {}", "ID", "Goal", "Status");
            println!("  {}", "─".repeat(8 + 2 + w + 2 + 20));
            for t in history {
                let goal = if t.goal.len() > w { format!("{}…", &t.goal[..w-1]) } else { t.goal.clone() };
                println!("  {:<8}  {:<w$}  {}",
                    &t.id[..8.min(t.id.len())],
                    goal,
                    fmt_status(&t.status)
                );
            }
        }

        // ── lopi sail ───────────────────────────────────────────
        Commands::Sail { port, host, max_agents, repo } => {
            let store = MemoryStore::open(db_path()).await?;
            let bus: EventBus<AgentEvent> = EventBus::new(512);
            let queue = TaskQueue::new();
            let pool = Arc::new(
                AgentPool::new(max_agents, repo.clone(), queue.clone(), bus.clone())
                    .with_store(store.clone())
            );

            println!("🚢 lopi sail");
            println!("   agents:    up to {max_agents} concurrent");
            println!("   repo:      {}", repo.display());
            println!("   dashboard: http://{host}:{port}");
            println!("   api:       http://{host}:{port}/api/tasks");
            println!("   ws:        ws://{host}:{port}/ws");
            println!();
            println!("   Submit a task:  POST http://{host}:{port}/api/tasks");
            println!("   Or use:         lopi run --goal \"...\" --repo {}", repo.display());
            println!();

            // Spawn pool dispatch loop in background.
            // AgentPool::clone() is cheap — all fields are Arc-wrapped.
            let pool_for_dispatch = (*pool).clone();
            tokio::spawn(async move {
                if let Err(e) = pool_for_dispatch.run().await {
                    tracing::error!("pool error: {e}");
                }
            });

            // Serve web in foreground.
            lopi_ui::web::serve(store, bus, queue, pool, &host, port).await?;
        }

        // ── lopi cancel ─────────────────────────────────────────
        Commands::Cancel { task_id } => {
            // Cancel via HTTP if a sail server is running on default port; otherwise report.
            let url = format!("http://127.0.0.1:3000/api/tasks/{task_id}");
            match reqwest_cancel(&url).await {
                Ok(msg) => println!("{msg}"),
                Err(_) => {
                    println!("⚠️  No running lopi sail server found on :3000.");
                    println!("   Start lopi sail first, or use the web dashboard to cancel.");
                }
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
            println!("  {:<40}  {:>10}  {:>10}  {}", "Keywords", "Avg Att.", "Success%", "Last seen");
            println!("  {}", "─".repeat(80));
            for p in patterns {
                let kw = if p.goal_keywords.len() > 40 {
                    format!("{}…", &p.goal_keywords[..39])
                } else {
                    p.goal_keywords.clone()
                };
                let avg = p.avg_attempts.map(|a| format!("{a:.1}")).unwrap_or_else(|| "-".into());
                let sr = p.success_rate.map(|s| format!("{:.0}%", s * 100.0)).unwrap_or_else(|| "-".into());
                let ts = &p.last_seen[..10.min(p.last_seen.len())];
                println!("  {:<40}  {:>10}  {:>10}  {}", kw, avg, sr, ts);
            }
        }
    }

    Ok(())
}

async fn reqwest_cancel(url: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client.delete(url).send().await.context("HTTP DELETE failed")?;
    let body: serde_json::Value = resp.json::<serde_json::Value>().await?;
    if body.get("cancelled").and_then(|v: &serde_json::Value| v.as_bool()).unwrap_or(false) {
        Ok(format!("⛔ Task cancelled."))
    } else {
        Ok(format!("ℹ️  {}", body.get("reason").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("unknown")))
    }
}
