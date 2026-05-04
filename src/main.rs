use anyhow::Result;
use clap::{Parser, Subcommand};
use lopi_agent::AgentRunner;
use lopi_core::Task;
use lopi_memory::MemoryStore;
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
    /// Tail agent logs
    Tail {
        #[arg(short, long)]
        task_id: Option<String>,
    },
    /// List all tasks and their status
    Dock,
    /// Start the web dashboard + API server
    Sail {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
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
            println!("🚢 lopi: setting sail — goal: {goal}, repo: {}", repo.display());
            let store = MemoryStore::open(db_path()).await?;
            let task = Task::new(goal);
            store.save_task(&task, "queued").await.ok();
            let (runner, _rx) = AgentRunner::new(task.clone(), repo);
            let outcome = runner.run().await?;
            store.mark_completed(&task.id, &format!("{:?}", outcome)).await.ok();
            println!("⚓ done: {outcome:?}");
        }
        Commands::Watch => {
            let store = MemoryStore::open(db_path()).await?;
            lopi_ui::tui::run(store).await?;
        }
        Commands::Tail { task_id } => {
            let store = MemoryStore::open(db_path()).await?;
            let history = store.load_history(20).await?;
            for t in history.iter().filter(|t| task_id.as_deref().is_none_or(|id| t.id.starts_with(id))) {
                println!("[{}] {} — {}", t.status, &t.id[..8.min(t.id.len())], t.goal);
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
        Commands::Sail { port, host } => {
            let store = MemoryStore::open(db_path()).await?;
            lopi_ui::web::serve(store, &host, port).await?;
        }
    }
    Ok(())
}
