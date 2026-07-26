//! Split out of `api_client.rs` purely to keep that file under the 500-line
//! CI gate — the raw Anthropic Messages API wire types (request/response
//! JSON shapes, SSE event variants) and the SSE stream decoder have no
//! dependency on `AnthropicClient`'s HTTP plumbing.

use super::ApiUsage;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub(super) struct CacheControl {
    #[serde(rename = "type")]
    kind: &'static str,
}

impl CacheControl {
    const fn ephemeral() -> Self {
        Self { kind: "ephemeral" }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct SystemBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

impl<'a> SystemBlock<'a> {
    pub(super) fn cached(text: &'a str) -> Self {
        Self {
            kind: "text",
            text,
            cache_control: Some(CacheControl::ephemeral()),
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct UserMessage<'a> {
    pub(super) role: &'static str,
    pub(super) content: &'a str,
}

#[allow(clippy::struct_field_names)]
#[derive(Debug, Deserialize)]
pub(super) struct UsageBlock {
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
pub(super) struct CompleteResp {
    pub(super) content: Vec<CompleteContentItem>,
    pub(super) usage: CompleteUsage,
}

#[derive(Deserialize)]
pub(super) struct CompleteContentItem {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) text: Option<String>,
}

/// Alias so `api_client.rs` can build an [`ApiUsage`] from `CompleteResp.usage`
/// without reaching into this module's private `UsageBlock`.
pub(super) type CompleteUsage = UsageBlock;

impl CompleteUsage {
    pub(super) fn into_api_usage(self, model_deprecation_warning: Option<String>) -> ApiUsage {
        ApiUsage {
            input_tokens: self.input_tokens.unwrap_or(0),
            output_tokens: self.output_tokens.unwrap_or(0),
            cache_read_tokens: self.cache_read_input_tokens.unwrap_or(0),
            cache_write_tokens: self.cache_creation_input_tokens.unwrap_or(0),
            model_deprecation_warning,
        }
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
pub(super) async fn decode_sse_stream<R>(
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
