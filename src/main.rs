use anyhow::Result;
use clap::{Parser, Subcommand};
use lopi_agent::AgentRunner;
use lopi_core::{EventBus, Task, TaskStatus};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "lopi",
    version,
    about = "Konjo agent orchestrator — lopi run, lopi sail, lopi dock"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Queue and run an agent task immediately
    Run {
        #[arg(short, long)]
        goal: String,
        #[arg(short, long, default_value = ".")]
        repo: PathBuf,
    },
    /// Watch live agent status (TUI)
    Watch,
    /// Tail agent event stream
    Tail {
        #[arg(short, long)]
        task_id: Option<String>,
        /// Show history from DB instead of live stream
        #[arg(long)]
        history: bool,
    },
    /// List all tasks and their status
    Dock,
    /// Start the web dashboard + API server with a live agent pool
    Sail {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "4")]
        max_agents: usize,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
}

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".lopi").join("lopi.db")
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Run { goal, repo } => {
            println!("🚢 lopi run — goal: {goal}, repo: {}", repo.display());
            let store = MemoryStore::open(db_path()).await?;
            let task = Task::new(goal);
            store.save_task(&task, "queued").await.ok();
            let (runner, bus) = AgentRunner::standalone(task.clone(), repo);
            let mut rx = bus.subscribe();
            // Stream status events to stdout while the agent runs.
            let stream = tokio::spawn(async move {
                while let Ok(ev) = rx.recv().await {
                    println!("  → {}", status_label(&ev));
                }
            });
            let outcome = runner.run().await?;
            stream.abort();
            store.mark_completed(&task.id, &status_label(&outcome)).await.ok();
            println!("⚓ {}", status_label(&outcome));
        }

        Commands::Watch => {
            let store = MemoryStore::open(db_path()).await?;
            lopi_ui::tui::run(store).await?;
        }

        Commands::Tail { task_id, history } => {
            let store = MemoryStore::open(db_path()).await?;
            if history {
                let rows = store.load_history(50).await?;
                for t in rows.iter().filter(|t| {
                    task_id.as_deref().is_none_or(|id| t.id.starts_with(id))
                }) {
                    println!("[{}] {} — {}", t.status, &t.id[..8.min(t.id.len())], t.goal);
                }
            } else {
                println!("📋 lopi tail: waiting for live events (Ctrl-C to stop)…");
                println!("   (use --history to view past tasks from the database)");
                // In a live `lopi sail` setup the bus would be shared; here we block.
                tokio::signal::ctrl_c().await?;
            }
        }

        Commands::Dock => {
            let store = MemoryStore::open(db_path()).await?;
            let history = store.load_history(50).await?;
            println!("⚓ lopi dock — {} task(s)", history.len());
            for t in history {
                println!("  [{}] {} — {}", t.status, &t.id[..8.min(t.id.len())], t.goal);
            }
        }

        Commands::Sail { port, host, max_agents, repo } => {
            let store = MemoryStore::open(db_path()).await?;
            let bus: EventBus<TaskStatus> = EventBus::new(256);
            let queue = TaskQueue::new();
            let pool = AgentPool::new(max_agents, repo, queue.clone(), bus.clone())
                .with_store(store.clone());

            println!("🚢 lopi sail — max_agents={max_agents}, web on :{port}");
            tokio::spawn(pool.run());
            lopi_ui::web::serve(store, bus, queue, &host, port).await?;
        }
    }
    Ok(())
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
            pr_url.as_deref().map(|u| format!(", pr={u}")).unwrap_or_default()
        ),
        TaskStatus::Failed { reason } => format!("failed ❌ {reason}"),
        TaskStatus::RolledBack => "rolled back".into(),
    }
}
