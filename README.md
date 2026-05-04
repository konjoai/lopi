# ⛵️ lopi

> High-performance Rust agent orchestrator for Claude Code.
> By [KonjoAI](https://github.com/konjoai).

```
lopi run     # queue and run a task
lopi watch   # live TUI of in-flight agents
lopi tail    # stream logs
lopi dock    # list every task
lopi sail    # start web dashboard
```

## What lopi is

A self-improving agent orchestrator written in Rust. It runs Claude Code agents concurrently, each in a git-isolated branch, with retry loops, persistent SQLite memory, a TUI + web dashboard, and remote control over Telegram + WhatsApp.

Think OpenClaw rebuilt on tokio: safer, faster, fully KonjoAI.

## The Konjo Way

**KONJO — Know, Outline, Nail, Justify, Optimize.**

Know the problem, outline the solution, nail the build, justify the claims, optimize the output. Plan, build, test, ship, rest, repeat.

## Quickstart

```bash
git clone https://github.com/konjoai/lopi.git
cd lopi
cargo build --release
cp lopi.toml.example lopi.toml
./target/release/lopi run --goal "fix the failing test in src/foo.rs" --repo .
```

## Architecture

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

## Configuration

Copy `lopi.toml.example` to `lopi.toml` and edit. Supports max-agent count, log level, Claude CLI path/timeout, allowed/forbidden dirs, auto-PR toggle, Telegram credentials, and web host/port.

## License

MIT © KonjoAI
