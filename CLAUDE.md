# lopi

High-performance Rust agent orchestrator for Claude Code — runs Claude agents concurrently in git-isolated branches with retry loops, SQLite memory, TUI+web dashboard, and Telegram/WhatsApp remote control.

## Stack
Rust 2021 · tokio · axum · ratatui · sqlx/SQLite · teloxide · git2 · clap

## Commands
```bash
cargo build                    # build workspace (also installs git hooks via cargo-husky)
cargo test --workspace         # run all crate tests (the standard runner — what CI + hooks use)
cargo nextest run              # optional faster runner; install first: cargo install cargo-nextest
cargo clippy -- -D warnings    # lint
cargo llvm-cov nextest         # tests + coverage report (needs cargo-nextest + cargo-llvm-cov)
cargo audit                    # security advisory check
cargo deny check               # license + advisory + bans
cargo run -- run --goal "fix foo" --repo .  # run a task
cargo run -- sail              # web dashboard on :3000
cargo run -- watch             # TUI dashboard
bash .konjo/scripts/install-hooks.sh        # install pre-commit hooks
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
| `lopi-context` | KV cache eviction layer — owns all message history + eviction policies |
| `lopi-git` | `GitManager` (branch/rollback/PR) + `DiffChecker` |
| `lopi-agent` | Plan → Implement → Test → Score → Retry → PR |
| `lopi-memory` | SQLite via sqlx |
| `lopi-orchestrator` | `AgentPool` + priority `TaskQueue` |
| `lopi-ui` | ratatui dashboard + axum web/JSON API |
| `lopi-remote` | teloxide Telegram bot + Twilio WhatsApp |
| `lopi-webhook` | GitHub CI-failure → task injection |
| `lopi-toon` | TOON (Token-Oriented Object Notation) |
| `lopi-ratelimit` | Rate limiting primitives |

## Quality Framework
This repo runs the **Konjo Three-Wall Quality Framework**. See `KONJO_QUALITY_FRAMEWORK.md`.

- **Wall 1** (pre-commit): `bash .konjo/scripts/install-hooks.sh` — installs `.konjo/hooks/pre-commit`
- **Wall 2** (CI): `.github/workflows/konjo-gate.yml` — coverage ≥ 80%, mutation ≤ 10%, complexity ≤ 15, dead code = 0, zero undocumented public APIs
- **Wall 3** (adversarial review): `claude-opus-4-6` reviews every PR against 10 mandatory questions

### Additional Hard Rules (enforced by CI — not in global CLAUDE.md)
- Coverage ≥ 80% (hard block); target ≥ 95%
- Zero cognitive complexity > 15 per function (`clippy::cognitive_complexity`)
- Zero dead code (`RUSTFLAGS="-W dead_code" cargo check`)
- Zero undocumented public APIs (`RUSTDOCFLAGS="-D missing_docs" cargo doc`)
- Function body ≤ 50 lines (30 target) — split before hitting 40
- File ≤ 500 lines (300 target) — create a new module before hitting 400
- No duplicate blocks > 10 lines at > 85% similarity (`dry_check.py`)
- `cargo audit` zero advisories; `cargo deny check` zero violations

## Live Dashboard (Browser Pane)
When asked to check on running stacks/tasks ("what's lopi running right now", "show me the stacks"), in a Claude Code Desktop session with a Browser pane:
1. Check whether `lopi sail` is already running before starting a new one — `lsof -iTCP:<port>` (port from `lopi.toml`, default `3000`) or `ps aux | grep "lopi sail"`. Reuse the running instance and its `--repo` target; don't spawn a duplicate.
2. If nothing is running, start it: `cargo run -- sail --repo <path>` (as a background process).
3. Open the dashboard with the Browser pane's `preview_start` tool using `{url: "http://localhost:<port>"}`. This step is required every time — the Browser pane does **not** auto-detect an already-running `lopi sail` process the way it detects a typical `npm run dev` server, since it's a Rust binary outside the usual JS dev-server patterns.

## Skills
See `.claude/skills/` — auto-loaded when relevant.
Run `/konjo` to boot a full session (Brief + Discovery + Plan).
Run `/konjo-quality` for full gate reference.
Run `/konjo-retrofit` to apply the framework to another repo.
