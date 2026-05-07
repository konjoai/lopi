#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::{Context, Result};
use lopi_core::{AgentEvent, EventBus};

/// Connect to a running lopi sail WebSocket and drive the TUI from network events.
pub async fn watch_remote(ws_url: String) -> Result<()> {
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

pub async fn reqwest_cancel(url: &str) -> Result<String> {
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
