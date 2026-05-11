use anyhow::Result;
use lopi_agent::{AnthropicClient, MODEL_HAIKU};
use lopi_github::GitHubClient;
use lopi_orchestrator::TaskQueue;
use lopi_webhook::{serve as serve_webhooks, TriageConfig};
use std::net::SocketAddr;
use std::sync::Arc;

pub async fn run(
    port: u16,
    host: String,
    webhook_secret: Option<String>,
    github_token: Option<String>,
    anthropic_key: Option<String>,
) -> Result<()> {
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid address: {e}"))?;

    let queue = TaskQueue::new();
    let triage = match (github_token, anthropic_key) {
        (Some(gh_token), Some(anth_key)) => {
            let github = Arc::new(
                GitHubClient::new(gh_token)
                    .map_err(|e| anyhow::anyhow!("GitHub client error: {e}"))?,
            );
            let api_client = Arc::new(AnthropicClient::new(anth_key));
            Some(TriageConfig { api_client, github, limiter: None, breaker: None, model: MODEL_HAIKU.to_string() })
        }
        _ => {
            tracing::warn!("GITHUB_TOKEN or ANTHROPIC_API_KEY missing — issue triage disabled");
            None
        }
    };

    println!("🪝 lopi serve-webhooks on {addr}");
    if triage.is_some() {
        println!("   issue triage: ✅ enabled (Haiku)");
    } else {
        println!("   issue triage: ⚠️  disabled (set GITHUB_TOKEN + ANTHROPIC_API_KEY)");
    }

    serve_webhooks(queue, webhook_secret, addr, triage).await
}
