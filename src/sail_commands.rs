use anyhow::Result;
use lopi_core::{AgentEvent, EventBus, LopiConfig};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{boot_scheduler, AgentPool, TaskQueue};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db_path;

pub async fn run(
    max_agents: usize,
    repo: PathBuf,
    host: String,
    port: u16,
    cfg: Option<&LopiConfig>,
) -> Result<()> {
    let store = MemoryStore::open(db_path()).await?;
    let bus: EventBus<AgentEvent> = EventBus::new(512);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(max_agents, repo.clone(), queue.clone(), bus.clone()).with_store(store.clone()),
    );

    print_startup_banner(max_agents, &repo, &host, port);

    let schedules = cfg.map(|c| c.schedules.clone()).unwrap_or_default();
    if !schedules.is_empty() {
        println!("   schedules: {} configured", schedules.len());
        let pool_sched = (*pool).clone();
        tokio::spawn(async move {
            match boot_scheduler(schedules, pool_sched).await {
                Ok(_sched) => { tokio::signal::ctrl_c().await.ok(); }
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

    let pool_handle = (*pool).clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down — cancelling all running agents");
        pool_handle.shutdown().await;
        std::process::exit(0);
    });

    if let Ok(token) = std::env::var("TELOXIDE_TOKEN") {
        spawn_telegram(token, queue.clone(), store.clone(), cfg);
    }

    let auth_token = cfg.and_then(|c| c.web.auth_token.clone());
    lopi_ui::web::serve_with_repo(store, bus, queue, pool, &host, port, auth_token, repo).await
}

fn print_startup_banner(max_agents: usize, repo: &Path, host: &str, port: u16) {
    println!("🚢 lopi sail");
    println!("   agents:    up to {max_agents} concurrent");
    println!("   repo:      {}", repo.display());
    println!("   dashboard: http://{host}:{port}");
    println!("   api:       http://{host}:{port}/api/tasks");
    println!("   ws:        ws://{host}:{port}/ws");
}

fn spawn_telegram(token: String, queue: TaskQueue, store: MemoryStore, cfg: Option<&LopiConfig>) {
    let allowed_chat_ids = cfg
        .map(|c| c.remote.telegram.allowed_chat_ids.clone())
        .unwrap_or_default();
    tokio::spawn(async move {
        if let Err(e) = lopi_remote::telegram::run(token, queue, store, allowed_chat_ids).await {
            tracing::error!("telegram bot error: {e}");
        }
    });
}
