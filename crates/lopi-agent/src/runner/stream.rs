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
                if let Some(estimated_usd) = acc.check_hard_stop(&model_owned, cap_usd) {
                    emit_budget_hard_stop(&bus, tid, estimated_usd, cap_usd);
                    return false;
                }
                true
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
                if let Some(estimated_usd) = acc.check_hard_stop(&model_owned, cap_usd) {
                    emit_budget_hard_stop(&bus, tid, estimated_usd, cap_usd);
                    return false;
                }
                true
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
            // Authoritative sub-agent-inclusive totals from the terminal
            // `result` when present, else the parent-only delta sums.
            input_tokens: accrual.input_tokens(),
            output_tokens: accrual.output_tokens(),
            cache_read_input_tokens: accrual.cache_read_tokens(),
            cache_write_input_tokens: accrual.cache_write_tokens(),
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
///
/// The incremental `TokenUsage` deltas only report the *parent* agent, so on a
/// run that fans out into sub-agents they undercount the persisted token
/// columns badly (while the billed cost, taken from the terminal `result`, is
/// already correct). The `result` envelope also carries an authoritative,
/// sub-agent-inclusive token total (summed across `modelUsage`); when present
/// it supersedes the deltas for the persisted counts — same envelope, same
/// trust as the cost.
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
    /// Latches once [`check_hard_stop`](Self::check_hard_stop) has requested
    /// an abort, so a stream already being killed doesn't re-request it on
    /// every remaining event in the same decoded line.
    hard_stopped: AtomicBool,
    /// Authoritative cumulative token totals from the terminal `result`'s
    /// usage breakdown — sub-agent-inclusive. Preferred over the delta sums
    /// for the persisted `turn_metrics` row once `has_authoritative` is set.
    auth_input: AtomicU64,
    auth_output: AtomicU64,
    auth_cache_read: AtomicU64,
    auth_cache_write: AtomicU64,
    /// Whether the terminal `result` carried an authoritative usage breakdown.
    has_authoritative: AtomicBool,
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
            StreamEvent::Result {
                total_cost_usd,
                usage,
                ..
            } => {
                self.cost_bits
                    .store(total_cost_usd.to_bits(), Ordering::Relaxed);
                self.saw_result.store(true, Ordering::Relaxed);
                if let Some(u) = usage {
                    self.auth_input.store(u.input_tokens, Ordering::Relaxed);
                    self.auth_output.store(u.output_tokens, Ordering::Relaxed);
                    self.auth_cache_read
                        .store(u.cache_read_tokens, Ordering::Relaxed);
                    self.auth_cache_write
                        .store(u.cache_write_tokens, Ordering::Relaxed);
                    self.has_authoritative.store(true, Ordering::Relaxed);
                }
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

    /// Effective input tokens for the persisted row: the authoritative,
    /// sub-agent-inclusive total when the terminal `result` carried one, else
    /// the parent-only delta sum.
    fn input_tokens(&self) -> u32 {
        self.effective(&self.auth_input, &self.input)
    }

    /// Effective output tokens — see [`input_tokens`](Self::input_tokens).
    fn output_tokens(&self) -> u32 {
        self.effective(&self.auth_output, &self.output)
    }

    /// Effective cache-read tokens — see [`input_tokens`](Self::input_tokens).
    fn cache_read_tokens(&self) -> u32 {
        self.effective(&self.auth_cache_read, &self.cache_read)
    }

    /// Effective cache-write tokens — see [`input_tokens`](Self::input_tokens).
    fn cache_write_tokens(&self) -> u32 {
        self.effective(&self.auth_cache_write, &self.cache_write)
    }

    /// Pick the authoritative counter when the `result` supplied one, else the
    /// delta-summed fallback — saturating into the `u32` the row stores.
    fn effective(&self, authoritative: &AtomicU64, delta: &AtomicU64) -> u32 {
        if self.has_authoritative.load(Ordering::Relaxed) {
            authoritative.saturating_u32()
        } else {
            delta.saturating_u32()
        }
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

    /// Hard-stop check: same estimated-cost basis as [`check_soft_warn`], but
    /// fires at [`HARD_STOP_MARGIN`] of `cap_usd` rather than 80%, and the
    /// caller kills the subprocess on `Some` rather than merely logging.
    /// This is lopi's own backstop for the CLI's `--max-budget-usd` flag,
    /// which only checks its *billed* total between turns — a turn packing
    /// several serial `WebFetch`/`WebSearch` calls can accumulate real cost
    /// mid-turn and only get compared against the cap once the whole turn
    /// finishes, by which point spend may already be well past it.
    /// Estimating from token counts observed *within* the turn closes that
    /// window instead of trusting the CLI's own end-of-turn accounting.
    /// `cap_usd <= 0.0` (the "disabled" sentinel) never stops.
    fn check_hard_stop(&self, model: &str, cap_usd: f64) -> Option<f64> {
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
        if estimated < cap_usd * HARD_STOP_MARGIN {
            return None;
        }
        if self.hard_stopped.swap(true, Ordering::Relaxed) {
            return None;
        }
        Some(estimated)
    }
}

/// Fraction of `cap_usd` at which [`UsageAccrual::check_hard_stop`] fires —
/// below 100%, not at it. The check only re-evaluates when a
/// `StreamEvent::TokenUsage` delta arrives, and the closure returning
/// `false` only *requests* the subprocess be killed — real spend can still
/// tick up between that last observed delta and the moment the process
/// actually exits. Streaming deltas land in small chunks (tens to low
/// hundreds of tokens per event, a small fraction of the cost of most
/// configured caps), so 5% headroom comfortably absorbs one more such
/// chunk without giving up enough of the user's budget to make the stop
/// fire meaningfully early on a legitimately expensive run.
const HARD_STOP_MARGIN: f64 = 0.95;

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

/// Budget & Guardrail Controls — lopi's own hard stop: estimated cost reached
/// 100% of the resolved cap, so the caller is about to kill the subprocess
/// rather than let the CLI's own between-turn accounting decide when to
/// (see [`UsageAccrual::check_hard_stop`]). Reuses [`AgentEvent::BudgetExceeded`]
/// (scope [`BudgetScope::Task`](lopi_core::BudgetScope::Task)) — the same wire
/// shape the fleet-hourly governor emits — so the web Forge's existing
/// budget-exceeded handling picks this up with no new client-side plumbing.
fn emit_budget_hard_stop(
    bus: &lopi_core::EventBus<AgentEvent>,
    tid: lopi_core::TaskId,
    estimated_usd: f64,
    cap_usd: f64,
) {
    tracing::warn!(
        task_id = %tid,
        estimated_usd,
        cap_usd,
        "session cost reached its resolved --max-budget-usd cap — killing the subprocess"
    );
    bus.send(AgentEvent::info(
        tid,
        format!(
            "● budget hard-stop: estimated ${estimated_usd:.4} reached the ${cap_usd:.2} cap — ending this session"
        ),
    ));
    bus.send(AgentEvent::BudgetExceeded {
        task_id: Some(tid),
        scope: lopi_core::BudgetScope::Task,
        limit_usd: cap_usd,
        burned_usd: estimated_usd,
    });
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
