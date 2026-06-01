// Direct Anthropic API client — implements prompt caching (2.1), SSE streaming (2.2),
// and a shared connection pool (2.5) alongside the existing CLI-based ClaudeCode path.
//
// Architecture: this client is the long-term target for planning calls.
// Implementation calls still go through the `claude` CLI (full tool access).
// Migration path: plan via API → pass plan text to CLI for implementation.

#![allow(clippy::missing_errors_doc)]

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::claude::MODEL_HAIKU;

// ── Shared HTTP client (2.5) ──────────────────────────────────────────────────

/// Lazily initialised singleton reqwest client shared across all API calls.
///
/// TLS handshake costs 50–150 ms on first connection; subsequent calls reuse
/// the same connection. `pool_max_idle_per_host(14)` stays under Anthropic's
/// 15-concurrent-connection limit for Pro tier.
static HTTP: std::sync::OnceLock<Arc<reqwest::Client>> = std::sync::OnceLock::new();

fn shared_http() -> Arc<reqwest::Client> {
    HTTP.get_or_init(|| {
        Arc::new(
            reqwest::Client::builder()
                .pool_max_idle_per_host(14)
                .pool_idle_timeout(Duration::from_secs(90))
                .timeout(Duration::from_secs(300))
                .tcp_keepalive(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|e| {
                    tracing::warn!("reqwest client builder failed ({e}); using default client");
                    reqwest::Client::new()
                }),
        )
    })
    .clone()
}

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: &'static str,
}

impl CacheControl {
    const fn ephemeral() -> Self {
        Self { kind: "ephemeral" }
    }
}

#[derive(Debug, Serialize)]
struct SystemBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

impl<'a> SystemBlock<'a> {
    fn cached(text: &'a str) -> Self {
        Self {
            kind: "text",
            text,
            cache_control: Some(CacheControl::ephemeral()),
        }
    }
}

#[derive(Debug, Serialize)]
struct UserMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[allow(clippy::struct_field_names)]
#[derive(Debug, Deserialize)]
struct UsageBlock {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    cache_read_input_tokens: Option<u32>,
    cache_creation_input_tokens: Option<u32>,
}

// ── SSE event types ───────────────────────────────────────────────────────────
// Wire-format deserialization targets — fields populated by serde, not all read in code.

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseEvent {
    MessageStart {
        message: SseMessageStart,
    },
    ContentBlockStart {
        index: usize,
        content_block: SseContentBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: SseDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: SseMessageDeltaStop,
        usage: Option<UsageBlock>,
    },
    MessageStop,
    Ping,
    Error {
        error: SseErrorDetail,
    },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SseMessageStart {
    usage: Option<UsageBlock>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SseContentBlock {
    #[serde(rename = "type")]
    kind: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SseMessageDeltaStop {
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseErrorDetail {
    message: String,
}

// ── complete() response types ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct CompleteResp {
    content: Vec<CompleteContentItem>,
    usage: UsageBlock,
}

#[derive(Deserialize)]
struct CompleteContentItem {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

// ── Usage record ──────────────────────────────────────────────────────────────

/// Aggregated token usage counters returned by every API call.
#[derive(Debug, Default, Clone)]
pub struct ApiUsage {
    /// Number of prompt tokens billed at the full input rate.
    pub input_tokens: u32,
    /// Number of tokens in the model's response.
    pub output_tokens: u32,
    /// Prompt tokens served from Anthropic's KV cache (billed at ~10% of input rate).
    pub cache_read_tokens: u32,
    /// Prompt tokens written into Anthropic's KV cache this turn.
    pub cache_write_tokens: u32,
}

impl ApiUsage {
    /// Estimated USD cost using Anthropic's 2025-06 pricing for the given model.
    #[must_use]
    pub fn estimated_cost(&self, model: &str) -> f64 {
        let (input_rate, output_rate, cache_read_rate, cache_write_rate) = if model.contains("opus")
        {
            (15.0, 75.0, 1.50, 18.75)
        } else if model.contains("haiku") {
            (0.80, 4.00, 0.08, 1.00)
        } else {
            // sonnet default
            (3.00, 15.0, 0.30, 3.75)
        };
        let mtok = 1_000_000.0_f64;
        (f64::from(self.input_tokens) * input_rate
            + f64::from(self.output_tokens) * output_rate
            + f64::from(self.cache_read_tokens) * cache_read_rate
            + f64::from(self.cache_write_tokens) * cache_write_rate)
            / mtok
    }
}

// ── Client ────────────────────────────────────────────────────────────────────

/// HTTP client for the Anthropic Messages API with prompt caching and SSE streaming.
#[derive(Clone)]
pub struct AnthropicClient {
    http: Arc<reqwest::Client>,
    api_key: String,
}

impl AnthropicClient {
    /// Construct from `ANTHROPIC_API_KEY` env var.
    ///
    /// # Errors
    ///
    /// Returns an error if `ANTHROPIC_API_KEY` is not set in the environment.
    pub fn from_env() -> Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;
        Ok(Self {
            http: shared_http(),
            api_key: key,
        })
    }

    /// Construct from an explicit API key string.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: shared_http(),
            api_key: api_key.into(),
        }
    }

    /// Stream a planning prompt. Returns the full accumulated text and usage.
    ///
    /// The `system` block is sent with `cache_control: {type: "ephemeral"}` so
    /// repeated calls with the same system prompt hit Anthropic's KV cache
    /// (90% cost reduction, 50–85% TTFT reduction after turn 1).
    ///
    /// `on_delta` is called with each text delta as it arrives — enables the
    /// speculative plan step execution path in the agent runner.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the SSE stream contains an error event.
    pub async fn stream_plan<F>(
        &self,
        model: &str,
        system: &str,
        prompt: &str,
        mut on_delta: F,
    ) -> Result<(String, ApiUsage)>
    where
        F: FnMut(&str),
    {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 8192,
            "stream": true,
            "system": [SystemBlock::cached(system)],
            "messages": [UserMessage { role: "user", content: prompt }],
        });

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("sending streaming plan request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API {status}: {body}");
        }

        let mut text = String::new();
        let mut usage = ApiUsage::default();
        let stream = resp.bytes_stream();
        let mut lines = BufReader::new(tokio_util::io::StreamReader::new(
            stream.map(|r: reqwest::Result<bytes::Bytes>| r.map_err(std::io::Error::other)),
        ))
        .lines();

        let mut event_type = String::new();

        while let Some(line) = lines.next_line().await.context("reading SSE stream")? {
            if line.starts_with("event:") {
                event_type = line.trim_start_matches("event:").trim().to_string();
                continue;
            }
            if line.starts_with("data:") {
                let data = line.trim_start_matches("data:").trim();
                if data == "[DONE]" {
                    break;
                }
                let ev: SseEvent = match serde_json::from_str(data) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                match ev {
                    SseEvent::MessageStart { message } => {
                        if let Some(u) = message.usage {
                            usage.input_tokens += u.input_tokens.unwrap_or(0);
                            usage.cache_read_tokens += u.cache_read_input_tokens.unwrap_or(0);
                            usage.cache_write_tokens += u.cache_creation_input_tokens.unwrap_or(0);
                        }
                    }
                    SseEvent::ContentBlockDelta {
                        delta: SseDelta::TextDelta { text: t },
                        ..
                    } => {
                        on_delta(&t);
                        text.push_str(&t);
                    }
                    SseEvent::MessageDelta { usage: Some(u), .. } => {
                        usage.output_tokens += u.output_tokens.unwrap_or(0);
                    }
                    SseEvent::Error { error } => {
                        anyhow::bail!("Anthropic SSE error: {}", error.message);
                    }
                    _ => {}
                }
                let _ = &event_type;
            }
        }

        Ok((text, usage))
    }

    /// Non-streaming single-turn call (for fix and score prompts).
    ///
    /// Uses the cached system block so the system prompt KV is warm from
    /// the preceding streaming plan call.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response cannot be parsed.
    pub async fn complete(
        &self,
        model: &str,
        system: &str,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<(String, ApiUsage)> {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": [SystemBlock::cached(system)],
            "messages": [UserMessage { role: "user", content: prompt }],
        });

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("sending complete request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API {status}: {text}");
        }

        let r: CompleteResp = resp.json().await.context("parsing complete response")?;
        let text: String = r
            .content
            .into_iter()
            .filter(|c| c.kind == "text")
            .filter_map(|c| c.text)
            .collect();

        let usage = ApiUsage {
            input_tokens: r.usage.input_tokens.unwrap_or(0),
            output_tokens: r.usage.output_tokens.unwrap_or(0),
            cache_read_tokens: r.usage.cache_read_input_tokens.unwrap_or(0),
            cache_write_tokens: r.usage.cache_creation_input_tokens.unwrap_or(0),
        };

        Ok((text, usage))
    }

    /// Quick availability probe — sends a 5-token request to Haiku.
    /// Used by the circuit breaker's HALF-OPEN canary.
    ///
    /// # Errors
    ///
    /// Returns an error if the probe request fails or returns an empty response.
    pub async fn canary_probe(&self) -> Result<()> {
        let (text, _) = self
            .complete(MODEL_HAIKU, "You are a test probe.", "Respond with OK.", 10)
            .await?;
        if text.trim().is_empty() {
            anyhow::bail!("canary probe returned empty response");
        }
        Ok(())
    }
}

// ── Lopi system prompt (cached prefix) ───────────────────────────────────────

/// Canonical lopi system prompt injected as a cached block on every API call.
///
/// This is the byte-identical prefix that Anthropic's prompt cache keys on.
/// Any non-deterministic content (timestamps, per-task IDs) must NOT appear here.
pub const LOPI_SYSTEM_PROMPT: &str = "\
You are running inside lopi, a Konjo AI agent orchestrator. \
Your job is to plan and implement software engineering tasks with \
precision, correctness, and efficiency. \
Produce concise, actionable output. \
Never include apologies, preamble, or explanations unless asked. \
Always follow the task constraints exactly.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::MODEL_SONNET;

    #[test]
    fn usage_cost_sonnet() {
        let u = ApiUsage {
            input_tokens: 1_000_000,
            ..ApiUsage::default()
        };
        let cost = u.estimated_cost(MODEL_SONNET);
        assert!(
            (cost - 3.0).abs() < 0.01,
            "sonnet input rate should be $3/MTok"
        );
    }

    #[test]
    fn usage_cost_cache_hit_cheaper() {
        let full = ApiUsage {
            input_tokens: 100_000,
            ..ApiUsage::default()
        };
        let cached = ApiUsage {
            cache_read_tokens: 100_000,
            ..ApiUsage::default()
        };
        assert!(
            cached.estimated_cost(MODEL_SONNET) < full.estimated_cost(MODEL_SONNET),
            "cache read must be cheaper than full input"
        );
    }

    #[test]
    fn shared_http_returns_same_instance() {
        let a = shared_http();
        let b = shared_http();
        assert!(Arc::ptr_eq(&a, &b), "shared_http must return the same Arc");
    }
}
