use anyhow::Result;
use lopi_agent::{AnthropicClient, MODEL_HAIKU};
use lopi_github::GitHubClient;
use lopi_orchestrator::TaskQueue;
use lopi_webhook::{serve as serve_webhooks, TriageConfig};
use std::net::SocketAddr;
use std::sync::Arc;

/// Enforce the fail-closed webhook-secret policy: refuse to boot without a
/// signing secret unless the `LOPI_ALLOW_UNVERIFIED_WEBHOOK=1` escape hatch
/// is set for local/test use. Previously an unset secret silently disabled
/// GitHub HMAC signature verification (fail-open) rather than refusing to
/// start.
fn enforce_webhook_secret_policy(secret: &Option<String>, allow_unverified: bool) -> Result<()> {
    if secret.is_some() {
        return Ok(());
    }
    if allow_unverified {
        tracing::warn!(
            "LOPI_WEBHOOK_SECRET not set — running with UNVERIFIED webhook signatures \
             (LOPI_ALLOW_UNVERIFIED_WEBHOOK=1 escape hatch active). Do not use in production."
        );
        return Ok(());
    }
    anyhow::bail!(
        "refusing to start serve-webhooks: LOPI_WEBHOOK_SECRET is not set. GitHub webhook \
         HMAC verification is mandatory outside local/test use. Set LOPI_WEBHOOK_SECRET, or \
         set LOPI_ALLOW_UNVERIFIED_WEBHOOK=1 to explicitly run unverified for local/test use."
    )
}

/// Parse the `LOPI_ALLOW_UNVERIFIED_WEBHOOK` escape-hatch env var. Only the
/// exact value `"1"` enables it — unset, empty, or any other value fails
/// closed, matching `enforce_webhook_secret_policy`'s default-deny stance.
fn parse_allow_unverified(raw: Option<&str>) -> bool {
    raw == Some("1")
}

pub async fn run(
    port: u16,
    host: String,
    webhook_secret: Option<String>,
    github_token: Option<String>,
    anthropic_key: Option<String>,
) -> Result<()> {
    let raw_env = std::env::var("LOPI_ALLOW_UNVERIFIED_WEBHOOK").ok();
    let allow_unverified = parse_allow_unverified(raw_env.as_deref());
    enforce_webhook_secret_policy(&webhook_secret, allow_unverified)?;

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
            Some(TriageConfig {
                api_client,
                github,
                limiter: None,
                breaker: None,
                model: MODEL_HAIKU.to_string(),
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_configured_boots_regardless_of_escape_hatch() {
        let secret = Some("s3cret".to_string());
        assert!(enforce_webhook_secret_policy(&secret, false).is_ok());
        assert!(enforce_webhook_secret_policy(&secret, true).is_ok());
    }

    #[test]
    fn no_secret_and_no_escape_hatch_fails_closed() {
        assert!(enforce_webhook_secret_policy(&None, false).is_err());
    }

    #[test]
    fn no_secret_with_escape_hatch_boots_unverified() {
        assert!(enforce_webhook_secret_policy(&None, true).is_ok());
    }

    #[test]
    fn parse_allow_unverified_requires_exact_one() {
        assert!(parse_allow_unverified(Some("1")));
        assert!(!parse_allow_unverified(Some("0")));
        assert!(!parse_allow_unverified(Some("true")));
        assert!(!parse_allow_unverified(Some("")));
        assert!(!parse_allow_unverified(None));
    }

    /// Regression test exercising `run()` itself (not just the extracted
    /// policy helper): with no webhook secret and the escape hatch unset in
    /// the ambient environment, `run()` must fail before ever attempting to
    /// bind a socket.
    #[tokio::test]
    async fn run_fails_closed_without_secret_or_escape_hatch() {
        // SAFETY-relevant only in the sense that env vars are process-global;
        // no other test in this binary reads/writes this key.
        std::env::remove_var("LOPI_ALLOW_UNVERIFIED_WEBHOOK");
        let result = run(0, "127.0.0.1".to_string(), None, None, None).await;
        assert!(result.is_err());
    }
}
