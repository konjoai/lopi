# Changelog

## [Unreleased] — Sprint U: DAG-Structured Retry + Time-Travel Replay 🕸️

### Added

**Loop Engineering — Phase 16.6 Per-run drill-down trace** (`lopi-memory`, `lopi-ui`, web, macOS)
- A **Recent Runs** panel on the Loop screen: each run expands an
  attempt-by-attempt trace — lifecycle stages (plan→implement→test→score),
  per-attempt pass%/lint/diff/tokens/cost, the verifier verdict (passed/
  confidence + gaps), and captured errors. Backed by
  `GET /api/loop-engineering/runs` + `/runs/:id`, projecting `attempts` +
  `turn_metrics` + `verifier_verdicts` (`lopi-memory/store/run_trace.rs`). The
  single-run counterpart to the aggregate Loop Health view.

**Loop Engineering — Phase 16.3 Loop Health observability + stall guard** (`lopi-agent`, `lopi-memory`, `lopi-ui`, web, macOS)
- **No-progress stall guard** — the loop halts early when the weighted score
  stops improving for `LoopConfig.no_progress_limit` consecutive attempts
  (design-doc gap #7), instead of burning the whole retry budget on a stuck
  loop (`update_no_progress_streak`, wired into `run_loop.rs`).
- **`GET /api/loop-engineering/health`** projects data the loop already persists
  (`attempts`, `turn_metrics`, `verifier_verdicts`) into one observability
  snapshot: headline KPIs (runs, attempts, success rate, verifier pass rate,
  spend, tokens), per-attempt score series, outcome distribution, token/cost
  burn (`lopi-memory/store/loop_health.rs`).
- **Loop Health view on both surfaces** — KPI tiles, sparklines (score/attempt,
  context pressure, diff size, cost burn), and an outcome-distribution bar,
  leading the Loop screen. Web composes `StatCard`+`Sparkline`; macOS composes
  `Charts.Sparkline`.

**Loop Engineering — Phase 16.5 Adaptive Strategy Escalation** (`lopi-core`, `lopi-agent`, `lopi-orchestrator`, `lopi-ui`, web, macOS)
- **The loop now climbs its own ladder.** Instead of pinning one self-prompt
  strategy for a whole run, `escalate_strategy` makes the agent apply
  progressively more cognitive scaffolding the longer a task resists a fix:
  cheap `Direct` retries first, then Reflexion → Self-Refine → Plan-Then-Act.
  `SelfPromptStrategy::escalated(base, attempt)` climbs one S-rung per failed
  attempt (capped at S4, starting from the configured base) — a pure, saturating
  function. Backed by RefineCoder (arXiv:2502.09183).
- **Runner** — `AgentRunner::with_strategy_escalation` + `effective_strategy(attempt)`;
  the adaptive-retry path now frames the failure with the *effective* strategy
  for that attempt. Loaded from `.lopi/loop.toml` in the `lopi run` CLI and the
  orchestrator pool.
- **API** — `GET /api/loop-engineering` config now carries `escalate_strategy`
  and an `escalation_ladder` (attempt → strategy preview); new
  `POST /api/loop-engineering/escalation` toggles it (persisted to `.lopi/loop.toml`).
  All loop-as-code writes now share one `persist_loop_update` helper.
- **Web + macOS** — an "Adaptive escalation" switch on the Loop screen plus a live
  per-attempt ladder (`#1 S2 → #2 S3 → #3 S4 …`).
- **Tests** — pure escalation math (`from_rank`/`escalated`, saturation +
  base-relative), runner `effective_strategy` unit tests, handler ladder test,
  two HTTP e2e tests, an `api.test.ts` case; verified live against `lopi sail`.

**Loop Engineering — Phase 16.4 Self-Prompting Strategy Engine** (`lopi-core`, `lopi-agent`, `lopi-orchestrator`, `lopi-ui`, web, macOS)
- **Direct agents to prompt *themselves*.** A new `SelfPromptStrategy` (S1–S4) is
  the highest-leverage loop lever: the text the agent feeds back into its own
  next planning step after a failed attempt. `crates/lopi-core/src/self_prompt.rs`
  implements four research-backed strategies as pure `frame(base, attempt)`
  transforms:
  - **S1 Direct** — raw failure, verbatim (legacy default; byte-identical).
  - **S2 Reflexion** — name the root cause, then try a *different* approach
    (Shinn et al. 2023).
  - **S3 Self-Refine** — critique against correctness/coverage/minimality, then
    revise (Madaan et al. 2023).
  - **S4 Plan-Then-Act** — write a numbered plan before editing (Plan-and-Solve).
- **Loop-as-code, editable from the UI.** `LoopConfig` gains a `self_prompt`
  field and a `save_to_repo` writer; the new `POST /api/loop-engineering/strategy`
  validates a tag and persists it to `.lopi/loop.toml` (422 on unknown tags).
  `GET /api/loop-engineering` now carries a `self_prompt_strategies` catalog,
  each entry with a **live preview** of the self-prompt it generates.
- **Wired live into the runner.** `AgentRunner::with_self_prompt` routes the
  adaptive-retry failure block through the chosen strategy before injecting it
  into the next planning prompt — honored by both the `lopi run` CLI path and the
  orchestrator pool, loaded from `.lopi/loop.toml`.
- **Web + macOS.** A new "Self-Prompting Strategy" panel on the Loop screen:
  a picker, strategy cards (active state), and a live self-prompt preview.
- **Tests.** Pure-function strategy tests + `save_to_repo` round-trips in
  `lopi-core`; catalog/handler tests in `lopi-ui`; three HTTP-level e2e tests
  (`web/loop_tests.rs`) covering snapshot read, persisted round-trip, and the
  422 reject path; an `api.test.ts` case for the web client. Verified against a
  live `lopi sail` server end-to-end.

**Loop Engineering — Phase 16.2b runner enforcement** (`lopi-agent`, `lopi-git`)
- The **L1–L4 autonomy ladder now changes end-of-loop behavior** — previously
  `autonomy_level` was configurable and observable but ignored by the runner.
  A new shared `AgentRunner::finalize` (`crates/lopi-agent/src/runner/finalize.rs`)
  replaces both `open_pr` call sites in `run_loop.rs` (main success + post-fix
  success) and branches on `task.autonomy_level`:
  - **L1 `report_only`** — commit to the branch, log a diff/score report, return
    `Success` with `pr_url: None`. No PR is opened.
  - **L2 `draft_pr`** (default) — open a **draft** PR (the GitHub review is the
    human gate).
  - **L3 `verified_pr`** — force the Konjo verifier on (regardless of
    `verifier_enabled`) **before** opening a normal PR.
  - **L4 `auto_merge`** — verifier must pass and the score must clear the gate,
    then open a PR and **auto-merge** (`gh pr merge --auto --squash`).
- **`GitManager`** (`crates/lopi-git/src/manager.rs`) gains `open_draft_pr` and
  `auto_merge`; PR/merge argument building is factored into pure, unit-tested
  helpers. The verifier now also runs on the post-fix success path for L3/L4.
- `run_loop.rs` was split into focused modules (`finalize`, `plan_gate`,
  `plan_steps`, `seed`, `speculative`) to stay under the 500-line file gate.

**Loop Engineering — Phase 16.2 sidebar screen** (`lopi-ui`, `web/`, `macos/`)
- **`GET /api/loop-engineering`** aggregation endpoint composes one read-only
  snapshot for the primary repo: effective `.lopi/loop.toml` (with validation),
  the L1–L4 autonomy ladder, discovered skills (`.claude/skills/*/SKILL.md`) +
  rules (`.claude/rules/*.md`), live schedules with their trust level, and the
  Konjo quality-gate catalog.
- **New Loop screen on both surfaces** (web `/loop`, macOS `Loop` nav) rendering
  that snapshot in Konjo identity: Effective Config card, the colored autonomy
  ladder, scheduled loops each with a **Trust-Level dropdown** (the one writable
  control → `POST /api/schedules/:id/autonomy`), skills, rule chips, and the
  three quality-gate walls. Built in lockstep — the web and macOS screens share
  the same payload and layout.

**Loop Engineering — Phase 16.1 backend** (`lopi-core`, `lopi-memory`, `lopi-ui`, CLI)
- **`LoopConfig` + `AutonomyLevel`** (`crates/lopi-core/src/loop_config.rs`): the
  "loop as code" schema loaded from `<repo>/.lopi/loop.toml` (autonomy level,
  intent anchor, enabled skills/rules, permission policy, no-progress + iteration
  caps, per-run budget) and the L1–L4 phased-autonomy ladder (report-only →
  draft-PR → verified-PR → auto-merge) with capability gates and `validate()`.
- **Per-schedule trust level** persisted: new `autonomy_level` column on the
  `schedules` table (idempotent migration), plumbed through `ScheduleRow` /
  `ScheduleInput` / `ScheduleSpec` → `Task`, with a `set_schedule_autonomy`
  store method and a `POST /api/schedules/:id/autonomy` endpoint for the
  forthcoming Loop Engineering Trust-Level dropdown.
- **`lopi loop validate` / `lopi loop show`** CLI: validate a repo's loop config
  in CI (non-zero exit on issues) and inspect the effective values.
- Full design + the five feature options (A–E) and the build sequence are
  written up in [`docs/LOOP_ENGINEERING.md`](docs/LOOP_ENGINEERING.md) and
  catalogued as Phase 16 in [`docs/COMPETITIVE_ROADMAP.md`](docs/COMPETITIVE_ROADMAP.md).

**Forge multi-agent cockpit — web + macOS** (`web/`, `macos/`)
- **Sessions sidebar** lists every task whether mounted or not. Closing a pane
  now *parks* the session in the sidebar instead of deleting it; a dedicated
  trash action is the only permanent delete.
- **Deleted-session resurrection bug fixed.** Closing a pane and deleting a
  session were conflated, and a best-effort server `DELETE` left the snapshot
  free to re-hydrate "deleted" sessions on reload. A new layout layer
  (`web/src/lib/stores/layout*.ts`, `macos/.../Store/PaneLayout.swift`)
  separates close-pane from delete-session and tombstones deletions so the
  snapshot reducer can never bring them back. A persisted "known" set tells a
  genuinely-new task apart from a returning one, so fresh tasks still auto-open.
- **Resizable auto-tiling pane grid** (default 4): 2 = halves, 3 = thirds,
  4 = quarters, with drag-resizable column/row gutters and drag-to-reorder.
- **Model / effort / priority / repo / branch selectors** via custom Konjo
  dropdowns, persisted and wired into task submission as planning constraints.
- **macOS native Forge**: a `Canvas`-based ever-morphing fire/ice **orb**
  driven by phase/activity/pressure, the resizable grid, sidebar, and selectors
  — reaching parity with the web Forge. New `Forge` nav section is now the
  landing screen. (macOS is compile-unverified in this CI environment.)
- Pure layout algorithms are unit-tested (`layout-core.test.ts`, 32 cases).
- The `AgentEvent → AgentState` reducer is split out of `agents.ts` into
  `stores/agentReducer.ts`, bringing `agents.ts` back under the 500-line gate.

### Changed

**Sessions sidebar — drag-into-pane, filter, status grouping** (`web/.../SessionSidebar.svelte`)
- **Drag a session row directly onto a specific pane** to mount it there (real
  HTML5 DnD via `application/x-lopi-session`); the new `mountInPane` layout
  action removes it from any slot it already held, so dragging never
  duplicates a pane. Clicking a row still drops it into the first free pane.
- **Filter box** — case-insensitive match across goal / repo / branch, with a
  clear button and a "no matches" empty state.
- **Status grouping** — sessions split into sticky `active` / `done` / `failed`
  headers (newest-first within each, empty groups hidden). Pure, testable logic
  lives in `session-groups.ts` (**16 tests**); the component stays a renderer.

**Reducer test coverage** (`web/.../agentReducer.test.ts`)
- The `AgentEvent → AgentState` reducer (split into `agentReducer.ts`) shipped
  without tests. Added **28 cases** covering every variant —
  queue/start/turn-metrics/status/score-clamp/completed/verdict transitions,
  events for unknown tasks (no-op), and immutability (input map + agent never
  mutated). The extraction is what made this testable in isolation.

**Springy, interruptible tile motion** (`web/.../TileGrid.svelte`)
- Adding or removing a pane was instant. Now the surviving tiles **glide** to
  their new tracks (FLIP, 420ms `cubicOut`) while the added/removed tile
  **scales** in/out (`backOut` pop on enter). The cell list is keyed and never
  changes during a gutter drag, so the spring can't fight a live resize. The
  divider gutters ease to their new boundaries on re-flow and snap instantly
  while dragging. 60fps, interruptible, no layout thrash.
- **macOS parity** (`PaneGridView.swift`): the native grid gets the same
  behaviour via `.animation(.spring(response:0.42, dampingFraction:0.82),
  value: count)` plus a scale+opacity pane transition — keyed on `count` so a
  gutter drag never fights the spring.

### Fixed

**Forge panes never went live — reactivity bug** (`web/.../AgentGrid.svelte`)
- Panes resolved their agent through a helper called in markup
  (`agent={agentFor(index)}`). Svelte tracks an expression's dependencies
  *syntactically* — it sees `agentFor` and `index`, never the `$agents` /
  `$paneSlots` stores read **inside** the function — so the grid evaluated
  once at mount (agents still empty; mock/live data arrives ~1.5s later) and
  then froze on the idle state forever. Every pane showed "— idle —" with an
  empty ring even though the sessions sidebar (which iterates `$agents`
  directly) correctly listed every running agent, and the layout had already
  mounted them into slots. Replaced the helper with a reactive
  `$: paneAgents = $paneSlots.map(...)` derivation that names both stores, so
  panes now light up the moment an agent appears. This is what makes the Forge
  actually *live* — orbs, metrics, logs and phase all render on first paint.

**`AgentDag` execution trace** (`crates/lopi-agent/src/dag.rs`)
- Models one agent attempt as a directed acyclic graph of pipeline stages —
  `NodeKind = Plan | Implement | Test | Score | Verify | Diff | Pr`, each a
  `DagNode { kind, status, depends_on, output_hash }`.
- `canonical()` builds the linear pipeline; `resume_point()` returns the
  earliest non-`Done` node (the partial-restart entry point); `reset_from()`
  rewinds a node + downstream while preserving upstream memoized output;
  `complete_node()` / `fail_node()` / `set_status()` drive transitions;
  `edges()` exposes the graph; full serde round-trip.
- Grounded in the Scheduler-Theoretic Framework (arXiv 2604.11378): partial
  restart from failed nodes beats linear retry. 14 unit tests.

**Idempotency safeguard** (`dag.rs`) — *discovery-driven*
- `NodeKind::is_side_effecting()` (`Pr` opens a PR); `DagNode.idempotency_key`
  records the committed external effect and is **preserved across
  `reset_from`** (unlike `output_hash`); `should_execute()` skips an
  already-committed side-effecting node so a replay reuses the effect instead
  of opening a duplicate PR. Grounded in ACRFence (arXiv 2603.20625) on
  semantic-rollback hazards in agent retry.

**`agent_dag_nodes` persistence** (`lopi-memory`)
- One row per pipeline stage; `upsert_dag_node` (upsert on `(task_id, kind)`)
  + `load_dag_nodes`. Edges are derived from `depends_on`, so no redundant
  edges table. 3 tests.

**`GET /api/agents/:id/dag`** (`lopi-ui`)
- Returns `{ task_id, nodes, edges }`; edges derived from each node's
  `depends_on`. Unknown task → empty graph (200). 2 tests on the graph shaper.

**`AgentDag::from_rows` + `lopi replay`**
- `AgentDag::from_rows` (`lopi-agent/dag_rows.rs`) reconstructs a DAG from
  persisted `agent_dag_nodes`; `NodeKind` / `NodeStatus` gain `FromStr`.
- `lopi replay --task <id> [--from <node>] [--dry-run]` loads the persisted
  DAG, resolves the restart stage (explicit `--from` or the resume point), and
  prints the partial-restart plan — which stages re-run, which reuse memoized
  upstream output (♻️), and which side-effecting stages are skipped because
  their external effect already landed (⏭️, idempotency-key reuse). Read-only
  for now; live re-execution rides on the runner producer. 7 tests.

**Mutation gate** (`.cargo/mutants.toml`)
- New cargo-mutants config scoped-excluding the CLI entry point (`main`) and
  two pure-IO shells (`replay_commands::run` / `print_plan`) — they hold no
  branching logic, delegating to the unit-tested `replay_plan` / `classify` /
  `resolve_restart`. The replay plan computation is fully mutation-covered.

### Notes
- The runner producer (wiring `AgentRunner` to build/persist the DAG and emit
  `node_id` on events) and the TUI "DAG" tab follow — the producer requires
  splitting the 606-line `run_loop.rs` and live-agent validation, so it is held
  for an environment that can exercise a real run. See PLAN.md Sprint U.

---

## [Unreleased] — Sprint T: Topology-Adaptive Routing + Q-Learning 🧭

### Added

**Q-learning router** (`crates/lopi-orchestrator/src/q_router.rs`)
- `QRouter` — an epsilon-greedy contextual-bandit router over a
  `(task_type → agent_config)` Q-table. `select` explores with probability ε
  (default 0.1) and otherwise exploits the highest-valued action; `update`
  folds a normalised reward in via `Q ← Q + α·(reward − Q)` (default α = 0.5).
- `snapshot` / `hydrate` round-trip the table for persistence; `q_value`
  exposes a single cell. All inputs (ε, α, reward) are clamped to `[0, 1]`.
- 9 unit tests (update math, clamping, greedy + explore selection,
  snapshot↔hydrate, param clamping).

**`routing_q_values` table** (`lopi-memory`)
- `MemoryStore::upsert_q_value` (upsert keyed on the `(state, action)` PK) and
  `load_q_table` (most-recently-updated first). 3 tests.

**`GET /api/routing/q-values`** (`lopi-ui`)
- Returns the persisted Q-table as JSON for inspection.

**Topology classifier corpus** (`lopi-orchestrator::topology`)
- Expanded to a 30-case labelled corpus spanning all four topologies plus the
  hybrid/tie fallback.

**`Strategy::QLearned` in the constellation router** (`lopi-orchestrator::constellation`)
- New routing strategy: dispatch selects the member with the best learned
  Q-value for the constellation (state = constellation name, action = agent id),
  exploring epsilon-greedily via the shared `QRouter`.
- `ConstellationRouter::record_outcome(constellation, agent_id, reward)` feeds a
  task's quality reward back into the Q-table; `q_snapshot()` exposes it.
- `constellation.rs` (690 lines, over the 500 budget) split into
  `constellation/{mod,types,select,tests}.rs` — each well under 300 — to clear
  the file-size gate before the feature landed. Behaviour preserved; 4 new tests.

**Topology classifier wired into the dispatch path** (`lopi-orchestrator::pool`)
- `AgentPool::submit()` now fills in `Task::topology` via the keyword classifier
  when the task carries no explicit hint — logged, advisory, and never blocks
  submission. The hint flows through to the runner via `Task::topology`.
- `effective_topology(&task)` helper (explicit hint, else classify the goal).
- `pool.rs` (929 lines, well over budget) split into
  `pool/{mod,types,registry,run_loop,tests}.rs` — each ≤ 354 lines — to clear
  the file-size gate. Behaviour preserved; public API (`crate::pool::*`)
  unchanged. 3 new tests.

### Notes
- Remaining Sprint T work (`AgentPool::dispatch` topology branching, the
  `low_confidence` Haiku fallback, task-type-keyed Q-state, and the RoundRobin
  benchmark) is deferred — `pool.rs` is also over the size budget and touches
  the live agent-spawn loop. See PLAN.md Sprint T.

---

## [0.19.0] — Sprint S: Konjo Verifier + macOS app + web overhaul 🔬🖥️

### Added — Konjo Verifier (Sprint S)

**`VerifierAgent`** (`crates/lopi-agent/src/verifier.rs`)
- Rubric-guided Opus second-score pass. After the heuristic `Score` passes,
  `run_verifier_pass` sends `{goal, plan, diff, test_output, rubric}` to Opus and
  parses a `VerifierVerdict { passed, gaps, fix_hints, confidence }`.
- On rejection, `fix_hints` are appended to `Task::constraints` and the task
  retries with them as hard requirements. Verifier errors are non-fatal (the
  runner proceeds, `tracing::warn!`).
- **Rubric resolution chain:** `Task::rubric` →
  `.konjo/rubrics/feature_completeness.toml` (via `verifier::resolve_rubric` /
  `load_rubric_file`) → `default_rubric()`. `Rubric::from_toml_str` in
  `lopi-core` keeps the parse IO-free; the runner reads the file with `tokio::fs`.

**Persistence + events**
- `verifier_verdicts` SQLite table + `save_verifier_verdict` / `load_verifier_verdicts`
  (`lopi-memory/src/store/verifier.rs`).
- `AgentEvent::VerifierVerdict { task_id, passed, gaps, fix_hints, confidence }`
  on the event bus.

**Surfacing**
- Web: Pulse feed + Router tab render verifier (and budget) events.
- macOS: live cockpit cognition viz includes verdicts.
- Telegram: `/dock` rows carry a 🔬✅ / 🔬❌ marker for the latest verdict.

**Rubrics + docs**
- Three canonical rubrics in `.konjo/rubrics/`: `feature_completeness.toml`,
  `refactor_safety.toml`, `security_audit.toml`.
- `KONJO_VERIFIER.md` documents the rubric format, the resolution chain, and the
  brand position ("the only orchestrator that grades its own work before a PR").
- Sprint S1: Konjo CLI/TUI overhaul — REPL, slash commands, bypass mode.

### Added — Native macOS app

- SwiftUI dashboard in `macos/`: scaffold (Phases 1–2 + Cron), all admin panels
  (Phase 5), live cockpit with real-time cognition visualisation and Konjo motion.
- Durable cron schedules + config REST API (macOS Phase 0).

### Added — Web UI (the Forge) OpenClaw-parity overhaul

- New tabs: `pulse`, `router`, `logs`, `debug`, `config`; reactive orb with
  colored reactions; global logs API; Tools tab.
- Live SSE log tail in the Tasks drawer + quality-trend sparkline.
- `web/mod.rs` split into static + middleware modules to hold the file-size gate.

### Tests
- Verifier resolution chain + parse tests (`lopi-core`, `lopi-agent`).
- Workspace total: **631 passing**, 0 failing.

---

## [Unreleased] — Sprint R: Telegram Bot Overhaul ⛵️

### Added

**Full remote control surface** (`crates/lopi-remote/src/telegram/`)
- Rewrote `telegram.rs` as a module (`telegram/mod.rs`, `handlers.rs`, `monitor.rs`, `callbacks.rs`, `notify.rs`, `format.rs`) — all files under 400 lines
- 19 commands: `/task`, `/urgent`, `/critical`, `/status`, `/fleet`, `/dock`, `/cancel`, `/retry`, `/schedules`, `/run`, `/tail`, `/learn`, `/patterns`, `/approve`, `/cost`, `/draft`, `/submit`, `/cancel_draft`, `/help`

**Completion notifications** (`notify.rs`)
- `notify_loop` subscribes to `EventBus<AgentEvent>` and pushes Telegram messages on `TaskStarted` (attempt 1 only), `StatusChanged` (Implementing/Testing only), `ScoreUpdated` (score ≥ 0.75), `TaskCompleted` (always), `TaskCancelled`, `BudgetExceeded`
- PR URLs sent as **separate follow-up messages** for one-tap copyability
- Goal cached from `TaskQueued` events — completion messages include the task goal
- Suppresses `TurnMetrics`, `LogLine`, `PoolStats` — zero noise

**New monitoring commands** (`monitor.rs`)
- `/fleet` — running agents + queued tasks + pool stats + today's token cost, with [Refresh] [Dock] inline buttons
- `/dock [N]` — last N tasks (default 8, max 20) with status emoji and relative timestamps
- `/tail <id> [N]` — last N log lines for a task (default 10, max 30)
- `/cost` — today's tokens/cost + all-time task count + budget limits
- `/schedules` — all configured cron entries with next fire time
- `/run <name>` — trigger a named schedule immediately

**New task commands** (`handlers.rs`)
- `/critical <goal>` — critical priority queue at front
- `/cancel <id>` — sends cancel signal to running agent via `pool.cancel_by_prefix()`
- `/retry <id>` — looks up failed task by ID prefix, requeues at HIGH priority

**Draft mode** (`handlers.rs`)
- `/draft` — enter multi-line input mode; each plain-text message appends a line
- `/submit` — joins draft lines and queues as a task
- `/cancel_draft` — discard current draft

**Formatting helpers** (`format.rs`)
- `short_id()`, `priority_badge()`, `status_emoji()`, `relative_time()`, `format_uptime()` — consistent display across all commands

**`lopi-orchestrator` additions**
- `AgentPool::running_agents() -> Vec<RunningAgentInfo>` — lock-free snapshot for fleet display
- `AgentPool::cancel_by_prefix(prefix)` — cancel by ID prefix without needing the full UUID
- `TaskQueue::peek_queued() -> Vec<(Priority, String)>` — priority-sorted snapshot for fleet display
- `RunningAgentInfo` struct exported from `lopi-orchestrator`

**`sail_commands.rs`** — `spawn_telegram()` now passes `pool`, `bus`, `schedules`, and `notify_chat_id` through to `telegram::run()`

### Tests
- `format.rs`: 10 tests (short_id, priority_badge, all status_emoji variants, relative_time suite)
- `notify.rs`: 4 tests (success/no-PR/failed completion messages, budget exceeded format)
- `handlers.rs`: 4 tests (dock N parsing, tail arg parsing, auth check logic)
- `monitor.rs`: 4 tests (tail arg parsing, schedule name trim)
- **22 new tests**. Workspace: 499 → **571 passing**, 0 failing.

---

## [Unreleased] — Sprint P: Production Deployment + Tier Gating 🚀

### Added

**`CustomerTier` enum** (`lopi-core::tier`)
- `Free | Starter | Growth | Enterprise` variants with `max_agents()`, `display_name()`, `price_usd_cents_per_month()`, `features()`, `from_stripe_name()`.
- `Display` + `FromStr` round-trip; full `serde` support for wire serialization.
- 6 unit tests: serde round-trip, max_agents, from_stripe_name, display, price ordering.

**Tier column in `github_installations`** (`lopi-memory`)
- Idempotent `ALTER TABLE … ADD COLUMN tier TEXT NOT NULL DEFAULT 'free'` migration.
- `MemoryStore::set_installation_tier(installation_id, tier)` — updates tier on subscription event.
- `MemoryStore::customer_tier(customer_id)` — reads active installation tier; defaults to `Free` when absent.
- 3 new tests: set/get tier, unknown customer defaults to Free, upgrade+downgrade cycle.

**Stripe subscription → tier wiring** (`lopi-app::stripe`)
- `customer.subscription.created` / `customer.subscription.updated` — extracts tier from `items[0].price.nickname` or `metadata.lopi_plan`; reads `metadata.lopi_installation_id` to update the correct installation row.
- `customer.subscription.deleted` — downgrades tier to `Free`.
- `extract_tier_from_subscription()` + `extract_installation_id()` helpers — no stub logic remaining.

**`GET /api/plans`** (`lopi-ui::web`)
- Returns a static JSON array of all four `CustomerTier` variants with `id`, `name`, `price_usd_per_month`, `max_agents`, `features`.
- 2 endpoint integration tests: response shape + field presence.

**Tier-aware `max_agents` cap in `lopi sail`** (`src/sail_commands.rs`)
- `tier_capped_max_agents()` reads `LOPI_CUSTOMER_ID` env var, queries the DB for the customer's tier, and caps the requested `--max-agents` to `tier.max_agents()`.
- Logs a tracing `info!` event when the cap is applied. Falls back gracefully on DB error or absent env var.

**`Dockerfile`** (repo root)
- Multi-stage build: `rust:1.87-slim-bookworm` build stage → `debian:bookworm-slim` runtime.
- Non-root `lopi` user; persistent `VOLUME ["/home/lopi/.lopi"]`; `EXPOSE 3000 3002`.
- Dependency-layer caching via manifest-only pre-build stub.

**`fly.toml`** (repo root)
- Two process groups: `app` (`lopi serve-app` on 3002) and `web` (`lopi sail` on 3000).
- Persistent `lopi_data` volume mounted at `/home/lopi/.lopi`.
- HTTP health checks on `/` (app) and `/api/health` (web); TLS + HTTP on 443/80.

### Tests
- 6 `lopi-core::tier` tests + 3 `lopi-memory::installations` tests + 2 `lopi-ui::web` tests = **11 new**.
- Workspace: 488 → **499 passing**, 0 failing.

---

## [Unreleased] — P1 Agent Survivability Sprint 🚦

### Added

**P1.1 — Cost governor + circuit breakers** (`lopi-core::BudgetScope`,
`lopi-ratelimit::budget`)
- Three-tier hierarchical budget enforcement: `Fleet` → `Agent` → `Task`.
- `BudgetGovernor` wraps three `Arc<CircuitBreaker>`. `check()` walks the
  breakers innermost-first and returns the tightest enclosing scope that
  refuses, so the runner can attribute the failure correctly.
- `record_success(cost)` / `record_failure()` / `record_cost_only(cost)`
  feed each scope. `states()` returns a snapshot for `/metrics` exposition.
- `BudgetError::Exceeded { scope, limit_usd }` vs.
  `BudgetError::BreakerOpen { scope }` — distinguishes "hourly cap reached"
  from "too many consecutive failures".
- `BudgetConfig` defaults: $25/hr fleet · $5/hr agent · $1.50/hr task.
- New `AgentEvent::BudgetExceeded { task_id, scope, limit_usd, burned_usd }`
  — runner emits this the moment `check()` refuses, so the Forge UI can
  flag the breach before the next turn fires.

**P1.2 — OpenTelemetry spans behind `otel` Cargo feature** (root crate)
- Workspace deps `opentelemetry` · `opentelemetry_sdk` ·
  `opentelemetry-otlp` · `tracing-opentelemetry` are now `optional = true`
  and gated by `otel = ["dep:…"]`.
- Four GenAI-semconv-aligned spans wrap each agent turn:
  `lopi.agent.think` (planning step) · `lopi.agent.act` (`claude.implement`) ·
  `lopi.agent.score` (`scorer.score`) · `lopi.agent.task.complete` (terminal
  success return).
- Wrapped with `.instrument(span)` so the runner's outer future stays
  `Send` and the pool's `JoinSet::spawn` accepts it.
- `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_SERVICE_NAME` env vars honoured.
- Zero OTel runtime cost when the feature is off.

**P1.3 — Durable checkpoint + resume** (`lopi-memory::CheckpointRow` +
`lopi resume` + `POST /api/agents/:id/checkpoint`)
- New `agent_checkpoints` table with `idx_checkpoints_task_created` index.
- `CheckpointInput` builder · `MemoryStore::save_checkpoint` ·
  `latest_checkpoint` · `list_checkpoints`.
- `lopi resume --agent-id <uuid>` CLI subcommand loads the most-recent
  checkpoint and prints a human-readable summary (attempt, state, repo,
  hash, plan preview, score).
- `POST /api/agents/:id/checkpoint` accepts a JSON body
  `{state, attempt?, last_plan?, last_score?, repo_path?, context_hash?}`
  and persists it. Returns 201 with `{checkpoint_id, task_id}` or 400 for
  a non-UUID id. Sits behind Bearer auth + per-IP rate limiting.

**P1.4 — Structured output schema validation** (`lopi-core::schema`)
- Optional `Task::output_schema: Option<serde_json::Value>`. When present,
  the runner validates the scorer's JSON projection against it after each
  attempt.
- Pragmatic JSON Schema subset (`type`, `required`, `properties`, `enum`)
  — dep-free beyond `serde_json` to keep `lopi-core` at tier 1. Unknown
  keywords are permissive (ignored, not rejected).
- Process-wide `lopi_schema_violations_total{kind=…}` counter exposed via
  `/metrics`. Labels: `type`, `required`, `enum`, `property`.
- On validation failure: increments counter, warns the bus, stashes the
  violation summary as `last_error` so the next planning prompt sees it
  (via adaptive retry), rolls back git, and retries.

### Documentation

- **`PLAN.md`** — new "Researched Feature Roadmap" section: P1/P2/P3
  tiers covering MCP+A2A, multi-tier memory, human-in-the-loop pause
  points, constellation auto-scaling, compile-time policy proc macro,
  hierarchical agent delegation, and fleet replay.

### Tests

- `lopi-core::budget` — 2 unit tests (scope wire-string + JSON round-trip).
- `lopi-ratelimit::budget` — 6 governor unit tests.
- `lopi-memory::checkpoints` — 4 store unit tests.
- `lopi-ui::web::tests` — 2 endpoint integration tests for checkpoint route.
- `lopi-core::schema` — 10 validator unit tests including realistic score
  schema and counter increment.

### Architecture

- `.konjo/arch.toml` layer rules honoured: `BudgetScope` (tier 1) lives in
  `lopi-core`; `BudgetGovernor` and underlying `CircuitBreaker` (tier 2)
  live in `lopi-ratelimit`. No upward dependency.

## [0.17.0] — Sprint O: GitHub App Server Scaffold 🔐

### Added

**`crates/lopi-app/`** — new crate: GitHub App OAuth + Stripe webhook server
- `AppConfig::from_env()` — loads `GITHUB_APP_ID`, `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_REDIRECT_URI`, `GITHUB_WEBHOOK_SECRET`, `STRIPE_WEBHOOK_SECRET` at startup; gracefully degrades when absent
- `GET /app/install` — redirects to GitHub App installation page
- `GET /app/callback` — exchanges OAuth code for access token; stub for customer record creation
- `POST /app/webhook` — HMAC-verified GitHub App installation events; on `created`: upserts installation, provisions per-customer `MemoryStore`; on `deleted`: marks installation inactive
- `POST /stripe/webhook` — HMAC-SHA256 + timestamp replay protection (300s window); dispatches on `customer.subscription.{created,updated,deleted}`
- 6 unit tests (HMAC validation for both GitHub and Stripe)

**`crates/lopi-memory/src/store/installations.rs`** — GitHub App installation ledger
- `github_installations` table: `installation_id`, `customer_id`, `account_login`, `account_type`, `status`, timestamps
- `upsert_installation(id, login, type)` — idempotent; handles reinstalls
- `delete_installation(id)` — marks as `'deleted'`
- `customer_for_installation(id)` — lookup by installation_id (active only)
- `list_installations()` — all active installations
- `sanitise_customer_id(login)` — lowercase, alphanumeric + hyphen only
- 5 unit tests: install/delete/reinstall/list/sanitise

**`lopi serve-app` CLI command** — start the lopi-app server
- `lopi serve-app [--port 3002] [--host …]`
- Prints credential status at startup: `✅ configured` or `⚠️ missing` per service
- Provisions `MemoryStore` from the shared `db_path()`

**`web/src/routes/onboard/+page.svelte`** — customer onboarding page
- 3-step install flow: install App → `lopi spec --save` → `lopi watch-gap-fill`
- "Install GitHub App" button → `lopi serve-app` install endpoint
- Pricing table: Starter $299/mo · Growth $999/mo · Enterprise $4,999/mo

### Fixed — File budget
**`store/tests.rs`** (504 lines) split into `tests.rs` (190) + `tests_extra.rs` (322)

### Tests
- 5 installations + 6 lopi-app tests (11 new)
- Workspace: 408 → **419 passing**, 0 failing. 0 clippy warnings.

---

## [0.16.0] — Sprint N: Trust Calibration + Per-Customer Isolation 🎯

### Added

**Trust calibration — `compute_weight_adjustments()` is now live**
- `crates/lopi-orchestrator/src/pool.rs`: `compute_weight_adjustments()` is now `async` and actually calls `store.compute_weight_adjustments()` — pulling score weights from annotated pattern history on every task dispatch
- Approved patterns that needed fewer attempts tighten lint/diff penalties; rejected patterns loosen them. Signal clamped to [-2.0, 2.0] × 0.005 → delta applied to weights
- Falls back to defaults gracefully when no annotations exist or the store is absent

**`lopi trust` CLI command** (`src/trust_commands.rs`)
- Shows approved vs rejected pattern counts and avg-attempt stats
- Prints current score weight adjustments (live from the DB)
- Gives direction signal: "tightening / loosening / balanced"

**`MemoryStore::open_for_customer(base_dir, customer_id)`** — per-customer isolated store
- Creates `{base_dir}/{customer_id}/lopi.db` — one SQLite file per tenant
- Sanitises `customer_id`: only `[A-Za-z0-9-_]` allowed; unsafe chars become `_`
- 2 integration tests: isolation verified by cross-store task count, path traversal sanitised

**`crates/lopi-memory/src/store/patterns.rs`** — extracted from mod.rs
- All pattern operations: `jaccard_similarity`, `keyword_fingerprint`, `find_similar_patterns`, `load_patterns`, `find_pattern_by_id_prefix`, `insert_postmortem_pattern`, `mine_patterns`, `annotate_pattern`, `load_annotated_patterns`, `compute_weight_adjustments`
- `PatternRow` struct moved here
- store/mod.rs: 557 → **310 lines** ✅

**`src/task_commands.rs`** — Watch/Tail/Dock/Cancel extracted from main.rs
- main.rs: 511 → **448 lines** ✅

### Architecture notes

Trust calibration closes the learning loop: the human annotates patterns → weights adjust → agent gets scored differently on next attempt → better patterns get approved. Over 50–200 annotated patterns, the weights converge to reflect what this specific human values. Per-customer store isolation is the SaaS tenancy primitive — each customer's pattern history, lessons, and quality runs are fully separated.

### Tests
- 2 new per-customer store isolation tests
- Workspace: 405 → **408 passing**, 0 failing. 0 clippy warnings.

---

## [0.15.0] — Sprint M: Continuous Loop + Multi-Repo 🔄

### Added

**`crates/lopi-memory/src/store/quality.rs`** — quality check run ledger
- `quality_check_runs` table: `spec_items`, `passing`, `failing`, `gaps`, `score`, `run_at`
- `MemoryStore::save_quality_run(QualityRunRecord)` — persist one run with auto-computed score
- `MemoryStore::load_quality_trend(repo_path, limit)` — fetch runs ordered by `run_at DESC`
- `MemoryStore::quality_trend_delta(repo_path)` — (latest_score, prev_score) pair for trend arrow
- `QualityRunRow::improved_vs(&prev)` — boolean trend comparison
- 5 unit tests

**`lopi gap-fill` — now persists quality data + prints trend**
- After each run: saves a `QualityRunRow` to SQLite via `save_quality_run()`
- Loads previous run and prints coverage trend: `coverage: 82% ↑ (was 76%)`
- Returns `QualitySnapshot` so the daemon loop can log without re-querying
- New `quiet: bool` param — suppresses output when called from the daemon

**`lopi watch-gap-fill` — Kitchen Loop daemon**
- `lopi watch-gap-fill [--repo .] [--interval 60] [--sail-url ...] [--run-now]`
- Runs gap-fill every N minutes (default 60), persisting results and queuing fix tasks
- `--run-now`: triggers one immediate run before the loop starts
- Ctrl-C cleanly exits the loop

**`lopi sail --repos` — multi-repo mode**
- `--repos repo1,repo2,…` — additional repo paths alongside the primary `--repo`
- Each extra repo gets its own `AgentPool` dispatch loop sharing the shared queue and bus
- Pool already routes by `task.repo_path` — multi-repo just adds parallel dispatch
- Banner prints all repos at startup

**`/api/quality/trend`** — quality trend web endpoint
- `GET /api/quality/trend?repo=<path>&limit=<n>` — returns quality check run history
- Falls back to `AppState.repo_path` when `repo` query param is absent

### Architecture notes

The `watch-gap-fill` daemon is the mechanical basis of the Kitchen Loop. Each iteration runs the full spec → test → gap detection → queue pipeline. As fix tasks complete and get merged, the next iteration finds fewer gaps — driving the autonomous quality ratchet. The SQLite trend table makes the improvement measurable rather than impressionistic.

Multi-repo dispatch works because `task.repo_path` is already a field on `Task` and the pool already routes on it. Adding `--repos` spawns parallel dispatch goroutines, each bound to one repo path. No new queue needed.

### Tests
- 5 new quality.rs tests + 2 gap_fill_commands snapshot tests
- Workspace: 399 → **405 passing**, 0 failing. 0 clippy warnings.

---

## [0.14.0] — Sprint L: Synthetic User + File Budget Fixes 🔬

### Added

**`lopi-spec/src/test_runner.rs`** — test run parser
- `run_tests(repo_path)` — auto-detects `cargo test` vs `pytest`, runs with `--no-fail-fast`, captures pass/fail per test name
- `parse_cargo_output(output)` — parses `test name ... ok/FAILED` lines into `Vec<TestRunResult>`
- `parse_pytest_output(output)` — parses `file::test_name PASSED/FAILED` lines
- `coverage_gaps(spec_items, results)` — returns spec items with no passing run (failing tests + never-ran tests)
- `TestRunResult { name, passed, error }` — serialisable result record
- 8 unit tests (cargo format, pytest format, gap detection)

**`src/gap_fill_commands.rs`** — `lopi gap-fill`
- Loads spec surface (cached or live) → runs tests → computes coverage gaps → queues fix tasks via `POST /api/tasks` on a running `lopi sail` server
- `--dry-run`: reports gaps without queuing
- `--sail-url`: configurable target (default `http://127.0.0.1:3000`)

**`lopi check --fail-on-violations`** — CI-compatible exit code
- Exits with `std::process::exit(1)` when file-size or spec-drift violations are found
- Zero means clean; non-zero blocks CI pipeline

### Fixed — File Budget Violations (all three files were > 500 lines)

**`crates/lopi-agent/src/runner/run_loop.rs`**: 651 → 480 lines
- Extracted `run_stability_preflight` + `save_stability_ledger_entry` → new `stability_runner.rs`
- Extracted `run_postmortem_if_configured` + `persist_postmortem_outcome` → new `postmortem_runner.rs`
- Moved `status()` + `emit_turn_metrics()` to `mod.rs` (always-available utilities)

**`crates/lopi-ui/src/web/mod.rs`**: 593 → 372 lines
- Extracted all 9 route handlers → new `web/handlers.rs`
- `types` module promoted to `pub(crate)` for cross-file access

**`src/main.rs`**: 560 → 486 lines
- Extracted `Commands::Run` (97-line agent loop) → new `src/run_command.rs`
- `is_self_modify_attempt`, `status_label` promoted to `pub(crate)`

### Tests
- 8 new `lopi-spec::test_runner` tests
- Workspace: 390 → **399 passing**, 0 failing
- 0 clippy warnings

---

## [0.13.0] — Sprint K: Spec Surface + KCQF 📋

### Added

**`crates/lopi-spec`** — new crate: spec surface extractor
- `SpecSurface::extract(repo_path)` — walks all `.rs` and `.py` files, extracts test function names and doc comments
- **Rust** (`rust_extractor.rs`): `#[test]`, `#[tokio::test]`, `#[async_std::test]`, `#[rstest]`, `#[proptest]`; captures preceding `///` doc comments as description
- **Python** (`python_extractor.rs`): `def test_*` and `async def test_*`; captures inline docstring as description
- `SpecSurface::save(repo)` — writes `.lopi/spec_surface.json` as a cacheable baseline
- `SpecSurface::load(repo)` — loads cached surface (returns `None` when not yet saved)
- `SpecSurface::top_descriptions(n)` — returns top N items as TOON-ready strings
- `SpecItem { name, description, kind, file, line }` · `SpecKind: RustTest | PythonTest`
- 24 unit tests across `lib.rs`, `rust_extractor.rs`, `python_extractor.rs`

**`src/spec_commands.rs`** — two new CLI commands
- `lopi spec [--repo .] [--export] [--save]` — extract + display spec surface as a table, optionally cache to `.lopi/spec_surface.json`
- `lopi check [--repo .]` — KCQF quality analysis:
  - File-size gate: reports any `.rs` / `.py` file > 500 lines (with path + line count)
  - Spec drift gate: compares live extraction against the cached baseline; lists newly removed tests as regression risks
- 4 unit tests in `spec_commands.rs` (size violations, target-skip, clean pass)

**Spec surface injection into planning** (`lopi-agent/src/runner/run_loop.rs`)
- At each run, loads `.lopi/spec_surface.json` if present; injects top 10 items as additional constraints in the planning prompt alongside patterns and lessons
- Log line: `📋 spec surface: N items loaded`

**`/api/spec` web endpoint** (`lopi-ui/src/web/mod.rs`)
- `GET /api/spec` — returns cached spec surface or runs live extraction; JSON with `count`, `rust_files_scanned`, `python_files_scanned`, `extracted_at`, `items`
- `AppState::new_with_repo(...)` — new variant that records `repo_path` for spec serving
- `serve_with_repo(...)` — new variant of `serve()` that passes repo_path into AppState; called from `sail_commands::run()` so the spec API reflects the actual sailed repo

### Architecture notes

Spec surface is the ground truth for the self-improvement loop. Injecting the top 10 descriptions into the planning prompt lets Claude know what the repo already claims to do — reducing the risk of agents writing tests that contradict or duplicate existing spec items. The spec drift check in `lopi check` is the first automated regression guard: any test that disappears between runs is surfaced before it becomes a silent regression.

### Tests

- 24 lopi-spec tests
- 4 spec_commands tests
- Workspace: 362 → **390 passing**, 0 failing

---

## [0.12.0] — Sprint J: GitHub Issue Loop 🪝

### Added

**`crates/lopi-github`** — new crate: thin GitHub REST API write client
- `GitHubClient::new(token)` — constructs a reqwest-based client with `User-Agent: lopi/<version>`
- `GitHubClient::post_comment(owner, repo, issue_number, body)` — posts a comment on any issue or PR
- `GitHubClient::add_labels(owner, repo, issue_number, labels)` — adds one or more labels

**`crates/lopi-webhook/src/issue_triage.rs`** — Haiku-powered issue classifier
- `IssueCategory: Bug | Feature | Question | WontFix` — four-way classification
- `IssueTriage { category, confidence, summary }` — structured triage output
- `classify_issue(client, limiter, breaker, model, title, body)` — calls Haiku with a byte-stable system prompt (`cache_control: ephemeral`) for cross-issue cache hits; cost ~$0.0003/issue
- `parse_triage_response(raw)` — defensive three-line parser: category, confidence (clamped 0–1), ≤120-char summary
- `format_triage_comment(triage, repo)` — formatted Markdown comment including category icon, confidence %, summary, and action description
- 14 unit tests covering parsing, edge cases, label mapping, comment formatting

**`crates/lopi-webhook/src/issue.rs`** — issue handler
- `IssuePayload` — parsed issue fields: owner, repo, full_name, number, title, body, labels
- `IssuePayload::has_lopi_fix_label()` — case-insensitive `lopi:fix` label check
- `extract_from_json(payload, full_name)` — zero-copy extraction from raw webhook JSON
- `spawn_triage(...)` — fires a Tokio background task: classify → comment → label → optionally queue fix task
- Auto-queue threshold: Bug + confidence ≥ 0.7, OR any issue with `lopi:fix` label (overrides classification)

**`crates/lopi-webhook/src/github.rs`** — extended webhook router
- `TriageConfig { api_client, github, limiter, breaker, model }` — optional triage configuration passed to `serve()`
- `serve(queue, secret, addr, triage: Option<TriageConfig>)` — updated signature; triage is opt-in, webhook returns 200 immediately while triage runs in background
- Routes `issues` event `action == "opened"` and `action == "labeled"` to `issue::spawn_triage`

**`src/main.rs`** — new CLI command
- `lopi serve-webhooks [--port 3001] [--host ...] [--webhook-secret ...] [--github-token ...] [--anthropic-key ...]`
- All credentials also read from `LOPI_WEBHOOK_SECRET`, `GITHUB_TOKEN`, `ANTHROPIC_API_KEY` env vars
- Triage enabled only when both `GITHUB_TOKEN` and `ANTHROPIC_API_KEY` are set; gracefully degrades to comment-only webhook server otherwise

### Architecture notes

The webhook server runs independently from `lopi sail` — two separate processes with separate ports (3001 vs 3000). Webhook returns 200 immediately; all AI work (Haiku triage call, GitHub API write) happens in a spawned Tokio task. If either fails, a `tracing::warn!` is emitted and the issue is skipped — webhook liveness is never blocked by external API calls.

Kitchen Loop analogy: this is the inbound side of the loop. Issues arrive from GitHub → lopi triages and queues → agents fix and open PRs → reviewer merges → patterns learned. Combined with Sprint I's lesson injection, the self-improvement cycle is now end-to-end.

### Tests
- 2 lopi-github tests (client construction)
- 14 lopi-webhook issue_triage tests
- 2 lopi-webhook issue.rs tests
- 18 new tests total. Workspace: 313 → **331 passing**, 0 failing.

---

## [0.11.0] — Sprint I: Phase 5b Self-Improvement Second Wave

### Added

**Score weights wiring** (`crates/lopi-agent/src/runner/mod.rs`)
- `AgentRunner::score_weights: ScoreWeights` — field; defaults to `ScoreWeights::default()`
- `AgentRunner::task_lessons: Vec<String>` — lessons for injection into the API planning path
- `AgentRunner::with_score_weights(weights)` — chainable builder
- Run loop now logs weighted score alongside raw score: `📊 score: pass=X% lint=Y diff=ZL (weighted=W.WW)`
- Fixed-score path also logs weighted score after the in-place fix attempt

**`compute_weight_adjustments()` in pool.rs** — free function that computes per-task score weights before handing off to the runner. Placeholder: returns defaults. Phase 5b.1 will query approved patterns for weight tuning.

**Lesson + Pattern injection** (`crates/lopi-agent/src/claude.rs`, `run_loop.rs`)
- `ClaudeCode::patterns: Vec<(String, String)>` + `ClaudeCode::with_patterns()` — tabular (keywords, constraints) pairs fed to TOON encoder at site 2
- `ClaudeCode::lessons: Vec<(String, String)>` + `ClaudeCode::with_lessons()` — (category, content) lessons from the lessons table
- `plan()` now passes both to `encode_task_context()` — TOON renders them as §9.3 tabular rows (saves ~158 tokens/attempt)
- `run_loop.rs` single memory query now builds **both** string constraints (legacy) **and** tabular pattern pairs; loads lessons via `store.load_lessons(repo_path, 10)` and stores them in `self.task_lessons` for the API path
- Extracted `plan_streaming()` → new `crates/lopi-agent/src/claude_stream.rs` (claude.rs: 474 → 408 lines)

**Post-mortem lessons** (`crates/lopi-agent/src/runner/run_loop.rs`)
- After `insert_postmortem_pattern()` succeeds, also calls `store.save_lesson(repo_path, "recovery", constraint, Some(task_id), 1.0)` — makes the constraint discoverable in future lesson injections

**API plan lessons** (`crates/lopi-agent/src/runner/api_plan.rs`)
- `build_user_prompt(task, last_error, lessons)` — appends `# Lessons from past patterns` section when lessons are non-empty
- 1 new test: `user_prompt_includes_lessons_when_provided`

**CLI annotate** (`src/main.rs`)
- `lopi learn annotate <id-prefix> <approved|rejected>` — validates annotation, resolves id prefix via `find_pattern_by_id_prefix`, calls `annotate_pattern()`

### Tests
- 1 new api_plan test. Workspace: 261 → **313 passing**, 0 failing.

---

## [0.10.0] — Sprint H: Self-Improvement Engine 🧠

### Added

**`lopi learn` CLI subcommand tree** (was a single flat command)
- `lopi learn list [--limit N] [--postmortem-only]` — sorted pattern table with id prefix, keywords, avg attempts, success %, and source emoji (📊 mined / 🧠 post-mortem)
- `lopi learn show <id-prefix>` — full pattern detail page
- `lopi learn export [--limit N]` — JSON output to stdout for analytics pipelines

**`runner::postmortem` module** (`crates/lopi-agent/src/runner/postmortem.rs`)
- `run_postmortem(client, limiter, breaker, model, goal, error_log)` — single-turn Claude reflection over a failed run. Returns one imperative constraint string (≤ 200 chars, must start with `must` / `do not` / `always` / `never`).
- `extract_constraint(raw)` — defensive validation: strips markdown bullets, takes first non-empty line, rejects fluffy non-imperative responses, truncates over-long lines.
- `run_postmortem_quiet(...)` — error-swallowing variant for terminal-failure path: never blocks task completion.
- System prompt is byte-stable for `cache_control: ephemeral` cache hits across post-mortems in a session.

**Adaptive retry** (`AgentRunner::with_adaptive_retry()`)
- New builder method, chainable on top of `with_api(...)`.
- Stashes the previous attempt's score (test_pass_rate, lint_errors, diff_lines, errors) as `last_error` after each failed attempt.
- After all retries exhausted, automatically fires `run_postmortem_if_configured()` — runs the post-mortem if both adaptive retry AND a configured `AnthropicClient` are present.
- Persists the derived constraint to the patterns table.

**`MemoryStore` additions** (`crates/lopi-memory/src/store.rs`)
- `insert_postmortem_pattern(goal_keywords, constraint) -> id` — creates a row with `derived_from_postmortem = 1`, seeded `success_rate = 0.0`.
- `find_pattern_by_id_prefix(prefix) -> Option<PatternRow>` — for `lopi learn show` UX.
- `load_patterns` ordering changed: `ORDER BY COALESCE(success_rate, 0) DESC, last_seen DESC` — real-data patterns now surface above zero-seeded post-mortem rows.

**Schema migration** (`crates/lopi-memory/src/schema.sql`)
- `ALTER TABLE patterns ADD COLUMN derived_from_postmortem INTEGER NOT NULL DEFAULT 0`.
- Fixed `apply_schema()` to correctly strip leading `--` SQL comments before the ALTER TABLE prefix check — comments above ALTER TABLE statements no longer break the duplicate-column-tolerant migration path.

### Tests

- 4 new lopi-memory tests: postmortem-pattern insert + retrieve, prefix-not-found, postmortem flag in load_patterns, ordering correctness.
- 11 new lopi-agent tests in `runner::postmortem::tests`: extract_constraint validation across 7 input shapes, build_prompt determinism + content + truncation.
- 2 new lopi-agent integration tests: `runner_default_has_no_direct_api`, `with_api_enables_direct_path` (already shipped in Sprint G).
- Workspace total: 244 → **261 passing**, 0 failed.

### Architecture note

The post-mortem fires on terminal failure (all retries exhausted) and uses Haiku for cost. A single short turn of <2000 tokens with cached system prompt costs roughly $0.0008. The constraint it derives slots into the existing `extra_constraints` mechanism in the planning prompt — no new prompt-injection plumbing required, the pattern miner already feeds patterns into TOON-encoded prose at planning time.

The `last_error` field is now stashed on the runner but not yet injected into the next attempt's planning prompt — that's a follow-up sprint (H1) since it requires touching the prompt builders in both `claude.rs::plan` and `runner::api_plan::build_user_prompt`.

---

## [0.9.0] — Sprint G: Direct Anthropic SDK planning path

### Added

**Direct API path for the planning step** (`crates/lopi-agent/src/runner/api_plan.rs`)
- `AgentRunner::plan_via_api(model, attempt) -> Result<String>` — replaces the `claude` CLI subprocess call when the runner has been wired with `AnthropicClient` via the new `AgentRunner::with_api(client, limiter, breaker)` builder.
- The CLI path remains the load-bearing default. On any direct-API failure (rate limited, breaker open, network error, 4xx/5xx) the run loop falls back to the CLI silently — an API outage cannot stall agent execution.

**Resilience layered on every API request:**
1. `CircuitBreaker::check()` — refuses if open from prior failures or if the hourly cost cap was hit.
2. `AnthropicLimiter::acquire_request(4000)` — concurrent TPM + RPM enforcement at default-pro limits (120k TPM / 15 RPM).
3. `AnthropicClient::stream_plan` — SSE streaming with `cache_control: ephemeral` on the system prompt for ~90% cost reduction on repeat calls.
4. `CircuitBreaker::record_success` / `record_failure` / `record_cost` — feeds the failure counter and hourly USD spend back into the breaker.

**Real `TurnMetrics` from API responses:**
- Every successful direct-API plan call captures real `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_write_input_tokens`, `ttft_ms`, `turn_latency_ms`, and `estimated_cost_usd`.
- `TurnMetrics` event emitted on the `EventBus` so the lopi-ui Forge animates with **real** `cost_usd` and `tokens_per_sec` instead of the phase-derived stubs (UI-2 baseline).
- Persisted to the SQLite `turn_metrics` table via `MemoryStore::save_turn_metrics`.

**Builder API:**
- `AgentRunner::with_api(client: Arc<AnthropicClient>, limiter: Arc<AnthropicLimiter>, breaker: Arc<CircuitBreaker>)` — chainable on top of `new()` or `standalone()`. `has_direct_api()` accessor for tests and tracing.
- New optional fields on `AgentRunner`: `api_client`, `limiter`, `breaker`, `session_id` (used by `TurnMetrics.session_id`).

**Prompt builder:**
- `build_user_prompt(&Task)` — deterministic markdown rendering of goal/constraints/allowed_dirs/forbidden_dirs. Same task → byte-identical prompt → cache hit on the system+user prefix.

### Changed
- `lopi-agent` now depends on `lopi-ratelimit` and `chrono` (workspace).
- `runner/run_loop.rs` planning branch routes through `plan_via_api` first when configured, with transparent CLI fallback.

### Tests
- 7 new tests in `runner::api_plan::tests`: prompt builder determinism + content + section omission, builder integration (default has no direct API; `with_api` enables it).
- lopi-agent: 10 → 17 passing.
- Workspace total: 244 passing, 0 failed.

### Architecture note
The CLI path is intentionally retained for the **implementation step** because file-edit tool access requires the `claude` CLI's native filesystem hooks. Migrating implementation to direct API would require either Anthropic's tool-use protocol with custom file-edit tools, or a sidecar that bridges API tool calls to filesystem ops — neither in scope for this sprint. Sprint G specifically targets the planning step where pure text generation suffices and prompt caching delivers the largest cost win.

---

## [0.8.0] — Observability, Correctness, Systems, Resilience

### Added

**Sprint A — Observability**
- `lopi-core`: `TurnMetrics` struct capturing token accounting (input/output/cache read/write), latency (TTFT, turn, tool execution), context pressure, eviction count, tool call count, and estimated cost per turn
- `lopi-memory`: `turn_metrics` table with `task_id` and `timestamp` indexes; `MemoryStore::save_turn_metrics()` for persisting per-turn records
- `benchmarks/corpus/README.md`: ten canonical benchmark tasks T01–T10 with measurement protocol and acceptance criteria
- `benchmarks/run.sh`: shell runner for the corpus — per-task logging, machine-readable JSON summary in `benchmarks/results/<timestamp>/`

**Sprint E — Systems**
- `src/main.rs`: mimalloc installed as global allocator (`#[global_allocator]`) — ~30% lower allocation latency on macOS vs system malloc
- `lopi-agent/runner.rs`: `backoff_secs()` — full-jitter exponential backoff (base 500 ms, cap 30 s, Uniform[0, ceiling]) applied before each retry
- `.config/nextest.toml`: nextest configuration — default profile uses `num-cpus` threads; `ci` profile adds 2 retries, 120 s test timeout, slow-timeout termination
- `.cargo/config.toml`: `[alias] t = "nextest run"`
- `crates/lopi-context/benches/eviction.rs`: three Criterion benchmarks (`evict_to_budget_100_turns`, `to_api_messages_1000_turns`, `push_at_75pct_pressure`)

**Sprint F — Resilience**
- New crate `crates/lopi-ratelimit`:
  - `TokenBucket`: async token-bucket with non-spinning `acquire()` (computed wait from deficit) and non-blocking `try_acquire()`
  - `AnthropicLimiter`: dual TPM+RPM enforcement; `default_pro()` sets 120k TPM / 15 RPM; `acquire_request()` concurrently awaits both buckets via `tokio::join!`
  - `CircuitBreaker`: Closed → Open → HalfOpen state machine with two independent trip conditions (consecutive failures + per-hour USD cost cap); hourly automatic reset

### Changed

**Sprint B — Correctness**
- `lopi-memory`: `MemoryStore` refactored to dual-pool architecture — `write_pool` (max 1 connection, serialises all INSERTs/UPDATEs/DDL) and `read_pool` (max 8 connections, read-only); in-memory tests share one pool safely
- `lopi-git`: `checkout_new_branch()` now holds a process-wide `WORKTREE_LOCK` (once_cell `Lazy<Mutex<()>>`) for the duration of the git branch + checkout sequence to prevent parallel agent races on HEAD/index
- `lopi-git`: `GitManager::worktree_env()` returns `[("CARGO_TARGET_DIR", ".cargo-target")]` — consumed by agent sub-process spawn to isolate `target/` directories
- `lopi-agent`: `AgentRunner` gains `max_turns: u32` (default 25) and `turn_count: u32`; hard stop emits `TaskStatus::Failed { reason: "TurnLimitExceeded … " }` before the turn limit is exceeded
- `lopi-agent`: `AgentRunner` gains a `CancellationToken` field alongside the existing oneshot cancel channel; `check_cancel()` checks the token first (structured shutdown path) then the oneshot (web API / CLI path)
- `lopi-orchestrator`: `AgentPool` gains a `JoinSet<()>` field for structured task tracking; tasks are spawned into the join set; completed tasks are drained on each dispatch; `shutdown()` calls `abort_all()` and drains

### Tests
- lopi-ratelimit: 10 new tests — 0 failures
- Total workspace (excluding lopi-context integration): **57 unit tests, 0 failures**
- Criterion benchmarks in lopi-context compile and run cleanly

## [0.7.0] — lopi-context: KV cache eviction layer

### Added
- `crates/lopi-context` — new crate owning all Anthropic message history as a mutable, policy-driven data structure
  - `TaggedMessage` — wire-format message with eviction metadata: `PinPolicy`, `Phase`, `tool_pair_id`, `is_conclusion`, `evict_after`
  - `ContextWindow` — the central type; `push()`, `push_tool_pair()`, `transition_phase()`, `pin_conclusion()`, `evict_phase()`, `evict_to_budget()`, `evict_turn()`, `to_api_messages()`
  - Three composable eviction policies: `PhaseTransition` (explicit phase sweep), `BudgetLIFO` (oldest-first when pressure > 75%), `ExplicitTag` (per-turn `evict_after` sentinel)
  - **Invariant: tool_use/tool_result pairs are always evicted atomically** — `evict_turn(id, force=false)` returns `OrphanedToolPair` error; `force=true` evicts both
  - **Invariant: `is_conclusion = true` turns survive all automatic policies** — only `evict_turn(id, force=true)` can remove them
  - **Invariant: `to_api_messages()` always returns turns in insertion order**, regardless of what was evicted from the middle
  - Token estimation via `tiktoken-rs` cl100k_base (text blocks: BPE; tool blocks: JSON/4; 4-token overhead per message)
  - `ContextStats`, `EvictionStats`, `EvictionRecord` for observability; eviction log ready for Phase 2 SQLite persistence
- `lopi-agent` — `AgentRunner` now carries a `ContextWindow` tracking Boot → Planning → Implementation → Testing → Conclusion phase transitions; logs `token_pressure()` at each transition via `tracing::info!`
- 20 new tests across 5 integration test files: `tool_pair_atomicity`, `phase_eviction`, `budget_lifo`, `conclusion_preservation`, `api_message_ordering` — all deterministic, no API key required
- 1 `#[ignore]` integration test (`token_estimation`) verifying estimate within 10% of Anthropic count-tokens API; run with `cargo test --test token_estimation -- --ignored`

### Changed
- `lopi-agent` depends on `lopi-context`; `AgentRunner` gains a `pub context: ContextWindow` field
- Fixed pre-existing clippy warnings in `lopi-toon` (while_let_loop, unnecessary_to_owned, manual_strip ×3), `lopi-git` (needless_match), `lopi-orchestrator` (doc quote), `lopi-ui` (unnecessary_to_owned), `lopi` main (print_literal ×2)

### Architecture note
lopi-agent currently uses the `claude` CLI binary, not the Anthropic SDK directly. `lopi-context` tracks session-phase state and token pressure across the agent run. The `to_api_messages()` output is the integration point for future direct-SDK migration. The eviction contract is established now — the wire-up to API calls is the next phase.

### Tests
- lopi-context: 20 new tests — 0 failures
- **Total: 101 tests, 0 failures**

## [0.6.0] — lopi-toon: TOON encoder/decoder + prompt token reduction

### Added
- `crates/lopi-toon` — full TOON v3.0 encoder and decoder per spec (https://toonformat.dev/)
  - `encode(value: &Value) -> String` — encodes JSON data model to TOON
  - `decode(input: &str) -> Result<Value>` — decodes TOON back to JSON with strict validation
  - `encode_task_context(goal, allowed, forbidden, constraints, patterns)` — lopi-specific helper
  - Encoder: tabular arrays (§9.3), inline primitive arrays (§9.1), expanded mixed arrays (§9.4)
  - Encoder: minimal quoting per §7.2 — reserved words, numeric-like strings, special chars
  - Encoder: canonical number format — no exponents, no trailing zeros, -0→0, NaN/Inf→null
  - Decoder: root form discovery (§5), keyed vs root array headers (§5 fix)
  - Decoder: inline arrays, tabular rows, expanded list items, nested objects
  - Decoder: `split_on_delim` respects quoted strings; strict count/width enforcement
  - 29 tests covering: all scalar types, quoting edge cases, flat/nested objects, all array forms,
    spec example round-trip, token efficiency assertion
- `lopi-agent/src/claude.rs` — TOON integrated at all three sites from token analysis:
  - **Site 1** (`plan()`, `implement()`): constraints/allowed_dirs/forbidden_dirs arrays
    encoded as TOON §9.1 inline arrays (~17 tokens/prompt saved, ~14% reduction)
  - **Site 2** (`plan()` via `runner.rs`): pattern memory injected as TOON context
    (~158 tokens/attempt saved, grows linearly with pattern count — the dominant win)
  - **Site 3** (`fix()`): error text is free-form prose — TOON intentionally skipped (no gain)
- At 100 tasks/day, estimated **-1.9M tokens/month** net reduction

### Changed
- `lopi-agent` now depends on `lopi-toon`
- `claude.rs::plan()` prompt uses `encode_task_context()` for structured context block
- `claude.rs::implement()` uses TOON scope block for allowed/forbidden dirs
- `claude.rs::fix()` uses inline TOON array for allowed_dirs (prose errors unchanged)

### Tests
- lopi-toon: 29 new tests — 0 failures
- **Total: 75 tests, 0 failures**

## [0.5.0] — Phase 4: Scheduled Tasks, Repo Profiles, lopi watch --remote

### Added
- `ScheduleEntry` type in `lopi-core` — `name`, `repo`, `goal`, `cron`, `priority`, `allowed_dirs`, `forbidden_dirs`; fully serde-compatible with `[[schedules]]` TOML arrays
- `RepoProfile` type in `lopi-core` — per-repo `.lopi.toml` profile with `allowed_dirs`, `forbidden_dirs`, `test_command`, `lint_command`, `default_constraints`, `max_retries`; `apply(&mut Task)` merges non-empty overrides
- `RepoProfile::load_from_repo(path)` — reads `<repo>/.lopi.toml`, returns `Default` if not found
- `LopiConfig::find_and_load()` — auto-discovers `./lopi.toml` then `~/.lopi/lopi.toml`
- `lopi-orchestrator::scheduler` module — `boot(entries, pool)` registers async cron jobs via `tokio-cron-scheduler`; `next_run_times(cron, n)` computes upcoming fire times
- `lopi schedules list` — prints configured schedules with next UTC run time
- `lopi watch --remote <url>` — connects to a running `lopi sail` WebSocket, injects events into local bus, drives the ratatui TUI from network events
- `lopi watch --local` — original isolated local bus behaviour
- `lopi sail` boots the cron scheduler alongside the agent pool if `[[schedules]]` are configured
- `lopi run` reads per-repo `.lopi.toml` and applies it before submitting the task
- `.lopi.toml.example` — per-repo profile template
- Updated `lopi.toml.example` with commented `[[schedules]]` examples

### Tests
- lopi-core: +6 tests (schedule_entry_deserializes, config_with_schedules, config_empty_schedules, repo_profile_default, repo_profile_apply_overrides, repo_profile_apply_skips_empty) → **20 total**
- lopi-orchestrator: +2 tests (next_run_times_valid_expr, next_run_times_invalid_expr) → **7 total**
- **Total: 46 tests, 0 failures**

## [0.4.0] — Phase 2 Full: live concurrency, ratatui TUI, full dashboard

### Added
- `AgentEvent` enum in lopi-core — rich events replacing plain `TaskStatus` broadcasts:
  `TaskQueued`, `TaskStarted`, `StatusChanged`, `LogLine`, `ScoreUpdated`, `TaskCompleted`, `TaskCancelled`, `PoolStats`
- `LogLevel` enum (`info`, `warn`, `error`, `debug`) with `AgentEvent::info/warn/error` helpers
- `AgentPool`: `DashMap<TaskId, AgentHandle>` tracking live agents with `cancel_tx: oneshot::Sender<()>`
- `AgentPool::cancel(task_id)` — graceful cancel signal to running agent
- `AgentPool::submit(task)` — enqueue + broadcast `TaskQueued` + save to DB
- `AgentPool::stats()` → `PoolStats { running, queued, succeeded, failed, uptime_secs }`
- `AgentPool::with_store()` — attach memory for pattern mining + DB persistence
- `AgentRunner` upgraded: emits `AgentEvent` at every stage (LogLine, StatusChanged, ScoreUpdated, TaskStarted); accepts `cancel_rx: oneshot::Receiver<()>` and polls cancel between stages; integrates `MemoryStore` for attempt persistence and pattern seeding
- `ClaudeCode::with_extra_constraints()` — injects memory patterns into planning prompt
- Full ratatui TUI (`lopi watch`): agent table with 7 columns, log panel (last 20 lines with level color), stats bar, help overlay, keyboard: `q/j/k/↑↓/Enter/l/Esc/?/F1`
- Full web dashboard (`index.html`): dark Konjo purple theme, live agent cards with score bar + elapsed timer + cancel button, sidebar submit form (goal/repo/priority, Ctrl+Enter), log stream, WebSocket reconnect with exponential backoff, state snapshot on connect
- `GET /api/stats` — running/queued/succeeded/failed/uptime_secs
- `DELETE /api/tasks/:id` — cancel task via HTTP (proxied to pool cancel)
- `GET /ws` — WebSocket endpoint with full state snapshot on connect, then `AgentEvent` stream; `/ws/tasks` retained for compat
- `lopi cancel <task-id>` — CLI cancel via HTTP DELETE to running sail server
- `lopi learn [--limit N]` — pretty-print mined patterns table (keywords / avg_attempts / success% / last_seen)
- `lopi dock` — pretty table output (ID / Goal / Status columns)
- `lopi run` — streams live `StatusChanged` + `LogLine` + `ScoreUpdated` events to stdout

### Changed
- `EventBus<T>` remains in lopi-core/event.rs alongside `AgentEvent` and `LogLevel`
- `lopi sail` now passes `Arc<AgentPool>` to web server; pool boots as background task
- `lopi-ui::web::serve()` signature: takes `Arc<AgentPool>` instead of raw bus
- All existing tests pass (38 total, 0 failures)

### Tests
- lopi-core: +2 tests (`agent_event_log_helpers`, `agent_event_serde_round_trip`) → 14 total
- All others unchanged: lopi-git (3), lopi-orchestrator (5), lopi-memory (11), lopi-webhook (5)
- **Total: 38 tests, 0 failures**

## [0.3.0] — Remote control + self-improvement

### Added
- `POST /api/tasks` — inject tasks into the live AgentPool queue with `goal`, `priority`, `allowed_dirs`, `max_retries`; returns `{id, goal, queued, duplicate_of}`
- `GET /api/tasks/:id` — fetch a specific task by full or prefix ID
- `GET /api/patterns` — expose mined patterns ordered by success rate
- Telegram: `/urgent <goal>` command for `Priority::High` tasks; inline keyboard (priority bump / cancel) on every queued task; `CallbackQuery` handler for button responses
- GitHub webhook: HMAC-SHA256 verification via `X-Hub-Signature-256` header; returns 401 on failure; constant-time comparison
- `MemoryStore::mine_patterns()` — extracts sorted keyword fingerprint from goal, upserts running averages into `patterns` table after each completed run
- `MemoryStore::load_patterns(limit)` — returns patterns ordered by `success_rate DESC`
- `AgentPool::with_store(store)` — attaches memory for pattern mining and `mark_completed` after each agent run
- `hmac`, `sha2`, `hex` added as workspace dependencies

### Changed
- `lopi_ui::web::serve()` now takes `TaskQueue` as third argument (task injection)
- `AppState` in `lopi-ui` now holds a `TaskQueue` handle
- `AgentPool::new()` signature unchanged; optional store via `with_store()`
- `main.rs`: `lopi sail` passes queue to both pool and web server; store attached to pool

### Tests
- lopi-memory: +4 tests (mine_patterns insert, upsert dedup, short-word skip, load ordering)
- lopi-webhook: +5 tests (valid HMAC, wrong secret, tampered body, missing prefix, empty sig)
- Total: 36 tests, 0 failures

## [0.2.0] — Live concurrency + test foundation

### Added
- `lopi-core::EventBus<T>` — thin tokio broadcast wrapper for workspace-wide event fanout
- `TaskStatus` is now `Clone + PartialEq` (derived in lopi-core)
- `AgentRunner::standalone()` — creates its own isolated bus for `lopi run`
- `AgentRunner::new()` — takes a shared `EventBus<TaskStatus>` for pool integration
- `AgentPool` now receives and propagates the shared bus to every spawned runner
- `lopi sail` boots the `AgentPool` as a background task; exposes `/ws/tasks` WebSocket endpoint
- WebSocket handler fans out serialized `TaskStatus` JSON to all connected clients; handles lag gracefully
- `lopi run` streams live status events to stdout while the agent executes
- `lopi tail --history` shows past tasks from SQLite; `--task-id` filters by prefix
- `ClaudeCode` upgraded to use `--output-format json` with `ClaudeOutput` struct and transparent fallback for older CLI versions
- `MemoryStore::open_in_memory()` for test isolation
- `MemoryStore::task_count()` helper
- 27 tests across lopi-core (12), lopi-git (3), lopi-orchestrator (5), lopi-memory (7)

### Changed
- `lopi-ui::web::serve()` now takes `EventBus<TaskStatus>` as second argument
- `lopi-orchestrator::AgentPool::new()` now takes `EventBus<TaskStatus>`
- `lopi-core` dependency added to `lopi-ui` and root binary

All notable changes to lopi.

## [0.1.0] — Initial scaffold

### Added
- Cargo workspace with 8 crates: `lopi-core`, `lopi-git`, `lopi-agent`, `lopi-memory`, `lopi-orchestrator`, `lopi-ui`, `lopi-remote`, `lopi-webhook`
- `lopi-core` types: `Task`, `TaskId`, `TaskStatus`, `Priority`, `TaskSource`, `AgentRun`, `Attempt`, `AgentState`, `Score`, `LopiConfig`
- `lopi-git`: `GitManager` (real git2 integration: branch, rollback, commit, PR via `gh`) + `DiffChecker` with allow/forbid glob enforcement
- `lopi-agent`: `AgentRunner` with the full Plan → Implement → Diff-check → Test → Score → Fix → Retry → PR loop
- `lopi-memory`: sqlx SQLite store with `tasks`, `attempts`, `patterns` tables
- `lopi-orchestrator`: priority `TaskQueue` (with goal-dedup) and `AgentPool` (Semaphore-bounded)
- `lopi-ui`: ratatui TUI dashboard + axum JSON API + minimal static dashboard
- `lopi-remote`: teloxide bot (`/help /task /status /approve`) + Twilio WhatsApp webhook
- `lopi-webhook`: GitHub webhook receiver that injects high-priority fix tasks on CI failure
- CLI binary `lopi`: `run | watch | tail | dock | sail`
- Docs: CLAUDE.md, KONJO_PROMPT.md, PLAN.md, README.md, lopi.toml.example
