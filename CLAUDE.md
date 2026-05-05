# lopi

High-performance Rust agent orchestrator for Claude Code — runs Claude agents concurrently in git-isolated branches with retry loops, SQLite memory, TUI+web dashboard, and Telegram/WhatsApp remote control.

## Stack
Rust 2021 · tokio · axum · ratatui · sqlx/SQLite · teloxide · git2 · clap

## Commands
```bash
cargo build                    # build workspace
cargo test                     # run all crate tests
cargo clippy -- -D warnings    # lint
cargo run -- run --goal "fix foo" --repo .  # run a task
cargo run -- sail              # web dashboard on :3000
cargo run -- watch             # TUI dashboard
```

## Critical Constraints
- No `unwrap()`/`expect()` outside tests — use `anyhow::Result` and `?`
- No blocking I/O on async paths — use `spawn_blocking` for synchronous ops
- No silent failures — log via `tracing::warn!` if a fallback swallows an error
- `cargo build` must stay green — fix before doing anything else
- Stay inside `crates/` and `src/` — never touch root `Cargo.lock` deliberately
- Tokio is the only async runtime — never introduce another

## Crate Map
| Crate | Role |
|-------|------|
| `lopi-core` | Shared types: `Task`, `AgentRun`, `Score`, `LopiConfig` |
| `lopi-git` | `GitManager` (branch/rollback/PR) + `DiffChecker` |
| `lopi-agent` | Plan → Implement → Test → Score → Retry → PR |
| `lopi-memory` | SQLite via sqlx |
| `lopi-orchestrator` | `AgentPool` + priority `TaskQueue` |
| `lopi-ui` | ratatui dashboard + axum web/JSON API |
| `lopi-remote` | teloxide Telegram bot + Twilio WhatsApp |
| `lopi-webhook` | GitHub CI-failure → task injection |
| `lopi-toon` | TOON (Token-Oriented Object Notation) |

## Skills
See `.claude/skills/` — auto-loaded when relevant.
Run `/konjo` to boot a full session (Brief + Discovery + Plan).
