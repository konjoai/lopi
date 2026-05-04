# PLAN.md — lopi Master Plan

## Vision

lopi is the Konjo agent runtime. It runs Claude Code agents concurrently, each in git-isolated branches. It learns from every run, self-improves over time, and is controllable entirely from a phone. The web UI is clean and fast. The mobile experience is better than anything that exists today for agent orchestration.

---

## Phase 1 — MVP Core (Wk 1–3) ✅ SHIPPED v0.1.0

- [x] Cargo workspace (8 crates)
- [x] lopi-core types (`Task`, `Score`, `AgentRun`, `EventBus`)
- [x] lopi-git: branch isolation, rollback, `DiffChecker`
- [x] lopi-agent: Plan → Implement → Test → Score → Retry → PR loop
- [x] lopi-memory: SQLite persistence (`tasks`, `attempts`, `patterns`)
- [x] lopi-orchestrator: `AgentPool` + `TaskQueue` (priority + dedup)
- [x] lopi-ui: axum API + ratatui skeleton
- [x] lopi-remote: Telegram + WhatsApp stubs
- [x] CLI: `lopi run | watch | tail | dock | sail`

---

## Phase 2 — N Parallel Agents + Live Dashboard (Wk 4–6) ✅ SHIPPED v0.2.0

- [x] `AgentPool`: real Semaphore-bounded concurrency (`--max-agents`)
- [x] `EventBus<TaskStatus>`: real-time status broadcasts via `tokio::broadcast`
- [x] `AgentRunner` emits events to the shared bus
- [x] `lopi sail`: boots pool + exposes `/ws/tasks` WebSocket
- [x] WebSocket handler fans out `TaskStatus` JSON to all connected clients
- [x] `lopi run` streams live status events to stdout
- [x] `claude --output-format json` with `ClaudeOutput` struct + fallback
- [x] 27 tests: lopi-core (12), lopi-git (3), lopi-orchestrator (5), lopi-memory (7)

**Remaining in Phase 2 (v0.2.x):**
- [ ] ratatui TUI: live agent table (goal / status / attempt / score / branch / elapsed), log panel, keyboard controls
- [ ] Web dashboard upgrade (single-file, dark Konjo theme):
  - Live agent grid (cards: goal, status, progress bar, score, branch)
  - Task queue panel (submit new task via form)
  - Log stream panel (tail of recent events)
  - Stats bar (total tasks / pass rate / avg retries / uptime)

---

## Phase 3 — Remote Control + Self-Improvement (Wk 7–10) ✅ SHIPPED v0.3.0

- [x] `POST /api/tasks` — inject tasks into live `AgentPool` queue
- [x] `GET /api/tasks/:id` — fetch status by ID prefix
- [x] `GET /api/patterns` — mined pattern feed ordered by success rate
- [x] Telegram: `/task`, `/urgent`, `/status`, `/approve`, `/dock`
- [x] Telegram: inline keyboard (priority bump / cancel) on every queued task
- [x] Telegram: `CallbackQuery` handler for button responses
- [x] GitHub webhook: HMAC-SHA256 (`X-Hub-Signature-256`), 401 on bad sig, constant-time comparison
- [x] Pattern miner: keyword fingerprint extraction + running average upsert after each run
- [x] `AgentPool::with_store()` — mines patterns + marks completed after every agent run
- [x] 36 tests: lopi-core (12), lopi-git (3), lopi-orchestrator (5), lopi-memory (11), lopi-webhook (5)

---

## Phase 4 — Scheduled Tasks + Repo Profiles (Wk 9–10)

- [ ] `[schedules]` section in `lopi.toml`:
  ```toml
  [[schedules]]
  name = "nightly-lint"
  repo = "/Users/wesleyscholl/myrepo"
  goal = "Fix all clippy warnings"
  cron = "0 2 * * *"
  priority = "low"

  [[schedules]]
  name = "weekly-deps"
  repo = "/Users/wesleyscholl/myrepo"
  goal = "Update all dependencies to latest compatible versions"
  cron = "0 9 * * MON"
  ```
- [ ] `tokio_cron_scheduler` for cron execution
- [ ] `lopi schedules list` — show all schedules + next run time
- [ ] `lopi schedules add` — interactive schedule builder
- [ ] Telegram: `/schedule list`, `/schedule add`
- [ ] Repo profiles: per-repo `.lopi.toml` at repo root (`allowed_dirs`, `forbidden_dirs`, `test_command`, `lint_command`, `default_constraints`)

---

## Phase 5 — Self-Improvement Engine (Wk 11–14)

- [ ] Pattern learning: before running a new task, query similar past tasks → suggest constraints that worked → pre-load into system prompt
- [ ] `lopi learn` CLI command — show pattern library, success rates, top constraints
- [ ] Failure post-mortem: when all retries fail, run a "post-mortem" Claude session that analyzes the error log → generates new constraint/approach suggestion → stored as a pattern
- [ ] Self-modification loop (guarded): lopi can be given tasks targeting its own codebase in `crates/` — ONLY when `allow_self_modify = true` in config; same git isolation and PR workflow applies
- [ ] Adaptive retry: if attempt N failed with error type X, adjust prompt strategy for attempt N+1 (pass error + suggest different approach)
- [ ] Scoring evolution: score weights configurable and tunable based on which metrics correlate with user-approved PRs vs rejected ones
- [ ] `lopi learn` — browse pattern library interactively

---

## Phase 6 — GitHub Webhooks + CI Integration (Wk 15–16)

- [ ] `lopi-webhook` fully wired end-to-end:
  - CI failure → auto-queue fix task at `Priority::High`
  - Issue labeled `lopi:fix` → auto-queue
  - PR review comment → feed back to agent for revision
- [ ] `lopi serve-webhooks --port 3001` — dedicated webhook server command
- [ ] GitHub App mode: register as a GitHub App for proper auth + org-wide hooks
- [ ] Configurable rules: which events trigger which task templates
- [ ] HMAC verification for all event types (already implemented for CI failures in v0.3.0)

---

## Phase 7 — Production Web UI (Wk 17–20)

- [ ] Proper React (or Svelte) frontend — separate from embedded HTML:
  - Auth: simple token-based login
  - Agent dashboard: real-time agent cards with expandable log panels
  - Task composer: goal editor, repo picker, constraint builder, schedule toggle
  - Memory explorer: browse pattern library, success rates, annotate
  - Schedule manager: CRUD for scheduled tasks, next-run countdown
  - PR queue: agent-opened PRs waiting for approval (link to GitHub)
  - Settings: global config editor, repo profile manager, bot configuration
- [ ] Mobile-responsive (works on phone browser)
- [ ] Dark theme, Konjo aesthetic, fast

---

## Phase 8 — Native Mobile App (Wk 21–28)

- [ ] React Native (shares TypeScript + API types with web frontend)
- [ ] Push notifications via FCM/APNs: task completed, PR opened, CI fixed, task failed
- [ ] Per-task conversation threads (mirrors Telegram thread model natively)
- [ ] Voice input: dictate a task goal → transcribe → queue
- [ ] Quick actions widget (iOS/Android): "New task", "View dock", "Approve PRs"
- [ ] WebSocket connection with reconnect + offline queue (the "better than Claude Dispatch" goal)
- [ ] Connection indicator: green/amber/red dot for lopi server reachability
- [ ] Background sync: notifications arrive even when app is backgrounded
- [ ] Every agent session tracked, every completion notified, every error surfaced with context

---

## Phase 9 — Intelligence + Evolution (Ongoing)

- [ ] Multi-agent roles: Planner agent decomposes complex goals → spawns Implementer agents → Reviewer agent checks diff before PR
- [ ] Cross-repo awareness: agents can read (not write) other repos for context
- [ ] Goal decomposition: `lopi plan "Refactor the auth module"` → Claude breaks into subtasks → runs in dependency order
- [ ] Embedding-based memory: store attempt summaries as vectors → semantic search for similar past work
- [ ] Agent-to-agent communication: agents leave structured notes for each other via lopi-memory
- [ ] Leaderboard: track which constraint templates produce the highest pass rates → surface as suggested starting points
- [ ] Feedback loop: user marks approved PRs as "good" / rejected as "bad" → tune scoring weights accordingly

---

## Current Health

| Metric | Value |
|--------|-------|
| Tests | 36 passing, 0 failing |
| Build | Clean (0 warnings) |
| Crates | 8 |
| CLI commands | `run`, `watch`, `tail`, `dock`, `sail` |
| API endpoints | `GET /api/tasks`, `POST /api/tasks`, `GET /api/tasks/:id`, `GET /api/patterns`, `GET /api/health`, `GET /ws/tasks` |
| Latest release | v0.3.0 |

---

*KONJO — Know, Outline, Nail, Justify, Optimize.*
*Plan, build, test, ship, rest, repeat.*
*ᨀᨚᨐᨚ — build the ship. make it seaworthy.*
