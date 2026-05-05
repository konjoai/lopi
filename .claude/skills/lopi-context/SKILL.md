---
name: lopi-context
description: Full lopi project context — complete phase plan, current health metrics, standing rules for agents, crate details. Auto-load when working on lopi sprint planning, architecture decisions, or Phase 5+ work.
user-invocable: false
---
# lopi — Full Project Context

## Phase Plan

| Phase | Status | What Shipped |
|-------|--------|--------------|
| 1 — MVP Core | ✅ v0.1.0 | Cargo workspace, all 8 crates, CLI verbs |
| 2 — N Parallel Agents + Live Dashboard | ✅ v0.4.0 | AgentPool, EventBus, ratatui TUI, web dashboard, WebSocket |
| 3 — Remote Control + Pattern Mining | ✅ v0.3.0 | Telegram bot, GitHub webhook HMAC, pattern miner |
| 4 — Scheduled Tasks + Repo Profiles | ✅ v0.5.0 | cron scheduler, RepoProfile, `lopi watch --remote` |
| 5 — Self-Improvement Engine | 🔲 Next | Pattern learning, failure post-mortem, adaptive retry |
| 6 — GitHub Webhooks + CI Integration | 🔲 Planned | lopi-webhook end-to-end, GitHub App mode |
| 7 — Production Web UI | 🔲 Planned | React/Svelte frontend, auth, mobile-responsive |
| 8 — Native Mobile App | 🔲 Planned | React Native, push notifications, voice input |
| 9 — Intelligence + Evolution | 🔲 Ongoing | Multi-agent roles, goal decomposition, embedding memory |

## Current Health (Phase 4 / v0.5.0)
- Tests: 46 passing, 0 failing
- Build: Clean (0 warnings)
- Crates: 9 (lopi-core, lopi-git, lopi-agent, lopi-memory, lopi-orchestrator, lopi-ui, lopi-remote, lopi-webhook, lopi-toon)
- CLI: `run`, `watch`, `tail`, `dock`, `sail`, `schedules`
- API: GET /api/tasks, POST /api/tasks, GET /api/tasks/:id, GET /api/patterns, GET /api/health, GET /ws/tasks

## Phase 5 — Self-Improvement Engine (Next Sprint)

Priority items:
1. **Pattern learning**: before running a new task, query similar past tasks → suggest constraints that worked → pre-load into system prompt
2. **`lopi learn` CLI** — show pattern library, success rates, top constraints
3. **Failure post-mortem**: when all retries fail, run a "post-mortem" Claude session → analyze error log → generate constraint/approach suggestion → store as pattern
4. **Adaptive retry**: if attempt N failed with error type X, adjust prompt strategy for attempt N+1 (pass error + suggest different approach)
5. **Self-modification loop (guarded)**: `allow_self_modify = true` in config + same git isolation + PR workflow applies
6. **Scoring evolution**: score weights configurable and tunable based on which metrics correlate with user-approved vs rejected PRs

## Standing Rules for Agents on lopi
- Stay inside `crates/` and `src/` — never touch `.github/`, `infra/`, root `Cargo.lock` deliberately
- `cargo build` must stay green — if it goes red, fix before doing anything else
- No `unwrap()` outside tests — use `anyhow::Result` and `?`
- No silent failure — if a fallback path swallows an error, log via `tracing::warn!`
- Async everywhere — Tokio is the only runtime; no blocking I/O on async paths (use `spawn_blocking`)

## Key Architecture Decisions (Load-Bearing)
- `EventBus<TaskStatus>` uses `tokio::broadcast` — subscriber lag drops old events; this is intentional
- `AgentPool` uses `Arc<Semaphore>` for bounded concurrency — no global mutex on the task queue
- `lopi-memory` uses `sqlx` with SQLite + WAL mode — concurrent readers are fine, one writer at a time
- Pattern mining: keyword fingerprint extraction via `find_similar_patterns(goal)` — not embedding-based yet (Phase 9)
- GitHub webhook HMAC: constant-time comparison via `subtle::ConstantTimeEq` — do not simplify to `==`
