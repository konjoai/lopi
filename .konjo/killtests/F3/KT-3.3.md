# KT-3.3 — Which loss is acceptable?

**Sprint:** F3 · **Verdict:** CONFIRMED — the brief's expected asymmetry holds; no design inversion needed.
**Baseline commit:** `6688d7d` (`v0.28.0`, post-F2)
**Run first, per the brief's own instruction** — this is the kill-test most
likely to change Phase 4's design, so it was resolved before any code was
written.

## Method

Traced every call site of `record_task_log`, `prune_task_logs`,
`load_task_logs`, `load_recent_task_logs`, and `count_task_logs`
(`crates/lopi-memory/src/store/task_logs.rs`), and every consumer of
`AgentEvent::LogLine` off the live bus, across the whole repo.

### What reads `task_logs`

- `GET /api/tasks/:id/logs` — `crates/lopi-ui/src/web/task_stream_handlers.rs:104`
  (web dashboard's per-task historical tail).
- `GET /api/logs` — `task_stream_handlers.rs:123` (global dashboard Logs tab).
- MCP tool `lopi_get_logs` — `src/mcp_commands/mod.rs:238,391` (Claude
  Desktop / MCP client history read).
- Telegram `/tail` — `crates/lopi-remote/src/telegram/monitor.rs:153`.
- `lopi diag` export — `src/diag_commands.rs:82-85`, dumps
  `load_recent_task_logs` into a committable `task_logs.json` snapshot.
- `count_task_logs`: **no non-test call site anywhere in the repo.**

### What does *not* read `task_logs`

- `lopi replay` (`src/replay_commands.rs:20-33`) reconstructs its
  partial-restart plan from `agent_dag_nodes` via `load_dag_nodes` — a
  completely separate table. Replay correctness has no dependency on
  `task_logs`.
- No "evidence bundle" feature exists that consumes `task_logs`. Every
  "evidence"/"bundle" hit in `docs/`, `CHANGELOG.md`, `LEDGER.md`,
  `NEXT_SESSION_PROMPT.md` refers to kill-test audit evidence or MCPB
  packaging — unrelated.
- No CI/quality-gate code path reads `task_logs`.

### Live `LogLine` consumers independent of the DB

- CLI `lopi run` (`src/run_command.rs:53-88`) and the REPL bypass path
  (`src/repl/actions.rs:142`) subscribe directly to the bus and print
  `LogLine` live — no DB read, ever.
- The ratatui TUI (`crates/lopi-ui/src/tui.rs:144-158`) pushes `LogLine`
  straight from the bus into an in-memory ring buffer; it is read-only and
  never touches `task_logs`.

## The two loss scenarios, confirmed

1. **A broadcast event dropped** (`RecvError::Lagged`,
   `event_bridge.rs:36-38` pre-fix, `task_stream_handlers.rs:77-79`):
   permanent for that subscriber. CLI, REPL, and TUI have **no fallback** —
   they never read `task_logs`, so a dropped live event is simply gone for
   them. This is already tolerated behavior (`task_stream_handlers.rs:6-8`:
   "lagging clients drop frames").
2. **A `task_logs` persist dropped**: degrades only the *retrospective*
   surfaces — web dashboard historical tail, Telegram `/tail`, MCP
   `lopi_get_logs`, `lopi diag`'s snapshot completeness. Nothing reads
   `task_logs` for a decision, a gate, or replay/resume correctness.

## Verdict

**Pass condition met — the asymmetry the brief expected is real, not
assumed.** `task_logs` is load-bearing only for observational/inspection
surfaces, never for a correctness- or replay-relevant computation. The
ordering the brief proposes — *under pressure, drop persistence before
dropping live events* — is confirmed as the right call, not merely
plausible. Phase 4 proceeds with a drop-persistence overflow policy, not
backpressure.
