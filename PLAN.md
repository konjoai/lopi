# PLAN.md — lopi

## v0.1.0 — Phase 1 MVP (shipped)

- [x] Cargo workspace + 8 crates
- [x] Real types in `lopi-core`
- [x] Real git2 isolation in `lopi-git`
- [x] Real agent loop in `lopi-agent`
- [x] Real sqlx SQLite store in `lopi-memory`
- [x] Real bounded agent pool + priority queue in `lopi-orchestrator`
- [x] ratatui TUI + axum JSON API in `lopi-ui`
- [x] teloxide Telegram bot + Twilio webhook in `lopi-remote`
- [x] GitHub webhook in `lopi-webhook`
- [x] CLI: `run | watch | tail | dock | sail`
- [x] `cargo build` clean

## v0.2.0 — Live concurrency + test foundation ✅

- [x] `EventBus<T>` shared broadcast abstraction in lopi-core
- [x] `AgentRunner` emits `TaskStatus` events to the shared bus
- [x] `AgentPool` passes the bus to each spawned runner
- [x] `lopi sail` boots the AgentPool + exposes `/ws/tasks` WebSocket feed
- [x] WebSocket handler fans out `TaskStatus` JSON to all connected clients
- [x] `lopi run` streams live status events to stdout
- [x] `lopi tail --history` shows past tasks from DB
- [x] `claude --output-format json` structured output with fallback
- [x] Unit tests: lopi-core (12 tests), lopi-git DiffChecker (3), lopi-orchestrator TaskQueue (5)
- [x] Integration tests: lopi-memory (7 tests, in-memory SQLite)

## v0.3.0 — Remote control + self-improvement ✅

- [x] `POST /api/tasks` — inject tasks into the live AgentPool queue via HTTP
- [x] `GET /api/tasks/:id` — fetch status of a specific task by ID prefix
- [x] `GET /api/patterns` — expose mined patterns via API
- [x] Telegram: `/task <goal>` injects via shared `TaskQueue`; `/urgent <goal>` for high-priority
- [x] Telegram: inline keyboard buttons (priority bump / cancel) on queued tasks
- [x] Telegram: `/approve <id>` acknowledgment flow
- [x] GitHub webhook: HMAC-SHA256 signature verification (`X-Hub-Signature-256`)
- [x] Pattern miner: after each completed `AgentPool` run, upserts into `patterns` table
- [x] `AgentPool::with_store()` — optional memory attachment for pattern mining + mark_completed
- [x] `MemoryStore::mine_patterns()` — keyword fingerprint extraction + running average upsert
- [x] `MemoryStore::load_patterns()` — ordered by success_rate DESC
- [x] 36 tests: lopi-core (12), lopi-git (3), lopi-orchestrator (5), lopi-memory (11), lopi-webhook (5)

## v0.4.0 — Self-improvement (deeper)

- [ ] Constraint pre-seeding: on new task, query `find_similar_patterns` and inject winning constraints
- [ ] Success-rate dashboard panel in TUI and web
- [ ] `lopi patterns` CLI command — show mined patterns in terminal
- [ ] Pattern decay: age out patterns not seen in 30 days
