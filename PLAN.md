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

## v0.3.0 — Remote control

- [ ] Telegram inline-keyboard approval flow
- [ ] HMAC-verified GitHub webhook
- [ ] WhatsApp outbound status updates

## v0.4.0 — Self-improvement

- [ ] Pattern miner: `attempts` → `patterns`
- [ ] Constraint pre-seeding from similar past tasks
- [ ] Success-rate dashboards per pattern
