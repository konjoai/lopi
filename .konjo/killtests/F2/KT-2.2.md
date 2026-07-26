# KT-2.2 — Does `estimate_tokens` gate spend anywhere?

**Sprint:** F2 · **Verdict:** PASS (expected outcome) — Phase 5 is a labelling fix, not an enforcement fix.

## Method

Traced every caller of `crates/lopi-context/src/tokens.rs::estimate_tokens`:

- `crates/lopi-context/src/window.rs` — the only call sites (`push`, `push_tool_pair`,
  `pin_conclusion`). Each assigns the estimate to a `TaggedMessage`/tool-pair's
  `.tokens` field and adds it to `ContextWindow::current_tokens`, which feeds:
  - `token_pressure()` — `current_tokens / token_budget`, a `[0.0, 1.0]`
    observability ratio.
  - The local eviction decision inside `push`/`push_tool_pair`
    (`ContextError::Full` when `current_tokens + msg_tokens > token_budget`,
    triggering `eviction::check_expired_tags`/`evict_turn`/`evict_phase`).
- `token_pressure()` is read in exactly two shapes across `lopi-agent`:
  logged via `tracing::info!(pressure = ...)` at phase transitions
  (`runner/run_loop.rs`, `test_phase.rs`, `lifecycle.rs`, `speculative.rs`), and
  written into `TurnMetrics.context_pressure` (`runner/stream.rs:123`,
  `api_plan.rs`) — both observability surfaces, not gates.

Separately, the actual spend/budget-enforcement paths —
`AgentRunner::tokens_used` (an `AtomicU64` summed from streamed `TokenUsage`
events), the `ProgressGate` budget check, `task_budget`/`cli_budget_usd`, and
`TurnMetrics.estimated_cost_usd`/`input_tokens`/`output_tokens` persisted in
`runner/stream.rs`'s `persist_turn` — are all fed from `UsageAccrual`, sourced
from the CLI's real streamed `usage`/`modelUsage` response fields
(`claude_events::parse_result_usage`), never from `estimate_tokens`.

## Verdict

**Pass, as the brief expected.** `estimate_tokens` feeds only:
1. `ContextWindow`'s own internal eviction bookkeeping (what to drop from the
   next prompt when the local budget is full), and
2. `token_pressure`/`TurnMetrics.context_pressure`, both observability.

Real budget accounting — what gates a hard-stop, what `ProgressGate` sees,
what gets billed in `estimated_cost_usd` — comes entirely from the CLI's own
authoritative streamed usage, independent of `estimate_tokens`.

**Consequence:** Phase 5 does not need to touch any enforcement path. It only
needs to (a) replace or (b) relabel the `cl100k_base` estimate powering
`token_pressure`/`context_pressure` — gated on KT-2.4.
