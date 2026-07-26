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

use crate::claude::model_haiku;

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
    /// Sprint F2 Phase 4 — set when the response carried a model-deprecation
    /// warning header (see [`detect_deprecation_warning`]). `None` on the
    /// overwhelming majority of calls; callers with bus access (see
    /// `runner/api_plan.rs`) surface a `Some` value as an
    /// [`lopi_core::AgentEvent::warn`] so a future hard retirement shows up
    /// as a visible warning well before it becomes a silent outage.
    pub model_deprecation_warning: Option<String>,
}

/// Scan response headers for a model-deprecation warning. Anthropic signals
/// an upcoming hard retirement via a response header before the retirement
/// date arrives; lopi did not read response headers at all before Sprint F2
/// Phase 4. Matches by substring on the header *name* (case-insensitive,
/// containing `"deprecat"`) rather than one exact hardcoded name, since the
/// precise header name is not itself a stable, hand-verifiable API contract
/// worth pinning a single literal to — a substring match degrades gracefully
/// if Anthropic's exact header name shifts, where an exact match would
/// silently stop firing.
#[must_use]
pub fn detect_deprecation_warning(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers.iter().find_map(|(name, value)| {
        if name.as_str().to_ascii_lowercase().contains("deprecat") {
            let value = value.to_str().unwrap_or("(unreadable header value)");
            Some(format!("{name}: {value}"))
        } else {
            None
        }
    })
}

impl ApiUsage {
    /// Estimated USD cost for the given model, using rates from
    /// [`crate::pricing`] (Sprint F2 Phase 3 — externalized, no recompile
    /// needed to change a rate).
    ///
    /// This is a **fallback** estimate, not the primary cost source: prefer
    /// the CLI's own authoritative `total_cost_usd`
    /// (`ClaudeOutput::cost_usd`) wherever it's present. This estimator
    /// backs only the direct-API planning path and the mid-stream
    /// `--max-budget-usd` check, neither of which the CLI's reported cost
    /// covers.
    #[must_use]
    pub fn estimated_cost(&self, model: &str) -> f64 {
        let rates = crate::pricing::rates_for(model);
        let mtok = 1_000_000.0_f64;
        (f64::from(self.input_tokens) * rates.input
            + f64::from(self.output_tokens) * rates.output
            + f64::from(self.cache_read_tokens) * rates.cache_read
            + f64::from(self.cache_write_tokens) * rates.cache_write)
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
        task_budget: Option<u64>,
        mut on_delta: F,
    ) -> Result<(String, ApiUsage)>
    where
        F: FnMut(&str) + Send,
    {
        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": 8192,
            "stream": true,
            "system": [SystemBlock::cached(system)],
            "messages": [UserMessage { role: "user", content: prompt }],
        });

        // Phase 16.6 — attach the per-run task budget so the model paces itself
        // instead of being hard-cut. Resolved (model-gated, clamped) by
        // `api_budget`; dropped silently on models that reject the parameter.
        let budget = crate::api_budget::effective_task_budget(model, task_budget);
        if let Some(total) = budget {
            body["output_config"] = crate::api_budget::task_budget_output_config(total);
        }

        let mut req = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");
        if budget.is_some() {
            req = req.header("anthropic-beta", crate::api_budget::TASK_BUDGETS_BETA);
        }

        let resp = req
            .json(&body)
            .send()
            .await
            .context("sending streaming plan request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API {status}: {body}");
        }

        let deprecation_warning = detect_deprecation_warning(resp.headers());
        let stream = resp.bytes_stream();
        let reader = BufReader::new(tokio_util::io::StreamReader::new(
            stream.map(|r: reqwest::Result<bytes::Bytes>| r.map_err(std::io::Error::other)),
        ));
        let (text, mut usage) = decode_sse_stream(reader, &mut on_delta).await?;
        usage.model_deprecation_warning = deprecation_warning;
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

        let deprecation_warning = detect_deprecation_warning(resp.headers());
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
            model_deprecation_warning: deprecation_warning,
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
            .complete(model_haiku(), "You are a test probe.", "Respond with OK.", 10)
            .await?;
        if text.trim().is_empty() {
            anyhow::bail!("canary probe returned empty response");
        }
        Ok(())
    }
}

/// Decode one SSE stream from Anthropic's streaming Messages API into the
/// accumulated response text and token usage, invoking `on_delta` for each
/// text delta as it arrives.
///
/// Split out of `stream_plan` so the parsing logic — `event:`/`data:` line
/// dispatch, `[DONE]` handling, per-`SseEvent`-variant usage accounting, and
/// the SSE `error` event — is testable against synthetic in-memory SSE
/// bytes, independent of a real HTTP response (`stream_plan` builds `reader`
/// from `resp.bytes_stream()`; a test can instead wrap a `Cursor` over a
/// literal SSE payload).
async fn decode_sse_stream<R>(
    reader: R,
    on_delta: &mut (dyn FnMut(&str) + Send),
) -> Result<(String, ApiUsage)>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut text = String::new();
    let mut usage = ApiUsage::default();
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await.context("reading SSE stream")? {
        if line.starts_with("event:") {
            continue;
        }
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let ev: SseEvent = match serde_json::from_str(data) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "failed to decode SSE data line; skipping");
                continue;
            }
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
    }

    Ok((text, usage))
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
#[path = "api_client_tests.rs"]
mod tests;
