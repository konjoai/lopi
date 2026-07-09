//! Streaming plan/implement calls — forwards each decoded `StreamEvent` from
//! the CLI onto the event bus as it arrives (log line + structured pane
//! events), instead of waiting for the full response. Split out of
//! `run_loop.rs` to keep that module under the file-size gate once the
//! no-progress stall guard landed alongside it.

use super::AgentRunner;
use crate::claude::ClaudeCode;
use crate::claude_events::StreamEvent;
use anyhow::Result;
use lopi_core::AgentEvent;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

impl AgentRunner {
    /// Stream a plan from the CLI, forwarding both the human log line and the
    /// structured pane events (tool calls, token usage, cost, phase, rate
    /// limits) to the event bus as each `StreamEvent` arrives.
    pub(super) async fn stream_plan(&self, claude: &ClaudeCode) -> Result<String> {
        let bus = self.bus.clone();
        let tid = self.id();
        let tokens = self.tokens_used.clone();
        claude
            .plan_streamed(&self.task, self.last_error.as_deref(), move |ev| {
                forward_stream_event(&bus, tid, &tokens, ev);
            })
            .await
    }

    /// Stream the implementation from the CLI, forwarding each `StreamEvent`'s
    /// log line and structured pane events to the event bus.
    pub(super) async fn stream_implement(&self, claude: &ClaudeCode, plan: &str) -> Result<String> {
        let bus = self.bus.clone();
        let tid = self.id();
        let tokens = self.tokens_used.clone();
        claude
            .implement_streamed(&self.task, plan, move |ev| {
                forward_stream_event(&bus, tid, &tokens, ev);
            })
            .await
    }
}

/// The input + output tokens a single `StreamEvent` reports, for budget
/// metering (A3). Only [`StreamEvent::TokenUsage`] carries incremental usage;
/// every other event contributes `0`. Cache-read tokens are excluded — they are
/// not billed against the loop's generation budget.
fn event_tokens(ev: &StreamEvent) -> u64 {
    match ev {
        StreamEvent::TokenUsage {
            output_tokens,
            input_tokens,
            ..
        } => u64::from(*output_tokens) + u64::from(*input_tokens),
        _ => 0,
    }
}

/// Fan a single decoded `StreamEvent` onto the bus: the formatted log line as a
/// `LogLine` (for the log panel and thought stream) and every structured event
/// (for the token, cost, phase, and tool panes). Also meters the event's token
/// usage into `tokens` so the budget gate can cap the loop (A3).
fn forward_stream_event(
    bus: &lopi_core::EventBus<AgentEvent>,
    tid: lopi_core::TaskId,
    tokens: &Arc<AtomicU64>,
    ev: &StreamEvent,
) {
    let spent = event_tokens(ev);
    if spent > 0 {
        tokens.fetch_add(spent, Ordering::Relaxed);
    }
    if let Some(line) = ev.log_line() {
        let t = line.trim().to_string();
        if !t.is_empty() {
            bus.send(AgentEvent::info(tid, t));
        }
    }
    for structured in ev.structured_events(tid) {
        bus.send(structured);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_usage_event_sums_input_and_output() {
        let ev = StreamEvent::TokenUsage {
            output_tokens: 120,
            input_tokens: 80,
            cache_read_tokens: 5000,
        };
        // Cache reads are excluded; only input + output count.
        assert_eq!(event_tokens(&ev), 200);
    }

    #[test]
    fn non_usage_events_contribute_no_tokens() {
        assert_eq!(event_tokens(&StreamEvent::Other), 0);
        assert_eq!(event_tokens(&StreamEvent::Status("x".into())), 0);
    }
}
