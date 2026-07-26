use anyhow::Result;
use lopi_agent::{model_haiku, AnthropicClient};
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

/// Build the optional issue-triage config from `run()`'s two credential
/// inputs. `Some` only when both are present — a missing credential leaves
/// triage disabled (logged, non-fatal) rather than failing the whole server,
/// since issue triage is an optional feature of `serve-webhooks`, unlike the
/// mandatory webhook-secret policy above.
fn build_triage_config(
    github_token: Option<String>,
    anthropic_key: Option<String>,
) -> Result<Option<TriageConfig>> {
    match (github_token, anthropic_key) {
        (Some(gh_token), Some(anth_key)) => {
            let github = Arc::new(
                GitHubClient::new(gh_token)
                    .map_err(|e| anyhow::anyhow!("GitHub client error: {e}"))?,
            );
            let api_client = Arc::new(AnthropicClient::new(anth_key));
            Ok(Some(TriageConfig {
                api_client,
                github,
                limiter: None,
                breaker: None,
                model: model_haiku().to_string(),
            }))
        }
        _ => {
            tracing::warn!("GITHUB_TOKEN or ANTHROPIC_API_KEY missing — issue triage disabled");
            Ok(None)
        }
    }
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
    let triage = build_triage_config(github_token, anthropic_key)?;

    println!("🪝 lopi serve-webhooks on {addr}");
    if triage.is_some() {
        println!("   issue triage: ✅ enabled (Haiku)");
    } else {
        println!("   issue triage: ⚠️  disabled (set GITHUB_TOKEN + ANTHROPIC_API_KEY)");
    }

    serve_webhooks(queue, webhook_secret, addr, triage).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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

    #[test]
    fn build_triage_config_enables_when_both_credentials_present() {
        let triage =
            build_triage_config(Some("gh-token".to_string()), Some("anth-key".to_string()))
                .expect("both credentials present should not error");
        assert!(
            triage.is_some(),
            "triage must be enabled when both credentials are present"
        );
    }

    #[test]
    fn build_triage_config_disabled_when_github_token_missing() {
        let triage = build_triage_config(None, Some("anth-key".to_string()))
            .expect("missing credential is not an error");
        assert!(triage.is_none());
    }

    #[test]
    fn build_triage_config_disabled_when_anthropic_key_missing() {
        let triage = build_triage_config(Some("gh-token".to_string()), None)
            .expect("missing credential is not an error");
        assert!(triage.is_none());
    }

    #[test]
    fn build_triage_config_disabled_when_both_missing() {
        let triage = build_triage_config(None, None).expect("missing credentials is not an error");
        assert!(triage.is_none());
    }
}
