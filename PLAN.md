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

## v0.2.0 — Live concurrency

- [ ] `lopi sail` boots the AgentPool and exposes WebSocket task feed
- [ ] TUI subscribes to the same broadcast channel as the web dashboard
- [ ] `lopi tail --task-id X` streams logs from a specific run

## v0.3.0 — Remote control

- [ ] Telegram inline-keyboard approval flow
- [ ] HMAC-verified GitHub webhook
- [ ] WhatsApp outbound status updates

## v0.4.0 — Self-improvement

- [ ] Pattern miner: `attempts` → `patterns`
- [ ] Constraint pre-seeding from similar past tasks
- [ ] Success-rate dashboards per pattern
