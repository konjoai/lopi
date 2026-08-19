# lopi

High-performance Rust agent orchestrator for Claude Code — runs Claude agents concurrently in git-isolated branches with retry loops, SQLite memory, TUI+web dashboard, and a WhatsApp/Twilio webhook.

## Org rules

@~/.konjo/kiban/plugins/konjo/skills/konjo/SKILL.md

The org ethos applies here: ship over optimize, kill-test first, statistical rigor,
honest negative results, evidence first, token-efficient context.

Editorial rules: no em dashes, no AI-tell vocabulary. The prose lint enforces it; run
`konjo-prose` on docs before pushing.

Log durable decisions with `konjo-decision decide` at `repo:lopi` scope. Search with
`konjo-decision search` before reopening a settled call.

When you catch a mistake worth not repeating, invoke `correct`: it records a learning with
`konjo-learn` and proposes the smallest durable fix. A learning must name where its rule
lives (a CLAUDE.md line, a prose-lint word, a lane, or a gate), or it is refused.

Build the Konjo way: the `craft` skill carries the four behaviors (think before coding,
simplicity first, surgical changes, goal-driven execution) plus the verify-loop and the
pre-implementation trust-boundary contract. `verify_cmd` is declared in
`.konjo/profile.yml`.

## Stack
Rust 2021 · tokio · axum · ratatui · sqlx/SQLite · git2 · clap

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

## Invariants

Each bullet names its enforcing gate and that gate's tier (Gate-Tiering-1 — see
`LEDGER.md` — BLOCKING blocks merge, ADVISORY reports and never blocks). A bullet with no
dedicated mechanical gate says so explicitly as ADVISORY convention-only, rather than
silently implying coverage it doesn't have.

- No `unwrap()`/`expect()` outside tests (enforced: `static` → `repo:clippy` — `-D clippy::unwrap_used -D clippy::expect_used` — BLOCKING)
- No blocking I/O on async paths — use `spawn_blocking` for synchronous ops (ADVISORY — no dedicated gate, convention only)
- No silent failures — log via `tracing::warn!` if a fallback swallows an error (ADVISORY — no dedicated gate, convention only)
- `cargo build` must stay green — fix before doing anything else (ADVISORY — no standalone gate; a build failure surfaces as a failure in `static`'s `repo:clippy`/`repo:fmt-check` steps or `coverage`'s BLOCKING test step, not a dedicated gated diff assertion of its own)
- Stay inside `crates/` and `src/` — never touch root `Cargo.lock` deliberately (ADVISORY — no dedicated gate, convention only)
- Tokio is the only async runtime — never introduce another (ADVISORY — no dedicated gate, convention only)
- No unconfigured or failed-evaluation branch returns a permissive value (enforced: `konjo-gates` → `gate_polarity` — ADVISORY, standing baseline recorded in `LEDGER.md`)

Gate thresholds (coverage, complexity, dead code, docs, DRY, file size) are declared once
in `.konjo/profile.yml`'s `contract_gates`, not duplicated here — see that file for the
current list and `konjo-gates` (wired in `konjo-gate.yml`, ADVISORY tier as a whole job —
see `LEDGER.md`'s `Gate-Tiering-1` entry) for what's mechanically double-checked today
versus kept repo-native in `konjo-gate.yml`'s own G0-G5 jobs. The aggregator's own
BLOCKING/ADVISORY split per job is in `.konjo/scripts/gate_verdict.sh` and
`LEDGER.md`'s `Gate-Tiering-1` entry, not restated per-bullet here.

## Repo map
| Crate | Role |
|-------|------|
| `lopi-core` | Shared types: `Task`, `AgentRun`, `Score`, `LopiConfig` |
| `lopi-context` | KV cache eviction layer — owns all message history + eviction policies |
| `lopi-git` | `GitManager` (branch/rollback/PR) + `DiffChecker` |
| `lopi-agent` | Plan → Implement → Test → Score → Retry → PR |
| `lopi-memory` | SQLite via sqlx |
| `lopi-orchestrator` | `AgentPool` + priority `TaskQueue` |
| `lopi-ui` | ratatui dashboard + axum web/JSON API |
| `lopi-remote` | Twilio WhatsApp webhook handler. Not reachable from the built binary — the Telegram transport was removed in Sprint S10 Phase 4 and the gateway question is settled (`LEDGER.md`'s `Telegram-Gateway-Non-Goal`) |
| `lopi-webhook` | GitHub CI-failure → task injection |
| `lopi-toon` | TOON (Token-Oriented Object Notation) |
| `lopi-ratelimit` | Rate limiting primitives. `BudgetGovernor` is unwired — the orchestrator owns budgets in `lopi-orchestrator/src/budget/` |
| `lopi-oracle` | Textual cross-agent collision detection via `git merge-tree`. Advisory, detection-only |
| `lopi-index` | Symbol index + code map |
| `lopi-mcp` | MCP server/bridge surface |
| `lopi-skill` | Runtime skill registry + promotion |
| `lopi-spec` | Spec surface |
| `lopi-tools` | Shared tool helpers |
| `lopi-github` | GitHub REST client (write path) |
| `lopi-demo` | Demo/scripted-run fixtures |

## Repo-specific rules

### Live Dashboard (Browser Pane)
When asked to check on running stacks/tasks ("what's lopi running right now", "show me the stacks"), in a Claude Code Desktop session with a Browser pane:
1. Run `scripts/start-dashboard.sh --repo <path>` — it checks `/api/health` on the target port (from `lopi.toml`, default `3000`) and no-ops with an "already running" message if `sail` is up, so it's always safe to run instead of hand-checking with `lsof`/`ps`.
2. If nothing was running, the script starts `lopi sail` backgrounded and waits until it's healthy before returning.
3. Open the dashboard with the Browser pane's `preview_start` tool using `{url: "http://localhost:<port>"}`. This step is required every time — the Browser pane does **not** auto-detect an already-running `lopi sail` process the way it detects a typical `npm run dev` server, since it's a Rust binary outside the usual JS dev-server patterns.

### Skills
See `.claude/skills/` — auto-loaded when relevant. `konjo-ship` comes from the global
kiban clone; a local `.claude/skills/konjo-ship/` copy would shadow it — do not re-add one.
Run `/konjo` to boot a full session (Brief + Discovery + Plan).
Run `/konjo-quality` for full gate reference.
Run `/konjo-retrofit` to apply the framework to another repo.

## Pinning

This repo pins a kiban ref in `.konjo/kiban.ref` (currently `v1.19.0`) and `KIBAN_REF` in
`.github/workflows/konjo-gate.yml`'s `doc-staleness`, `konjo-gates`, and `mutation-hunt`
jobs (four pin sites total) — bump all four together; a kiban change should not silently
reach any of the three gates.
