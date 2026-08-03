# Work order: cost circuit breaker (per-task + per-day token ceilings)

Source: `KONJO_REVIEW_PIPELINE_PLAN.md` section 3.3 (L25) and section 4 (Phase 0), Sprint P0
companion doc section 4. This document is the work order the sprint explicitly asked for
instead of a speculative patch to `lopi-agent`'s hot path.

## What ships now vs. what this work order is for

**Shipped this sprint** (`crates/lopi-core/src/cost_breaker.rs`, `crates/lopi-core/src/economics_config.rs`):
- `CostCircuitBreaker::check(task_tokens_so_far, day_tokens_so_far) -> Result<(), CeilingExceeded>` —
  pure decision logic, no I/O, unit-tested with stubbed counters (6 passing tests).
- `EconomicsConfig::per_task_token_ceiling: Option<u64>` and `daily_token_ceiling: Option<u64>` —
  the config surface, `None` by default (opt-in, matching every other field in this config).

**Not shipped this sprint, tracked here**: wiring `CostCircuitBreaker::check` into the two
places lopi actually spends tokens, so a ceiling hit hard-stops *before* the next call
rather than being computed and ignored.

## Why this is a work order, not a patch

`ClaudeCode` (`crates/lopi-agent/src/claude.rs:71`) — the struct both call sites below are
methods on — is a pure CLI-argument builder today. It holds no config handle, no
`MemoryStore` handle, no notion of "today's token spend." Every field is `pub(crate)` and
set through a chain of `with_*` builders in `claude_builders.rs`. Adding a live daily
counter means:

1. Threading a `CostCircuitBreaker` (or an `Arc<EconomicsConfig>`) through that builder
   chain to every `ClaudeCode` construction site in `lopi-agent` and callers in
   `lopi-orchestrator`.
2. Deciding how the *day* counter is sourced live: `crates/lopi-memory/src/store/lessons.rs:91-106`
   (`daily_token_totals`) already computes this from SQLite, but it's an async query, and
   neither integration point below is currently async-DB-aware at the point the check
   would need to run (immediately before `cmd.spawn()`/`cmd.output()`/`.send()`, which are
   already inside an async fn, so the query itself is *possible* — the gap is that nothing
   currently passes a `MemoryStore` handle down to `claude_spawn.rs`).
3. Deciding how the *task* counter is sourced live: `UsageAccrual`
   (`crates/lopi-agent/src/runner/stream.rs:153-181`) already tracks a running per-session
   total via atomics and is the natural source for `task_tokens_so_far` — but it lives one
   layer up (`runner/stream.rs`), not inside `claude_spawn.rs`, so this also needs a value
   or handle passed down, not invented at the call site.

None of this is speculative to *design* (the two sources above already exist and are
correct for the job); it's speculative to *patch* without deciding the plumbing shape, and
a half-wired breaker that compiles but silently never fires would be worse than the current
state (per the plan's own framing: a circuit breaker that doesn't actually check anything
is not a circuit breaker). Hence: work order, not patch.

## Integration points (exact, from this sprint's PF-1 inventory)

### 1. `claude_spawn.rs` — the dominant path (implement AND plan, in practice)

- `crates/lopi-agent/src/claude_spawn.rs:130` — `cmd.spawn()` inside `run_streamed_once`
  (backs `plan_streamed`/`implement_streamed`).
- `crates/lopi-agent/src/claude_spawn.rs:255` — `cmd.output()` inside `run_once` (backs
  `fix()`/`implement_step()`).

Both build a `tokio::process::Command` and then invoke it with no cost gate at all today —
only the CLI's own `--max-budget-usd` (a **USD**, not token, per-session cap; see
`apply_cli_caps`) and the reactive `UsageAccrual::check_hard_stop`
(`crates/lopi-agent/src/runner/stream.rs:277-341`, fires at 95% of `task.cli_budget_usd`,
**after** tokens are already spent mid-stream) exist today. Neither is a pre-call token gate.

Recommended shape (once the plumbing in step 2 below exists):

```rust
// before cmd.spawn() at claude_spawn.rs:130, and before cmd.output() at :255
if let Err(e) = self.cost_breaker.check(self.task_tokens_so_far(), self.day_tokens_so_far()) {
    anyhow::bail!("{ERR_COST_CEILING_EXCEEDED}: {e}");
}
```

`ERR_COST_CEILING_EXCEEDED` should follow the existing `ERR_BUDGET_HARD_STOP` precedent
(defined `crates/lopi-agent/src/claude_model.rs:33`, raised at `claude_spawn.rs:169`) — a
stable string matched by name, not a generic `anyhow::Error` string. Per the plan: **no
retry, no silent degrade, no fallback model** — this must be a terminal error for the task,
propagated the same way `ERR_BUDGET_HARD_STOP` already is: matched by
`crates/lopi-agent/src/runner/terminal_errors.rs:20` (`err_chain.contains(ERR_BUDGET_HARD_STOP)`)
to classify it as terminal rather than retryable. `ERR_COST_CEILING_EXCEEDED` needs the same
terminal-error classification added to that match, not a new bespoke retry-decision path.

### 2. `api_client.rs` — the direct-HTTP planning path

- `crates/lopi-agent/src/api_client.rs:196` — before `.send()` in `stream_plan`.
- `crates/lopi-agent/src/api_client.rs:240` — before `.send()` in `complete`.

Lower priority than (1): per `api_client.rs:1-6`'s own header comment, this path is
currently planning-only and the CLI subprocess path is what "real work" spends tokens on.
Still needs the same gate for completeness — a per-day ceiling that only watches one of the
two paths to the Anthropic API is not a per-day ceiling.

### 3. Plumbing the counters down

- **Day counter**: pass a `MemoryStore` handle (or a cheap `Arc<AtomicU64>` cache refreshed
  periodically from `daily_token_totals`, to avoid a DB round-trip on every single spawn)
  into whatever constructs `ClaudeCode` today. `crates/lopi-orchestrator` is the natural
  owner of both the `MemoryStore` and the per-task loop, so the cache/counter likely lives
  there and gets threaded into `ClaudeCode::with_*` at construction, not fetched fresh
  inside `claude_spawn.rs`.
- **Task counter**: `UsageAccrual` (`runner/stream.rs:153-181`) already has this; expose a
  cheap read (`load(Ordering::Relaxed)`-shaped, matching its existing atomics) and pass that
  read, or the `UsageAccrual` handle itself, down to `claude_spawn.rs`'s two call sites.
- **Do not** reuse `lopi_ratelimit::BudgetGovernor` (`crates/lopi-ratelimit/src/budget.rs`).
  It is unwired dead code by explicit prior decision —
  `crates/lopi-orchestrator/src/budget/mod.rs:1-5` states outright *"why this is built
  fresh here rather than extending `lopi_ratelimit::BudgetGovernor` (unwired dead code —
  never call it from here)."* `CostCircuitBreaker` is a third, narrower thing (pure
  token-count ceilings, not the USD/hourly-rolling shape `BudgetGovernor` and
  `RunawayDetectors::check_hard_ceiling` both already implement) — it does not replace
  either existing mechanism, it adds the one check neither of them is: a pre-call gate on
  raw token counts, checked before the call starts rather than polled or checked mid-stream.

## Config

`per_task_token_ceiling`/`daily_token_ceiling` already parse under `[economics]` in
`lopi.toml` (both `None`/absent by default — no behavior change for any existing install
until an operator sets them). `lopi.toml.example` has no `[economics]` table today; add one
when this is wired, documenting both new fields alongside the existing
`hard_session_ceiling` example.

## Verify (for the wiring PR, not this sprint)

Structural proof only was required this sprint (a stubbed-counter unit test on the pure
logic — done, see `crates/lopi-core/src/cost_breaker.rs`'s test module). The wiring PR
should add: an integration test that constructs a `ClaudeCode` with a ceiling set low
enough that a real (or mocked) spawn trips it, asserting the task fails with
`ERR_COST_CEILING_EXCEEDED` and that no retry/fallback is attempted. Live proof (real token
spend against a real ceiling) requires M3 per the plan and is explicitly out of scope until
then.
