//! Streaming plan/implement calls — forwards each decoded `StreamEvent` from
//! the CLI onto the event bus as it arrives (log line + structured pane
//! events), instead of waiting for the full response. Split out of
//! `run_loop.rs` to keep that module under the file-size gate once the
//! no-progress stall guard landed alongside it.

use super::AgentRunner;
use crate::claude::ClaudeCode;
use anyhow::Result;
use lopi_core::AgentEvent;

impl AgentRunner {
    /// Stream a plan from the CLI, forwarding both the human log line and the
    /// structured pane events (tool calls, token usage, cost, phase, rate
    /// limits) to the event bus as each `StreamEvent` arrives.
    pub(super) async fn stream_plan(&self, claude: &ClaudeCode) -> Result<String> {
        let bus = self.bus.clone();
        let tid = self.id();
        claude
            .plan_streamed(&self.task, self.last_error.as_deref(), move |ev| {
                forward_stream_event(&bus, tid, ev);
            })
            .await
    }

    /// Stream the implementation from the CLI, forwarding each `StreamEvent`'s
    /// log line and structured pane events to the event bus.
    pub(super) async fn stream_implement(&self, claude: &ClaudeCode, plan: &str) -> Result<String> {
        let bus = self.bus.clone();
        let tid = self.id();
        claude
            .implement_streamed(&self.task, plan, move |ev| {
                forward_stream_event(&bus, tid, ev);
            })
            .await
    }
}

/// Fan a single decoded `StreamEvent` onto the bus: the formatted log line as a
/// `LogLine` (for the log panel and thought stream) and every structured event
/// (for the token, cost, phase, and tool panes).
fn forward_stream_event(
    bus: &lopi_core::EventBus<AgentEvent>,
    tid: lopi_core::TaskId,
    ev: &crate::claude_events::StreamEvent,
) {
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
