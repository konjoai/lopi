//! Sprint G — Direct Anthropic API planning path.
//!
//! Replaces the `claude` CLI subprocess for the planning step when the
//! AgentRunner has been configured with an `AnthropicClient`. Implementation
//! still goes through the CLI because file-edit tool access requires it.
//!
//! Resilience layers (in order of execution):
//!   1. CircuitBreaker::check — refuses if breaker is open from prior failures
//!      or if hourly cost cap was hit.
//!   2. AnthropicLimiter::acquire_request — concurrent TPM + RPM enforcement.
//!      Estimated 4000 tokens/plan request seeds the limiter.
//!   3. AnthropicClient::stream_plan — SSE streaming with `cache_control:
//!      ephemeral` on the system prompt. With 5-minute TTL (Feb 2026), each
//!      task's retry loop (typically 3–5 attempts) hits the cache ~90% of the
//!      time, reducing per-attempt cost by 8x on the cached portion.
//!   4. CircuitBreaker::record_success / record_failure / record_cost — feeds
//!      the failure counter and hourly USD spend back into the breaker.
//!
//! On every successful API call:
//!   - Build a TurnMetrics with real input/output/cache token counts and
//!     estimated USD cost.
//!   - Emit `AgentEvent::TurnMetrics` so the lopi-ui Forge animates with
//!     real cost and tokens-per-sec instead of phase-derived stubs.
//!   - Persist the metrics to the SQLite `turn_metrics` table via
//!     `MemoryStore::save_turn_metrics` for offline analysis.

use super::AgentRunner;
use crate::api_client::LOPI_SYSTEM_PROMPT;
use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::{AgentEvent, TurnMetrics};
use std::time::Instant;
use uuid::Uuid;

impl AgentRunner {
    /// True when this runner has been configured to use the direct API
    /// planning path. When false, callers should use the CLI path.
    pub(super) fn has_direct_api(&self) -> bool {
        self.api_client.is_some()
    }

    /// Execute the planning step via the direct Anthropic API instead of
    /// the `claude` CLI. Returns the full plan text.
    ///
    /// The caller is responsible for falling back to the CLI on error —
    /// this method propagates failures to the caller without retrying.
    /// Retries are managed by the outer agent loop.
    pub(super) async fn plan_via_api(&mut self, model: &str, attempt: u8) -> Result<String> {
        let client = self
            .api_client
            .as_ref()
            .context("plan_via_api called without api_client configured")?
            .clone();

        // 1. Circuit breaker — short-circuits if open
        if let Some(b) = &self.breaker {
            b.check().await.context("circuit breaker open")?;
        }

        // 2. Rate limiter — blocks until a request slot AND the estimated
        //    token budget is available. 4000 tokens is a reasonable upper
        //    bound for a planning prompt + completion at default config.
        if let Some(l) = &self.limiter {
            l.acquire_request(4000.0).await;
        }

        // 3. Build the prompt. The system prompt is `LOPI_SYSTEM_PROMPT`
        //    (cached, ephemeral). The user message carries the task goal +
        //    constraints + allowed dirs + lessons + last_error if adaptive retry enabled —
        //    same shape the CLI consumes, minus the TOON wrapper since we're
        //    sending raw API messages.
        let prompt = build_user_prompt(&self.task, self.last_error.as_deref(), &self.task_lessons);

        // 4. Stream the plan, accumulating deltas. Track wall-clock latency
        //    and TTFT. The `on_delta` closure pushes incremental events to
        //    the bus so the UI sees live token streaming rather than waiting
        //    for the full response.
        let task_id = self.task.id;
        let bus = self.bus.clone();
        let mut first_byte: Option<Instant> = None;
        let mut delta_count: u32 = 0;
        let started = Instant::now();

        let result = client
            .stream_plan(model, LOPI_SYSTEM_PROMPT, &prompt, |_text| {
                if first_byte.is_none() {
                    first_byte = Some(Instant::now());
                }
                delta_count += 1;
                // Periodic activity heartbeat — keeps the Forge alive while
                // the model is generating. Sample every 16 deltas to avoid
                // bus contention on long completions.
                if delta_count.is_multiple_of(16) {
                    bus.send(AgentEvent::info(task_id, format!("· {delta_count} deltas")));
                }
            })
            .await;

        let turn_latency = started.elapsed();
        let ttft_ms = first_byte
            .map_or(turn_latency, |t| t.duration_since(started))
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;

        match result {
            Ok((text, usage)) => {
                let cost_usd = usage.estimated_cost(model);

                // 5. Record success + cost on the breaker. Cost feeds the
                //    hourly cap; success resets the consecutive-failure counter.
                if let Some(b) = &self.breaker {
                    b.record_success().await;
                    b.record_cost(cost_usd).await;
                }

                // 6. Build TurnMetrics — single source of truth for
                //    observability. Persisted to SQLite + emitted on bus.
                let tokens_per_sec = if turn_latency.as_secs_f32() > 0.0 {
                    f32::from(
                        u16::try_from(usage.output_tokens.min(u32::from(u16::MAX))).unwrap_or(0),
                    ) / turn_latency.as_secs_f32()
                } else {
                    0.0
                };

                let metrics = TurnMetrics {
                    turn_id: Uuid::new_v4(),
                    task_id,
                    session_id: self.session_id,
                    model: model.to_string(),
                    attempt_number: attempt,
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    cache_read_input_tokens: usage.cache_read_tokens,
                    cache_write_input_tokens: usage.cache_write_tokens,
                    ttft_ms,
                    turn_latency_ms: turn_latency.as_millis().min(u128::from(u64::MAX)) as u64,
                    tool_execution_ms: 0,
                    context_tokens: 0,
                    context_pressure: self.context.token_pressure(),
                    evictions_this_turn: 0,
                    tool_calls: 0,
                    tools_parallel: false,
                    estimated_cost_usd: cost_usd,
                    timestamp: Utc::now(),
                };

                // 7. Emit live UI event — the Forge picks this up and
                //    drives shader uniforms with REAL cost and rate now.
                let cost_f32 = if cost_usd.is_finite() {
                    let bounded = cost_usd.clamp(f64::from(f32::MIN), f64::from(f32::MAX));
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        bounded as f32
                    }
                } else {
                    0.0
                };
                self.bus.send(AgentEvent::TurnMetrics {
                    task_id,
                    pressure: self.context.token_pressure(),
                    activity: 0.45_f32, // planning baseline; SDK doesn't expose live tokens/sec mid-stream
                    tokens_per_sec,
                    cost_usd: cost_f32,
                    tokens: (usage.input_tokens as u64).saturating_add(usage.output_tokens as u64),
                });

                // 8. Persist for offline analytics.
                if let Some(store) = &self.store {
                    if let Err(e) = store.save_turn_metrics(&metrics).await {
                        // Non-fatal — logging only. The agent run continues.
                        tracing::warn!(error = %e, "failed to persist turn metrics");
                    }
                }

                Ok(text)
            }
            Err(e) => {
                // Failure recorded on the breaker. Consecutive failures
                // accumulate toward the trip threshold. The error is
                // returned to the caller for fallback handling.
                if let Some(b) = &self.breaker {
                    b.record_failure().await;
                }
                Err(e.context("direct API plan failed"))
            }
        }
    }
}

/// Render the user prompt the API client sends with the cached system
/// prompt. Keeps it small and deterministic so prompt caching hits.
fn build_user_prompt(
    task: &lopi_core::Task,
    last_error: Option<&str>,
    lessons: &[String],
) -> String {
    let mut parts = Vec::with_capacity(6);
    parts.push(format!("# Task\n{}", task.goal));

    if !task.constraints.is_empty() {
        parts.push(format!(
            "\n# Constraints\n- {}",
            task.constraints.join("\n- ")
        ));
    }
    if !task.allowed_dirs.is_empty() {
        parts.push(format!(
            "\n# Allowed dirs\n- {}",
            task.allowed_dirs.join("\n- ")
        ));
    }
    if !task.forbidden_dirs.is_empty() {
        parts.push(format!(
            "\n# Forbidden dirs\n- {}",
            task.forbidden_dirs.join("\n- ")
        ));
    }
    if !lessons.is_empty() {
        parts.push(format!(
            "\n# Lessons from past patterns\n- {}",
            lessons.join("\n- ")
        ));
    }
    if let Some(err) = last_error {
        parts.push(format!(
            "\n# Previous attempt failed\nAnalyze this error and adjust your approach:\n{}",
            err
        ));
    }
    parts.push(
        "\nProduce a concise step-by-step plan to complete this task. \
         Each step should be a single edit or shell command."
            .to_string(),
    );

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopi_core::{Priority, Task, TaskSource};

    fn task_with(goal: &str, constraints: Vec<String>) -> Task {
        Task {
            id: lopi_core::TaskId::new(),
            goal: goal.into(),
            constraints,
            allowed_dirs: vec!["src/".into()],
            forbidden_dirs: vec![".github/".into()],
            priority: Priority::Normal,
            max_retries: 3,
            created_at: Utc::now(),
            source: TaskSource::Cli,
            repo_path: None,
            output_schema: None,
            tools: Vec::new(),
            required_capabilities: Vec::new(),
            rubric: None,
            base_branch: None,
            model: None,
            effort: None,
        }
    }

    #[test]
    fn user_prompt_includes_goal() {
        let p = build_user_prompt(&task_with("fix the broken test", vec![]), None, &[]);
        assert!(p.contains("fix the broken test"));
        assert!(p.contains("# Task"));
    }

    #[test]
    fn user_prompt_omits_empty_sections() {
        // Constraints empty → no "# Constraints" header
        let p = build_user_prompt(&task_with("g", vec![]), None, &[]);
        assert!(!p.contains("# Constraints"));
        // allowed_dirs is non-empty in the fixture so that header exists
        assert!(p.contains("# Allowed dirs"));
    }

    #[test]
    fn user_prompt_lists_constraints() {
        let p = build_user_prompt(
            &task_with("g", vec!["no panics".into(), "must compile".into()]),
            None,
            &[],
        );
        assert!(p.contains("no panics"));
        assert!(p.contains("must compile"));
        assert!(p.contains("# Constraints"));
    }

    #[test]
    fn user_prompt_ends_with_planning_directive() {
        let p = build_user_prompt(&task_with("g", vec![]), None, &[]);
        assert!(p.contains("step-by-step plan"));
    }

    #[test]
    fn user_prompt_is_deterministic_for_caching() {
        // Same task → byte-identical prompt → cache hit on system+user prefix.
        let t = task_with("g", vec!["a".into(), "b".into()]);
        assert_eq!(
            build_user_prompt(&t, None, &[]),
            build_user_prompt(&t, None, &[])
        );
    }

    #[test]
    fn user_prompt_includes_last_error_when_provided() {
        let t = task_with("g", vec![]);
        let err = "Attempt 1 failed: test_pass_rate: 50.0%";
        let p = build_user_prompt(&t, Some(err), &[]);
        assert!(p.contains("# Previous attempt failed"));
        assert!(p.contains(err));
    }

    #[test]
    fn user_prompt_includes_lessons_when_provided() {
        let t = task_with("g", vec![]);
        let lessons = vec!["use error handling".to_string(), "add logging".to_string()];
        let p = build_user_prompt(&t, None, &lessons);
        assert!(p.contains("# Lessons from past patterns"));
        assert!(p.contains("use error handling"));
        assert!(p.contains("add logging"));
    }

    // ── Builder integration ───────────────────────────────────────────────────
    // Verifies that `with_api()` correctly attaches an AnthropicClient,
    // limiter, and breaker to the runner. has_direct_api() must flip from
    // false → true. Without `with_api()`, the runner falls back to the CLI
    // path so the builder must not unconditionally enable direct-API mode.

    use crate::api_client::AnthropicClient;
    use crate::runner::AgentRunner;
    use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn runner_default_has_no_direct_api() {
        let task = task_with("g", vec![]);
        let (runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));
        assert!(!runner.has_direct_api());
    }

    #[test]
    fn with_api_enables_direct_path() {
        let task = task_with("g", vec![]);
        let (runner, _bus) = AgentRunner::standalone(task, PathBuf::from("."));

        let client = Arc::new(AnthropicClient::new("test-key"));
        let limiter = Arc::new(AnthropicLimiter::default_pro());
        let breaker = Arc::new(CircuitBreaker::new(3, Duration::from_secs(60), 5.0));

        let runner = runner.with_api(client, limiter, breaker);
        assert!(runner.has_direct_api());
    }
}
