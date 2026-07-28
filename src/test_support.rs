//! Test-only helper shared by `remote.rs` and `task_commands.rs`'s kill
//! tests — spins up a real `lopi sail` server in-process (the same
//! `lopi_ui::web::serve_with_repo` function `sail_commands::run` calls) so
//! mutation-testing kill tests assert against real server behavior instead
//! of a mock. Split out to avoid duplicating this setup in both files.
#![cfg(test)]
#![allow(clippy::unwrap_used)]

use lopi_core::{AgentEvent, EventBus};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use std::sync::Arc;
use std::time::Duration;

/// Reserve a free OS port by binding then immediately dropping the
/// listener. Small TOCTOU window, standard practice for test harnesses.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Spin up a real `lopi sail` server in-process and wait until
/// `/api/health` answers. Returns the server's base URL.
pub(crate) async fn spawn_live_server(auth_token: Option<String>) -> String {
    let port = free_port();
    let store = MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(64);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(1, std::env::temp_dir(), queue.clone(), bus.clone())
            .with_store(store.clone()),
    );

    tokio::spawn(async move {
        let _ = lopi_ui::web::serve_with_repo(
            store,
            bus,
            queue,
            pool,
            "127.0.0.1",
            port,
            auth_token,
            std::env::temp_dir(),
            Vec::new(),
            None,
        )
        .await;
    });

    let base_url = format!("http://127.0.0.1:{port}");
    let health_client = reqwest::Client::new();
    for _ in 0..100 {
        if health_client
            .get(format!("{base_url}/api/health"))
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    base_url
}
