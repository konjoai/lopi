# PLAN.md — lopi Master Plan

**Updated:** 2026-05-08 · v0.10.0 just shipped.

## Vision

lopi is the Konjo agent runtime. It runs Claude Code agents concurrently in
git-isolated branches, learns from every run, self-improves over time, and
is controllable from a phone. The web UI (the **Forge** + **Constellation**
in `web/`) is embedded into the binary via `rust-embed` so a single
executable ships the whole experience.

---

## Shipped (chronological)

### v0.1.0–v0.5.0 — Phases 1–4 (foundation)
Cargo workspace · core types · git isolation · agent loop · SQLite memory ·
agent pool with semaphore-bounded concurrency · TUI + axum web API · Telegram
+ WhatsApp stubs · webhooks · pattern miner · scheduled tasks · per-repo
profiles · `lopi watch --remote`.

### v0.6.0 — TOON encoder
`lopi-toon` crate · token-oriented prompt encoding · ~1.9M tokens/month
saved at 100 tasks/day.

### v0.7.0 — `lopi-context`: KV cache eviction
`ContextWindow` with three composable eviction policies (PhaseTransition,
BudgetLIFO, ExplicitTag) · tool_pair atomicity invariant · is_conclusion
preservation · token estimation via `tiktoken-rs`.

### v0.8.0 — Observability + Correctness + Systems + Resilience
`TurnMetrics` table · benchmark corpus · `mimalloc` global allocator ·
full-jitter exponential backoff · `nextest` config · `lopi-ratelimit`
crate (TokenBucket, AnthropicLimiter, CircuitBreaker) · dual-pool
MemoryStore · worktree lock · CancellationToken · structured shutdown.

### v0.7.x–v0.9.0 — UI sprints (the Forge)
- **UI-1:** SvelteKit + Three.js + custom GLSL shader · the Forge centerpiece
  with volumetric noise + fire/ice domains + Fresnel aura
- **UI-2:** real-data integration · TypeScript types mirror lopi-core ·
  defensive runtime parser (53 tests) · cross-language wire-format contract
  tests
- **UI-3:** `/constellation` · 3D orbital view · click-to-focus · trails ·
  starfield · center beacon
- **UI-3.1:** cross-agent insight lines · same-repo + same-phase + goal
  keyword overlap · phase-sync pulse animation
- **UI-4:** `rust-embed` integration · `lopi sail` ships the Forge inside
  the binary · 4-tier asset lookup (direct → .html → SPA fallback → placeholder)
- **UI-5:** keyboard shortcuts (j/k/⌘K/Esc/?) · Help overlay · Cost
  analytics panel with sparkline + top-N agents

### v0.9.0 — Sprint G: Direct Anthropic SDK planning path
`AgentRunner::with_api(client, limiter, breaker)` · `plan_via_api` replaces
the CLI subprocess for planning · prompt caching with `cache_control:
ephemeral` · real `TurnMetrics` from API responses · transparent CLI
fallback · 7 new tests.

### v0.10.0 — Sprint H: Self-Improvement Engine 🧠
- **`lopi learn`** subcommands:
  - `learn list [--postmortem-only] [--limit N]` — sorted pattern table
  - `learn show <id-prefix>` — full pattern detail
  - `learn export [--limit N]` — JSON for analytics
- **Failure post-mortem** (`runner::postmortem`) — when adaptive retry is
  enabled and all retries fail, runs a single Haiku reflection session that
  returns one imperative constraint string. Persisted to the patterns table
  with `derived_from_postmortem = 1`.
- **Adaptive retry** (`AgentRunner::with_adaptive_retry()`) — stashes the
  previous attempt's score errors as `last_error`; available for the next
  attempt's prompt. Reflexion-style.
- **Schema migration** — `patterns.derived_from_postmortem INTEGER NOT
  NULL DEFAULT 0` · idempotent ALTER TABLE handling now correctly strips
  leading SQL comments.
- **`MemoryStore::find_pattern_by_id_prefix`** + **`insert_postmortem_pattern`**
  + **`load_patterns` ordering** by COALESCE(success_rate, 0) DESC, last_seen
  DESC.
- 17 new tests (4 lopi-memory + 11 postmortem + 2 builder integration).
- Workspace total: 244 → 261 passing.

---

## Open backlog (in priority order)

### Phase 5b — Self-improvement, second wave
- [ ] Wire `with_adaptive_retry()` into `lopi run --adaptive-retry` CLI flag
- [ ] Use `last_error` in the next attempt's planning prompt (currently
      stashed but the run loop doesn't yet inject it — Sprint H1)
- [ ] Pattern annotation: user can mark a post-mortem pattern as
      "validated" / "rejected" via Telegram inline keyboard
- [ ] Self-modification loop (guarded): `allow_self_modify = true` in
      config; same git isolation and PR workflow applies
- [ ] Scoring evolution: tune Score::weighted() weights based on
      user-approved vs rejected PRs

### Phase 6 — Webhooks fully wired
- [ ] CI failure → auto-queue fix task at `Priority::High`
- [ ] Issue labeled `lopi:fix` → auto-queue
- [ ] PR review comment → feed back to agent for revision
- [ ] `lopi serve-webhooks --port 3001` — dedicated server command
- [ ] GitHub App mode for org-wide hooks
- [ ] HMAC verification for all event types (already in place for CI)

### Phase 7+ — UI polish (deferred)
- [ ] Mobile-responsive Forge degradation
- [ ] Optional ambient sound design tied to agent state
- [ ] Pattern library browser inside the Forge (read `lopi learn list`
      data via `/api/patterns`)
- [ ] Telegram notifications: "post-mortem pattern saved" with the
      derived constraint

### Phase 8 — Native mobile app
- [ ] React Native shell · push notifications via FCM/APNs · per-task
      conversation threads · voice input · Quick Actions widget

### Phase 9 — Intelligence + evolution (long-running)
- [ ] Multi-agent roles: Planner → Implementer → Reviewer
- [ ] Cross-repo awareness (read-only context from other repos)
- [ ] Goal decomposition: `lopi plan "..."` breaks into subtasks
- [ ] Embedding-based memory: store attempt summaries as vectors
- [ ] Agent-to-agent communication via lopi-memory

### Sprint I — Implementation step on direct API (large scope)
The plan path uses direct API (Sprint G). Implementation still uses the
CLI for filesystem tool access. Migrating implementation requires either
Anthropic's tool-use protocol with custom file-edit tools or a sidecar
that bridges API tool calls to filesystem ops. **Not in scope for any
near-term sprint** — the CLI is good enough.

---

## Current Health

| Metric | Value |
|---|---|
| Workspace tests | **261 passing**, 0 failing |
| Build | `cargo build --workspace`: clean |
| Crates | **11** (lopi-core, lopi-context, lopi-toon, lopi-ratelimit, lopi-git, lopi-agent, lopi-memory, lopi-orchestrator, lopi-ui, lopi-remote, lopi-webhook) |
| CLI commands | `run`, `watch`, `tail`, `dock`, `sail`, `cancel`, `learn list/show/export`, `schedules list` |
| API endpoints | `/api/health`, `/api/tasks` (GET+POST), `/api/tasks/:id` (GET+DELETE), `/api/stats`, `/api/patterns`, `/metrics` (Prometheus), `/sse` (SSE), `/ws` (WebSocket) |
| Embedded UI | SvelteKit Forge + Constellation, ~487 KB JS / 126 KB gzipped, served from `lopi-ui` via `rust-embed` |
| Direct-API planning | ✅ via `AgentRunner::with_api(client, limiter, breaker)` |
| Adaptive retry | ✅ via `AgentRunner::with_adaptive_retry()` (post-mortem auto-fires on terminal failure) |
| Latest release | **v0.10.0** |

---

*KONJO — Know, Outline, Nail, Justify, Optimize.*
*Plan, build, test, ship, rest, repeat.*
*ᨀᨚᨐᨚ — build the ship. make it seaworthy.*
