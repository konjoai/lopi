//! Streaming plan/implement calls — forwards each decoded `StreamEvent` from
//! the CLI onto the event bus as it arrives (log line + structured pane
//! events), instead of waiting for the full response. Split out of
//! `run_loop.rs` to keep that module under the file-size gate once the
//! no-progress stall guard landed alongside it.
//!
//! Each streamed call also **accrues** the turn's real token usage and billed
//! cost and persists a `turn_metrics` row when it completes. Without this the
//! CLI path — which handles every real run (always the implement step, and the
//! plan step unless the direct-API path is configured) — left `turn_metrics`
//! empty, so `/api/stats`, `/budget`, the loop traces and macOS's cost surfaces
//! all read `$0` regardless of real spend (bug #3). The direct-API planning
//! path persists its own metrics separately (`api_plan.rs`), so there is no
//! double-count: a given turn is recorded by exactly one path.

use super::AgentRunner;
use crate::api_client::ApiUsage;
use crate::claude::ClaudeCode;
use crate::claude_events::StreamEvent;
use anyhow::Result;
use chrono::Utc;
use lopi_core::{AgentEvent, TurnMetrics};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

impl AgentRunner {
    /// Stream a plan from the CLI, forwarding both the human log line and the
    /// structured pane events (tool calls, token usage, cost, phase, rate
    /// limits) to the event bus as each `StreamEvent` arrives, then persist the
    /// turn's usage/cost to `turn_metrics`.
    pub(super) async fn stream_plan(
        &self,
        claude: &ClaudeCode,
        model: &str,
        attempt: u8,
    ) -> Result<String> {
        let bus = self.bus.clone();
        let tid = self.id();
        let tokens = self.tokens_used.clone();
        let accrual = Arc::new(UsageAccrual::default());
        let acc = accrual.clone();
        let cap_usd = self.cli_budget_usd.unwrap_or(0.0);
        let model_owned = model.to_string();
        let text = claude
            .plan_streamed(&self.task, self.last_error.as_deref(), move |ev| {
                acc.observe(ev);
                forward_stream_event(&bus, tid, &tokens, ev);
                if let Some(estimated_usd) = acc.check_soft_warn(&model_owned, cap_usd) {
                    emit_budget_soft_warn(&bus, tid, estimated_usd, cap_usd);
                }
            })
            .await?;
        self.persist_turn(&accrual, model, attempt).await;
        Ok(text)
    }

    /// Stream the implementation from the CLI, forwarding each `StreamEvent`'s
    /// log line and structured pane events to the event bus, then persist the
    /// turn's usage/cost to `turn_metrics`.
    pub(super) async fn stream_implement(
        &self,
        claude: &ClaudeCode,
        plan: &str,
        model: &str,
        attempt: u8,
    ) -> Result<String> {
        let bus = self.bus.clone();
        let tid = self.id();
        let tokens = self.tokens_used.clone();
        let accrual = Arc::new(UsageAccrual::default());
        let acc = accrual.clone();
        let cap_usd = self.cli_budget_usd.unwrap_or(0.0);
        let model_owned = model.to_string();
        let text = claude
            .implement_streamed(&self.task, plan, move |ev| {
                acc.observe(ev);
                forward_stream_event(&bus, tid, &tokens, ev);
                if let Some(estimated_usd) = acc.check_soft_warn(&model_owned, cap_usd) {
                    emit_budget_soft_warn(&bus, tid, estimated_usd, cap_usd);
                }
            })
            .await?;
        self.persist_turn(&accrual, model, attempt).await;
        Ok(text)
    }

    /// Persist one `turn_metrics` row for a completed CLI stream, so every cost
    /// surface reflects real billed spend (bug #3). No-op when no store is
    /// attached or the stream reported no usage. Failures are logged, not
    /// fatal — a metrics-write hiccup must never fail the agent run.
    async fn persist_turn(&self, accrual: &UsageAccrual, model: &str, attempt: u8) {
        let Some(store) = &self.store else { return };
        if !accrual.has_usage() {
            return;
        }
        let metrics = TurnMetrics {
            turn_id: Uuid::new_v4(),
            task_id: self.task.id,
            session_id: self.session_id,
            model: model.to_string(),
            attempt_number: attempt,
            input_tokens: accrual.input.saturating_u32(),
            output_tokens: accrual.output.saturating_u32(),
            cache_read_input_tokens: accrual.cache_read.saturating_u32(),
            cache_write_input_tokens: accrual.cache_write.saturating_u32(),
            ttft_ms: 0,
            turn_latency_ms: 0,
            tool_execution_ms: 0,
            context_tokens: 0,
            context_pressure: self.context.token_pressure(),
            evictions_this_turn: 0,
            tool_calls: 0,
            tools_parallel: false,
            // Authoritative billed cost from the terminal `result` envelope.
            estimated_cost_usd: accrual.cost_usd(),
            timestamp: Utc::now(),
        };
        if let Err(e) = store.save_turn_metrics(&metrics).await {
            tracing::warn!(error = %e, "failed to persist CLI turn metrics");
        }
    }
}

/// Interior-mutable accumulator for one streamed CLI call. The forwarding
/// closure is `Fn` (no `&mut`), so the running totals live behind atomics: each
/// `TokenUsage` delta adds into the token counters, and the terminal `Result`
/// envelope's cumulative billed cost lands on `cost_bits`.
#[derive(Default)]
struct UsageAccrual {
    input: AtomicU64,
    output: AtomicU64,
    cache_read: AtomicU64,
    cache_write: AtomicU64,
    /// `f64::to_bits` of the terminal `result`'s cumulative `total_cost_usd`.
    cost_bits: AtomicU64,
    /// Whether a terminal `result` was seen (so a run that spent nothing but
    /// completed is still recorded, and a stream that died mid-flight without a
    /// result but produced tokens is too).
    saw_result: AtomicBool,
    /// Latches once [`check_soft_warn`](Self::check_soft_warn) has fired for
    /// this stream, so a session hovering near its cap warns once, not on
    /// every subsequent `TokenUsage` delta.
    warned: AtomicBool,
}

impl UsageAccrual {
    /// Fold one decoded event's usage/cost into the running totals.
    fn observe(&self, ev: &StreamEvent) {
        match ev {
            StreamEvent::TokenUsage {
                output_tokens,
                input_tokens,
                cache_read_tokens,
                cache_write_tokens,
            } => {
                self.output
                    .fetch_add(u64::from(*output_tokens), Ordering::Relaxed);
                self.input
                    .fetch_add(u64::from(*input_tokens), Ordering::Relaxed);
                self.cache_read
                    .fetch_add(u64::from(*cache_read_tokens), Ordering::Relaxed);
                self.cache_write
                    .fetch_add(u64::from(*cache_write_tokens), Ordering::Relaxed);
            }
            StreamEvent::Result { total_cost_usd, .. } => {
                self.cost_bits
                    .store(total_cost_usd.to_bits(), Ordering::Relaxed);
                self.saw_result.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// Whether anything worth persisting was observed.
    fn has_usage(&self) -> bool {
        self.saw_result.load(Ordering::Relaxed)
            || self.input.load(Ordering::Relaxed) > 0
            || self.output.load(Ordering::Relaxed) > 0
    }

    /// The billed cost captured from the terminal `result` (`0.0` if none).
    fn cost_usd(&self) -> f64 {
        f64::from_bits(self.cost_bits.load(Ordering::Relaxed))
    }

    /// Budget & Guardrail Controls Part 4.2 — soft-warn at 80% of the
    /// session's resolved USD cap. The CLI's own `--max-budget-usd` only
    /// hard-stops on the *billed* total, which this accrual only learns at
    /// the terminal `result` (too late for an early warning) — so this
    /// estimates a running cost from token counts observed so far, using the
    /// same per-model rate table as [`ApiUsage::estimated_cost`]. Fires at
    /// most once per streamed call: `warned` latches so a legitimately
    /// expensive `deep`/`unlimited` run gets one heads-up, not one per token
    /// delta. `cap_usd <= 0.0` (the "disabled" sentinel) never warns.
    fn check_soft_warn(&self, model: &str, cap_usd: f64) -> Option<f64> {
        if cap_usd <= 0.0 {
            return None;
        }
        let usage = ApiUsage {
            input_tokens: self.input.saturating_u32(),
            output_tokens: self.output.saturating_u32(),
            cache_read_tokens: self.cache_read.saturating_u32(),
            cache_write_tokens: self.cache_write.saturating_u32(),
        };
        let estimated = usage.estimated_cost(model);
        if estimated < cap_usd * 0.8 {
            return None;
        }
        if self.warned.swap(true, Ordering::Relaxed) {
            return None;
        }
        Some(estimated)
    }
}

/// Saturating `AtomicU64` → `u32` read, for the `u32` token fields of
/// [`TurnMetrics`]. Real per-turn counts are far below `u32::MAX`; the clamp is
/// a defensive guard, not an expected path.
trait SaturatingU32 {
    fn saturating_u32(&self) -> u32;
}

impl SaturatingU32 for AtomicU64 {
    fn saturating_u32(&self) -> u32 {
        u32::try_from(self.load(Ordering::Relaxed)).unwrap_or(u32::MAX)
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
    crate::quota_kill_log::observe_global(ev, Utc::now().timestamp());
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

/// Budget & Guardrail Controls Part 4.2 — self-report a session's estimated
/// cost crossing 80% of its resolved cap, via both a `tracing::warn!` (so
/// it's visible without a UI attached) and a structured [`AgentEvent`] (so a
/// listener — the web Forge, `lopi-remote`'s Telegram notifier — can ping a
/// human before the CLI's own hard stop, not after).
fn emit_budget_soft_warn(
    bus: &lopi_core::EventBus<AgentEvent>,
    tid: lopi_core::TaskId,
    estimated_usd: f64,
    cap_usd: f64,
) {
    tracing::warn!(
        task_id = %tid,
        estimated_usd,
        cap_usd,
        "session cost crossed 80% of its resolved --max-budget-usd cap"
    );
    bus.send(AgentEvent::BudgetSoftWarn {
        task_id: tid,
        estimated_usd,
        cap_usd,
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn token_usage_event_sums_input_and_output() {
        let ev = StreamEvent::TokenUsage {
            output_tokens: 120,
            input_tokens: 80,
            cache_read_tokens: 5000,
            cache_write_tokens: 200,
        };
        // Cache reads/writes are excluded; only input + output count.
        assert_eq!(event_tokens(&ev), 200);
    }

    #[test]
    fn non_usage_events_contribute_no_tokens() {
        assert_eq!(event_tokens(&StreamEvent::Other), 0);
        assert_eq!(event_tokens(&StreamEvent::Status("x".into())), 0);
    }

    fn usage(output: u32, input: u32, read: u32, write: u32) -> StreamEvent {
        StreamEvent::TokenUsage {
            output_tokens: output,
            input_tokens: input,
            cache_read_tokens: read,
            cache_write_tokens: write,
        }
    }

    fn result(cost: f64) -> StreamEvent {
        StreamEvent::Result {
            session_id: "s".into(),
            subtype: "success".into(),
            final_text: String::new(),
            total_cost_usd: cost,
            num_turns: 3,
        }
    }

    #[test]
    fn accrual_sums_token_deltas_across_events() {
        let acc = UsageAccrual::default();
        acc.observe(&usage(100, 3, 16_000, 6_000));
        acc.observe(&usage(50, 1, 22_000, 100));
        assert_eq!(acc.output.saturating_u32(), 150);
        assert_eq!(acc.input.saturating_u32(), 4);
        assert_eq!(acc.cache_read.saturating_u32(), 38_000);
        assert_eq!(acc.cache_write.saturating_u32(), 6_100);
    }

    #[test]
    fn accrual_captures_billed_cost_from_result() {
        let acc = UsageAccrual::default();
        acc.observe(&usage(10, 1, 0, 0));
        acc.observe(&result(0.047_891_6));
        assert!((acc.cost_usd() - 0.047_891_6).abs() < f64::EPSILON);
        assert!(acc.has_usage());
    }

    #[test]
    fn accrual_reports_no_usage_when_empty() {
        let acc = UsageAccrual::default();
        assert!(!acc.has_usage());
        assert_eq!(acc.cost_usd(), 0.0);
    }

    #[test]
    fn accrual_has_usage_on_result_even_with_zero_tokens() {
        let acc = UsageAccrual::default();
        acc.observe(&result(0.0));
        // A completed run that happened to spend nothing is still a real turn.
        assert!(acc.has_usage());
    }

    // ── Budget & Guardrail Controls Part 4.2 — soft-warn at 80% of cap ──────

    #[test]
    fn check_soft_warn_disabled_when_cap_is_zero() {
        let acc = UsageAccrual::default();
        acc.observe(&usage(1_000_000, 1_000_000, 0, 0));
        assert_eq!(acc.check_soft_warn(crate::claude::MODEL_OPUS, 0.0), None);
    }

    #[test]
    fn check_soft_warn_none_under_the_threshold() {
        let acc = UsageAccrual::default();
        // A trickle of tokens on a generous cap — nowhere near 80%.
        acc.observe(&usage(10, 10, 0, 0));
        assert_eq!(acc.check_soft_warn(crate::claude::MODEL_SONNET, 10.0), None);
    }

    #[test]
    fn check_soft_warn_fires_once_at_80_percent() {
        let acc = UsageAccrual::default();
        // Sonnet: 300K output tokens * $15/MTok = $4.50 — 90% of a $5 cap.
        acc.observe(&usage(300_000, 0, 0, 0));
        let first = acc.check_soft_warn(crate::claude::MODEL_SONNET, 5.0);
        assert!(first.is_some(), "must fire once 80% of the cap is crossed");
        assert!((first.unwrap() - 4.5).abs() < 0.01);

        // A second observation at the same (or higher) usage must not
        // re-fire — the warn latches per stream.
        acc.observe(&usage(10, 0, 0, 0));
        assert_eq!(
            acc.check_soft_warn(crate::claude::MODEL_SONNET, 5.0),
            None,
            "must not fire a second time for the same stream"
        );
    }

    #[test]
    fn emit_budget_soft_warn_sends_the_structured_event() {
        let bus: lopi_core::EventBus<AgentEvent> = lopi_core::EventBus::new(4);
        let mut rx = bus.subscribe();
        let tid = lopi_core::TaskId::new();
        emit_budget_soft_warn(&bus, tid, 4.5, 5.0);
        let ev = rx.try_recv().expect("event should have been sent");
        match ev {
            AgentEvent::BudgetSoftWarn {
                task_id,
                estimated_usd,
                cap_usd,
            } => {
                assert_eq!(task_id, tid);
                assert!((estimated_usd - 4.5).abs() < f64::EPSILON);
                assert!((cap_usd - 5.0).abs() < f64::EPSILON);
            }
            other => panic!("expected BudgetSoftWarn, got {other:?}"),
        }
    }
}
