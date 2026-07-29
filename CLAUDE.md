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
scripts/start-dashboard.sh     # same, but idempotent — checks /api/health first, no-ops if already up
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
- **Wall 2** (CI): `.github/workflows/konjo-gate.yml` — coverage floor never regresses (see below, not 80%/95% — that step is soft), mutation ≤ 10%, complexity ≤ 15, dead code = 0; doc coverage is checked but soft
- **Wall 3** (adversarial review): `claude-opus-4-6` reviews every PR against 10 mandatory questions

### Additional Hard Rules (enforced by CI — not in global CLAUDE.md)
Each bullet below is honest about its actual `konjo-gate.yml` job:step per the S13 Phase 0
audit (`.konjo/killtests/S13/`) — a bullet with no genuine enforcing step is stated as such
rather than left to imply one exists.
- Coverage never regresses below the locked floor in `.konjo/coverage-floor.txt` — `coverage:"Coverage floor gate"`, hard. The 80%/95% step (`coverage:"Coverage gate"`) is `continue-on-error: true` — soft, not a hard block, despite the name.
- Zero cognitive complexity > 15 per function — `complexity:"Cognitive complexity gate (clippy)"`, hard.
- Zero dead code — `static:"dead code — zero tolerance"`, hard.
- Zero undocumented public APIs — `complexity:"Documentation gate (rustdoc)"` runs `-D missing_docs` but is `continue-on-error: true` — **soft only**, known doc-link debt in `lopi-agent`/`lopi-orchestrator`.
- Function body ≤ 50 lines (30 target) — **no mechanical gate exists.** Only a WARNING-tier question (Q7) in the Wall 3 LLM review, which cannot block merge.
- File ≤ 500 lines (300 target) — `complexity:"File size gate"`, hard, but scoped to `*.rs`/`*.py` only; `web/` (TS/Svelte) and `macos/` (Swift) are not covered.
- No duplicate blocks > 20 lines at > 85% similarity (`dry_check.py`) — `complexity:"DRY check"`, hard. CI's actual threshold is 20 lines, not 10.
- `cargo audit` zero advisories; `cargo deny check` zero violations — `static` job, both hard.

## Live Dashboard (Browser Pane)
When asked to check on running stacks/tasks ("what's lopi running right now", "show me the stacks"), in a Claude Code Desktop session with a Browser pane:
1. Run `scripts/start-dashboard.sh --repo <path>` — it checks `/api/health` on the target port (from `lopi.toml`, default `3000`) and no-ops with an "already running" message if `sail` is up, so it's always safe to run instead of hand-checking with `lsof`/`ps`.
2. If nothing was running, the script starts `lopi sail` backgrounded and waits until it's healthy before returning.
3. Open the dashboard with the Browser pane's `preview_start` tool using `{url: "http://localhost:<port>"}`. This step is required every time — the Browser pane does **not** auto-detect an already-running `lopi sail` process the way it detects a typical `npm run dev` server, since it's a Rust binary outside the usual JS dev-server patterns.

## Skills
See `.claude/skills/` — auto-loaded when relevant.
Run `/konjo` to boot a full session (Brief + Discovery + Plan).
Run `/konjo-quality` for full gate reference.
Run `/konjo-retrofit` to apply the framework to another repo.
