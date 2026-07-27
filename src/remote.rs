#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::{Context, Result};
use lopi_core::{AgentEvent, EventBus};

/// Build the WebSocket handshake request for `ws_url`, attaching
/// `Authorization: Bearer <token>` when `token` is non-empty. Sprint S11,
/// Phase 0 moved `/ws` behind the server's normal auth; before that, this
/// client worked against any server because `/ws` checked nothing at all. A
/// native process (unlike a browser `WebSocket`) can set arbitrary headers
/// on the handshake, so no ticket dance is needed here — just the header.
fn ws_request(
    ws_url: &str,
    token: Option<&str>,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let mut request = ws_url.into_client_request()?;
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        let value = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&format!(
            "Bearer {token}"
        ))?;
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
            value,
        );
    }
    Ok(request)
}

/// Connect to a running lopi sail WebSocket and drive the TUI from network events.
pub async fn watch_remote(ws_url: String) -> Result<()> {
    use futures::StreamExt;
    use tokio_tungstenite::tungstenite::Message as WsMsg;

    let bus: EventBus<AgentEvent> = EventBus::new(512);
    let bus_tx = bus.clone();

    // Try to connect; if it fails immediately, fall back to local mode.
    // `LOPI_WEB_AUTH_TOKEN` mirrors the env var `sail_commands::run` reads
    // server-side — same credential, same name, both ends.
    let token = std::env::var("LOPI_WEB_AUTH_TOKEN").ok();
    let request = ws_request(&ws_url, token.as_deref())?;
    let (mut ws, _) = match tokio_tungstenite::connect_async(request).await {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn no_token_sends_no_authorization_header() {
        let request = ws_request("ws://127.0.0.1:3000/ws", None).unwrap();
        assert!(request
            .headers()
            .get(tokio_tungstenite::tungstenite::http::header::AUTHORIZATION)
            .is_none());
    }

    #[test]
    fn empty_token_sends_no_authorization_header() {
        // `LOPI_WEB_AUTH_TOKEN=""` (set but empty) must behave the same as
        // unset, matching `auth_middleware`'s own "no token configured"
        // fail-open-to-dev-mode posture rather than sending a useless
        // `Bearer `.
        let request = ws_request("ws://127.0.0.1:3000/ws", Some("")).unwrap();
        assert!(request
            .headers()
            .get(tokio_tungstenite::tungstenite::http::header::AUTHORIZATION)
            .is_none());
    }

    #[test]
    fn a_real_token_is_sent_as_a_bearer_header() {
        let request = ws_request("ws://127.0.0.1:3000/ws", Some("secret-token")).unwrap();
        assert_eq!(
            request
                .headers()
                .get(tokio_tungstenite::tungstenite::http::header::AUTHORIZATION)
                .unwrap(),
            "Bearer secret-token"
        );
    }
}
