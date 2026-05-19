# PLAN.md — lopi Master Plan

**Updated:** 2026-05-12 · v0.17.0 just shipped.

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

### v0.17.0 — Sprint O: GitHub App Server Scaffold 🔐
`lopi-app` crate (GitHub App OAuth + Stripe webhook, 6 tests) · `github_installations` table ·
`upsert/delete/list_installation` · per-customer store provisioned on install event ·
`lopi serve-app` CLI · SvelteKit `/onboard` page with 3-step flow + pricing ·
`store/tests.rs` split (504→190+322) · 11 new tests (419 total).

### v0.16.0 — Sprint N: Trust Calibration + Per-Customer Isolation 🎯
Trust calibration live: `compute_weight_adjustments()` async, pulls approved/rejected
pattern signal, adjusts lint+diff weights per task · `lopi trust` CLI · `MemoryStore::
open_for_customer(base_dir, customer_id)` per-tenant isolation · `store/patterns.rs`
extracted (mod.rs 557→310) · `task_commands.rs` extracted (main.rs 511→448) · 2 new tests (408 total).

### v0.15.0 — Sprint M: Continuous Loop + Multi-Repo 🔄
`quality_check_runs` table · `save_quality_run` / `load_quality_trend` / `quality_trend_delta` ·
gap-fill persists + prints trend · `lopi watch-gap-fill` Kitchen Loop daemon ·
`lopi sail --repos` multi-repo dispatch · `/api/quality/trend` endpoint · 5 new tests (405 total).

### v0.14.0 — Sprint L: Synthetic User + File Budget Fixes 🔬
`TestRunResult` parser (Cargo + pytest) · `coverage_gaps()` · `lopi gap-fill` command ·
`lopi check --fail-on-violations` CI exit code · file budget repairs (run_loop.rs 651→480,
web/mod.rs 593→372, main.rs 560→486) · `stability_runner.rs` + `postmortem_runner.rs` +
`web/handlers.rs` + `run_command.rs` extracted · 8 new tests (399 total).

### v0.13.0 — Sprint K: Spec Surface + KCQF 📋
`lopi-spec` crate (Rust + Python test extractor) · `SpecSurface::extract/save/load/top_descriptions` ·
`lopi spec` + `lopi check` CLI commands · spec injection into planning prompt (top 10 items) ·
`/api/spec` web endpoint · `serve_with_repo` · KCQF file-size gate + spec drift detection ·
28 new tests (390 total).

### v0.12.0 — Sprint J: GitHub Issue Loop 🪝
`lopi-github` crate · GitHubClient (post_comment, add_labels) · `issue_triage.rs`
Haiku classifier (Bug/Feature/Question/WontFix + confidence) · `issue.rs` handler with
background spawn_triage · `lopi serve-webhooks` CLI command · auto-queue on Bug ≥ 0.7
confidence or `lopi:fix` label · TriageConfig wired into webhook router · clap env feature ·
18 new tests (331 total).

### v0.11.0 — Sprint I: Phase 5b Second Wave
Score weights wired through pool → runner → run loop log · lesson + pattern injection into
TOON encoder (both tabular pairs and string constraints) · extract plan_streaming → claude_stream.rs ·
post-mortem also calls save_lesson(category="recovery") · api_plan lessons section ·
lopi learn annotate CLI command. 313 tests.

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

### Phase 5b — Self-improvement, second wave (residual)
- [ ] Wire `with_adaptive_retry()` into `lopi run --adaptive-retry` CLI flag
- [ ] Self-modification loop (guarded): `allow_self_modify = true` in
      config; same git isolation and PR workflow applies
- [ ] Scoring evolution: tune Score::weighted() weights based on
      user-approved vs rejected PRs — wire compute_weight_adjustments()
      to query approved patterns

### Phase 6 — Webhooks (partial ✅)
- [x] CI failure → auto-queue fix task at `Priority::High` (v0.10.0)
- [x] Issue labeled `lopi:fix` → auto-queue (v0.12.0)
- [x] Issue triage via Haiku + GitHub comment (v0.12.0)
- [x] PR review comment → feed back to agent (v0.10.0)
- [x] `lopi serve-webhooks --port 3001` — dedicated server command (v0.12.0)
- [ ] GitHub App mode for org-wide hooks (OAuth installation flow)
- [ ] HMAC verification for all event types (currently CI + issue + PR only)

### Sprint K — Spec Surface ✅ (shipped v0.13.0)
- [x] Parse test files → spec surface JSON (Rust `#[test]` + Python `def test_*`)
- [x] `lopi spec` / `lopi check` CLI
- [x] Spec injected into planning prompt (top 10 descriptions)
- [x] `/api/spec` web endpoint
- [x] KCQF file-size gate + spec drift detection in `lopi check`

### Sprint L — Synthetic User + Coverage Gap ✅ (shipped v0.14.0)
- [x] `TestRunResult` parser — cargo test + pytest output → per-test pass/fail
- [x] `coverage_gaps()` — cross-reference spec surface with test results
- [x] `lopi gap-fill` — runs tests, finds gaps, queues fix tasks via sail API
- [x] `lopi check --fail-on-violations` — CI-compatible exit code
- [x] File budget repairs — all three oversize files now under 500 lines

### Sprint M — Continuous Loop + Multi-Repo ✅ (shipped v0.15.0)
- [x] `quality_check_runs` table + CRUD in lopi-memory
- [x] `lopi gap-fill` persists quality run + prints trend delta
- [x] `lopi watch-gap-fill` — Kitchen Loop daemon (configurable interval)
- [x] `lopi sail --repos` — multi-repo concurrent dispatch
- [x] `/api/quality/trend` — trend history endpoint

### Sprint N — Trust Calibration + Per-Customer Isolation ✅ (shipped v0.16.0)
- [x] Trust calibration: `compute_weight_adjustments()` live from annotated patterns
- [x] `lopi trust` CLI — shows trust stats and current weight adjustments
- [x] `MemoryStore::open_for_customer(base_dir, customer_id)` — per-tenant isolation
- [x] `store/patterns.rs` extracted; `task_commands.rs` extracted (both files in budget)

### Sprint O — GitHub App Server Scaffold ✅ (shipped v0.17.0)
- [x] `lopi-app` crate: GitHub App OAuth routes + Stripe webhook handler
- [x] `github_installations` table + upsert/delete/list/lookup
- [x] `lopi serve-app` CLI — starts on port 3002, reads credentials from env
- [x] Per-customer store provisioned on `installation.created` webhook
- [x] SvelteKit `/onboard` page with 3-step flow and pricing table

### Sprint P — Production Deployment + Tier Gating ✅ (shipped)
- [x] `CustomerTier` enum in `lopi-core` — Free/Starter/Growth/Enterprise + `max_agents()`, `features()`, `from_stripe_name()`
- [x] `tier` column in `github_installations` — idempotent ALTER TABLE migration; `set_installation_tier()` + `customer_tier()` in `lopi-memory`
- [x] Stripe subscription handler wires `customer.subscription.{created,updated,deleted}` to tier via `lopi_installation_id` metadata
- [x] `/api/plans` endpoint — static JSON with all tier definitions (id, name, price, max_agents, features)
- [x] `LOPI_CUSTOMER_ID` tier cap in `lopi sail` — reads tier from DB at startup, caps `AgentPool` concurrency
- [x] `Dockerfile` — multi-stage build (rust:1.87-slim → debian:bookworm-slim), single binary, non-root user
- [x] `fly.toml` — fly.io deploy config: two process groups (`app` on 3002, `web` on 3000), persistent volume, health checks
- [ ] Register GitHub App on github.com (requires live domain — manual step)

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

## Researched Feature Roadmap

Discovery sweep across modern agent infrastructure (OpenTelemetry GenAI
semconv, Anthropic Agent SDK 2024–2025, Microsoft Agent Framework, MCP +
A2A specs, OpenAI structured outputs). Tiered by urgency; items ordered
within each tier by impact-per-LoC against lopi's current shape.

### 🔴 P1 — Implement now

Foundational survivability + observability. Without these, fleet
scale-up exposes lopi to runaway spend and opaque failures.

- **Cost governor + circuit breakers** — `BudgetConfig` hierarchy
  (fleet → agent → task) with pre-call enforcement in the planner and
  scorer paths. Each scope has a `CircuitBreaker` (`Closed` →
  `Open` → `HalfOpen`) tracking consecutive failures and per-window
  cost burn. Emits `AgentEvent::BudgetExceeded { scope, limit_usd,
  burned_usd }` the moment a call would breach the cap, so the UI can
  flag it before the next agent turn fires. Builds on the rate-limit
  primitives in `lopi-ratelimit`.
- **OpenTelemetry spans per agent turn** — `tracing` already runs
  workspace-wide. Add a feature-gated `otel` Cargo feature that wires
  `tracing-opentelemetry` + `opentelemetry-otlp` and emits four
  GenAI-semconv-aligned spans per turn: `lopi.agent.think`,
  `lopi.agent.act`, `lopi.agent.score`, `lopi.agent.task.complete`.
  Honors `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_SERVICE_NAME` envs.
  Off by default — zero runtime cost when the feature is disabled.
- **Durable checkpoint + resume** — Serialize `AgentState` (current
  attempt, last plan, last score, working directory, accumulated
  context hash) via `sqlx` to a new `agent_checkpoints` table before
  every action that can fail (plan, implement, score, PR). Adds
  `lopi resume --agent-id <uuid>` CLI subcommand and
  `POST /api/agents/{id}/checkpoint` endpoint to checkpoint on demand.
  Survives `lopi sail` restarts; pairs with the existing memory store
  schema migrations.
- **Structured output schema validation** — Optional
  `output_schema: Option<JsonSchema>` on `AgentSpec`. After each
  implement step, validate the generated diff metadata / score JSON
  against the schema. Counts violations in a Prometheus counter
  `lopi_schema_violations_total{agent,kind}` and reroutes failures
  through the existing adaptive-retry path with the violation message
  appended to the next plan prompt.

### 🟠 P2 — Next

Once P1 lands, lopi has the safety floor needed to enable richer
collaboration patterns.

- **✅ Tool registry** *(shipped)* — `lopi-tools` crate (tier 2) with
  `ToolSpec` + `ToolRegistry`. Atomic JSON persistence at
  `$LOPI_HOME/tool_registry.json`. `Task::tools: Vec<String>` allowlist
  on every task. REST: `GET/POST /api/tools`, `GET/DELETE
  /api/tools/:name`. 16 tests.
- **✅ Result caching** *(shipped)* — `result_cache` SQLite table keyed
  on `SHA-256(agent_id ‖ task_json)`. `MemoryStore::cache_{get,put,
  invalidate_for_agent,clear,sweep}` + `cache_stats`. Rolling-hour
  hit/miss ledger. REST: `GET /api/cache/stats`, `DELETE /api/cache`,
  `DELETE /api/cache/agent/:agent`. 14 tests.
- **✅ Constellation routing** *(shipped)* — `lopi-orchestrator::
  ConstellationRouter` with four strategies (`RoundRobin`,
  `WeightedRandom`, `LeastLoaded`, `TagMatch { required_tags }`),
  per-member atomic load counters, bounded last-hour decision log, and
  `max_concurrent` caps. REST: `GET/POST /api/constellations`,
  `POST /api/constellation/:name/dispatch`, `GET
  /api/constellation/:name/stats`. 15 tests.
- **MCP + A2A protocol support** — `McpClient` (JSON-RPC 2.0 over
  stdio + SSE transports) for tool-server discovery, and `A2AClient`
  with the published agent card so external clients can drive lopi
  via the Agent-to-Agent spec. Reuses the existing token-bucket rate
  limiter per peer.
- **Multi-tier agent memory** — Split `lopi-memory` into four address
  spaces: `working` (in-context, ephemeral), `episodic` (per-task,
  TTL-bounded), `semantic` (cross-task patterns — current
  `PatternEnricher`), and `procedural` (learned tool-call sequences).
  Background consolidation worker runs under `tokio::spawn` on a soft
  cadence. Optional `kohaku` vector-store backend behind a feature
  flag for embedding-based recall.
- **Human-in-the-loop pause points** — `require_approval: Vec<Pattern>`
  on the agent spec (regex against the proposed plan/diff). When a
  pattern matches, emit `AgentEvent::AwaitingApproval { task_id,
  reason, preview }`, suspend the runner, and expose
  `POST /api/agents/{id}/approve` + `/api/agents/{id}/reject` for
  resume. Telegram bot grows `/approve <id>` and `/reject <id>`
  commands. The Forge harbor renders awaiting agents with a distinct
  amber halo.
- **Constellation auto-scaling** — `FleetController` watches aggregate
  tokens/sec and queue depth. Above the high-water mark, spawn a new
  agent slot (subject to `BudgetConfig`); below the low-water mark,
  drain idle slots after a cooldown. Emits
  `FleetEvent::Scaled { from, to, reason }` so the UI can animate the
  new boat sailing in.

### 🟡 P3 — Later

Power tools — high leverage but require P1+P2 substrate to be useful.

- **Compile-time policy enforcement proc macro** — `#[lopi::policy]`
  on agent functions reads a TOML manifest and emits compile errors
  for capability/budget violations before the binary ships. Avoids
  runtime guard-rail drift.
- **Hierarchical agent delegation with budget slicing** — Parent agent
  can `spawn_child(spec, budget_slice)`. Child inherits parent context;
  parent's budget is debited atomically. Supports recursive
  decomposition for `lopi plan "..."` (planned in Phase 9).
- **Fleet replay + time-travel debugging** — Snapshot every
  `AgentEvent` + checkpoint into an append-only log. CLI: `lopi
  replay --task <id> --from <ts>` reconstructs the full agent state at
  any past point. Useful for post-mortem of complex multi-agent
  failures.

---

## Current Health

| Metric | Value |
|---|---|
| Workspace tests | **499 passing**, 0 failing |
| Build | `cargo build --workspace`: clean |
| Crates | **15** (+ lopi-app, lopi-github, lopi-spec) |
| CLI commands | `run`, `watch`, `tail`, `dock`, `sail [--repos]`, `cancel`, `learn list/show/export/annotate`, `schedules list`, `serve-webhooks`, `spec`, `check [--fail-on-violations]`, `gap-fill`, `watch-gap-fill`, `trust`, `serve-app` |
| API endpoints | `/api/health`, `/api/tasks` (GET+POST), `/api/tasks/:id` (GET+DELETE), `/api/stats`, `/api/patterns`, `/metrics` (Prometheus), `/sse` (SSE), `/ws` (WebSocket) |
| Embedded UI | SvelteKit Forge + Constellation, ~487 KB JS / 126 KB gzipped |
| Direct-API planning | ✅ via `AgentRunner::with_api(client, limiter, breaker)` |
| Adaptive retry | ✅ via `AgentRunner::with_adaptive_retry()` (post-mortem fires + lesson saved on terminal failure) |
| Lesson injection | ✅ patterns + lessons both TOON-encoded into planning prompt |
| Issue triage | ✅ Haiku classifier → GitHub comment → auto-queue via `lopi serve-webhooks` |
| Spec surface | ✅ `lopi-spec` crate · `lopi spec` · `lopi check` · `/api/spec` · injected into planning |
| Latest release | **v0.13.0** |

---

*KONJO — Know, Outline, Nail, Justify, Optimize.*
*Plan, build, test, ship, rest, repeat.*
*ᨀᨚᨐᨚ — build the ship. make it seaworthy.*
