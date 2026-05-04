# CLAUDE.md — lopi

> 🚢 **lopi** — high-performance Rust agent orchestrator for Claude Code.
> Built by KonjoAI. CLI poetry: `lopi run`, `lopi watch`, `lopi tail`, `lopi dock`, `lopi sail`.

## The Konjo Way

**KONJO — Know, Outline, Nail, Justify, Optimize.**

The Konjo Way: Know the problem, outline the solution, nail the build, justify the claims, optimize the output. Plan, build, test, ship, rest, repeat.

## What lopi is

A Rust-based self-improving agent orchestrator. It runs Claude Code agents concurrently, each in git-isolated branches, with retry loops, persistent memory, a TUI/web dashboard, and phone control via Telegram. Think OpenClaw rebuilt in Rust on tokio: safer, faster, fully KonjoAI.

## Crate map

| Crate | Role |
|-------|------|
| `lopi-core` | Shared types: `Task`, `AgentRun`, `Score`, `LopiConfig` |
| `lopi-git` | `GitManager` (branch/rollback/PR) + `DiffChecker` (off-limits globs) |
| `lopi-agent` | The Plan→Implement→Test→Score→Retry→PR loop |
| `lopi-memory` | SQLite via sqlx — `tasks`, `attempts`, `patterns` |
| `lopi-orchestrator` | `AgentPool` (Semaphore) + `TaskQueue` (priority + dedup) |
| `lopi-ui` | ratatui dashboard + axum web/JSON API |
| `lopi-remote` | teloxide Telegram bot + Twilio WhatsApp webhook |
| `lopi-webhook` | GitHub webhooks (CI failure → task injection) |

## Dev commands

```bash
cargo build               # build the workspace
cargo test                # run all crate tests
cargo run -- run --goal "fix flaky test in foo" --repo .
cargo run -- dock         # list recent tasks
cargo run -- watch        # TUI dashboard
cargo run -- sail         # web dashboard on :3000
```

## CLI verbs

- `lopi run --goal <g> --repo <p>` — run one task to completion
- `lopi watch` — live TUI of in-flight tasks
- `lopi tail [--task-id <id>]` — tail logs from one or all tasks
- `lopi dock` — list every task and its status
- `lopi sail [--port 3000]` — start web dashboard + JSON API

## Phase plan

- **Phase 1 (MVP)** — single agent loop, real git isolation, sqlite memory, CLI ✅
- **Phase 2 (Concurrency + UI)** — bounded `AgentPool`, ratatui live view, axum dashboard
- **Phase 3 (Remote)** — Telegram bot, GitHub webhook fix-on-failure, WhatsApp inbound
- **Phase 4 (Self-improvement)** — pattern memory, retry strategies tuned from history
