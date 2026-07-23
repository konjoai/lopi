# ⛵️ lopi

**ᨒᨚᨄᨗ** — *lopi*: Konjo for "boat." Load it, sail it, trust it, bring it home.

> Multi-agent Claude Code orchestrator, written in Rust. Runs concurrent
> agents in git-isolated branches, with retry loops, persistent memory,
> a TUI + web dashboard, a native macOS app, and remote control over
> Telegram/WhatsApp.
>
> By [KonjoAI](https://github.com/konjoai) · MIT licensed · `v0.22.0`
> [![crates.io](https://img.shields.io/crates/v/lopi.svg)](https://crates.io/crates/lopi)

```
lopi run     # queue and run a task
lopi watch   # live TUI of in-flight agents
lopi sail    # start the web dashboard (the Forge)
lopi tail    # stream logs
lopi dock    # list every task
```

---

## What lopi is

You give lopi a goal — a failing test, a CI red, "fix the lint warnings in
`src/auth/`." lopi hands it to a Claude Code agent, isolates the work on its
own branch, and runs it through a scored loop: **Plan → Implement → Test →
Score → Retry → PR**. Multiple agents run concurrently, each sandboxed to its
own git branch and worktree, with automatic rollback if anything goes wrong.

It's the Rust successor to the OpenClaw-style single-agent Python prototype
— rebuilt for concurrency, safety, and persistence. See
[`LOPI_VS_OPENCLAW.md`](./LOPI_VS_OPENCLAW.md) for the full feature-by-feature
comparison.

**Runs on your existing Claude subscription.** lopi drives the official
`claude` CLI as a subprocess (`claude -p`) and deliberately scrubs any
inherited `ANTHROPIC_API_KEY`/`ANTHROPIC_BASE_URL` env vars before every
spawn, so the CLI always falls back to your on-disk `~/.claude` subscription
credentials rather than silently billing an API key. No separate API key is
required to run agents. (A couple of standalone server components — see
[Configuration](#configuration) — can *optionally* take a direct Anthropic
API key for their own use, but the core agent loop never needs one.)

## Highlights

- **Concurrent, git-isolated agents** — an `AgentPool` runs N agents at once
  (default 4), each on its own `orka/<task_id>/<attempt>` branch and worktree.
  Off-limits path globs, a max-diff-line cap, and automatic hard rollback on
  any safety violation keep runs contained.
- **Scored retry loop** — a weighted composite score (tests + lint + diff
  size) decides accept / retry / rollback. Failed attempts feed a SQLite
  pattern library so later re-plans are seeded with what already failed.
  Optional Reflexion-style adaptive retry and a Layer 5 stability gate
  (variance-checked plan sampling) are available per run.
- **Granular budget controls** — per-run USD caps (`--budget`), named
  presets (quick/standard/deep/unlimited), token budgets, and a repo-level
  `.lopi/loop.toml` — because an unwired budget is how a session turns into
  a bill you didn't expect.
- **Three UI surfaces** — a `ratatui` terminal TUI (`lopi watch`), a
  SvelteKit web dashboard ("the Forge," served by `lopi sail`), and a native
  SwiftUI **macOS app** (in [`macos/`](./macos)) that talks to the same
  REST + WebSocket API.
- **Remote control from your phone** — a Telegram bot and WhatsApp (via
  Twilio) with an auth allowlist, phase-by-phase push notifications, and
  inline approve/reject buttons for opened PRs.
- **Event-driven, not just manual** — a GitHub webhook listener turns CI
  failures into auto-queued fix tasks; cron-style schedules
  (`[[schedules]]` in `lopi.toml`, editable live from the dashboard) handle
  recurring work like nightly lint sweeps.
- **Ships into Claude itself** — a Claude Code plugin (`lopi mcp-serve`)
  and a Claude Desktop extension (MCPB, `mcpb/`) expose task submission and
  live stack status as MCP tools, with a clickable status widget inside
  Claude Desktop.
- **Self-improving** — `lopi spec` extracts a repo's spec surface from its
  tests, `lopi gap-fill` turns coverage gaps into queued fix tasks, and
  `lopi skill promote` turns recurring lessons into reviewable `SKILL.md`
  drafts.

## Requirements

| To… | You need |
|---|---|
| Run agents at all | The [`claude` CLI](https://docs.claude.com/en/docs/claude-code), logged into a Claude subscription (Pro, Max, Team, or Enterprise) — lopi drives it as a subprocess and never needs a separate API key |
| Build from source | A stable Rust toolchain (edition 2021; verified with 1.89–1.94) and `git` |
| Build the Forge web dashboard | Node.js 18+ and npm — optional; without it, `lopi sail` serves a placeholder page instead |
| Build the native macOS app | Xcode 15+ and XcodeGen, macOS 14+ (only needed for [`macos/`](./macos)) |

lopi itself runs on macOS or Linux; the native app is macOS-only. No
database server, no Docker, no cloud account — everything (SQLite, git
worktrees, the web dashboard) is local to the machine you run it on.

## Quickstart

Via Homebrew (macOS/Linux):

```bash
brew install konjoai/lopi/lopi
```

Via [crates.io](https://crates.io/crates/lopi):

```bash
cargo install lopi
```

From source:

```bash
git clone https://github.com/konjoai/lopi.git
cd lopi
cargo build --release
cp lopi.toml.example lopi.toml
./target/release/lopi run --goal "fix the failing test in src/foo.rs" --repo .
```

Building the web dashboard is optional but recommended (`cd web && npm
install && npm run build`) — without it, `lopi sail` serves a placeholder
page instead of the Forge UI (the Homebrew formula builds it for you; `cargo
install` ships without it — build from source if you want the Forge UI from
a `cargo install`). See [`docs/RUNNING.md`](./docs/RUNNING.md) for the full
build/run guide, including the native macOS app.

## CLI

`lopi` is a single binary. Bare `lopi` (no args) drops into an interactive
REPL. The full surface:

| Command | What it does |
|---|---|
| `lopi run --goal "<g>" --repo <path>` | Run one agent task, stream status to stdout |
| `lopi bypass <goal…>` | Run with directory restrictions disabled (trusted envs only) |
| `lopi watch` | Live TUI — agent status (`--remote <ws>` or `--local`) |
| `lopi sail` | Web dashboard + agent pool (single- or multi-repo) |
| `lopi tail` / `lopi dock` | Stream events / list all tasks |
| `lopi cancel <id>` / `lopi resume --agent-id <id>` | Cancel / resume a task |
| `lopi learn` / `lopi stability` / `lopi trust` | Browse mined patterns / stability ledger / trust stats |
| `lopi schedules list` | Scheduled tasks + next run times |
| `lopi loop show\|validate --repo <path>` | Inspect / validate a repo's `.lopi/loop.toml` |
| `lopi worktree list\|gc` | Manage per-task git worktrees |
| `lopi skill promote` | Promote recurring lessons into skill drafts |
| `lopi gap-fill` / `lopi spec` / `lopi check` | Test-driven fix queue / spec surface / KCQF quality gate |
| `lopi replay --task <id>` | Inspect a task's DAG trace and replay from a given stage |
| `lopi diag` | Export a diagnostic snapshot (tasks, logs, audit, stability) as committable JSON |
| `lopi mcp-serve` | Serve lopi's MCP tools over stdio — the Claude Code / Desktop entry point |
| `lopi serve-webhooks` | Standalone GitHub webhook server (CI-failure → task injection) |
| `lopi serve-app` | GitHub App OAuth + Stripe webhook server |

## Architecture

17 crates in a Cargo workspace:

| Crate | Role |
|---|---|
| `lopi-core` | Shared types: `Task`, `Score`, `LopiConfig` |
| `lopi-agent` | Claude Code subprocess wrapper, retry runner, and scoring |
| `lopi-context` | Token-budget context window with phase-aware eviction |
| `lopi-git` | Branch management + path diff validation for agent runs |
| `lopi-memory` | SQLite-backed store for tasks, patterns, turn metrics, lessons |
| `lopi-orchestrator` | Concurrent agent pool, priority task queue, scheduler |
| `lopi-ui` | `ratatui` TUI + `axum` web/JSON API (the Forge) |
| `lopi-remote` | Telegram bot + Twilio WhatsApp webhook |
| `lopi-webhook` | GitHub webhook receiver — CI-failure/PR/issue triage → tasks |
| `lopi-mcp` | MCP client — lopi agents discovering and calling external tools |
| `lopi-tools` | Durable tool registry (specs, timeouts, retry budgets) |
| `lopi-skill` | Runtime registry of `SKILL.md` project knowledge |
| `lopi-spec` | Spec surface extractor (tests → machine-readable coverage inventory) |
| `lopi-ratelimit` | Token-bucket rate limiting + Anthropic concurrency controls |
| `lopi-toon` | Token-Oriented Object Notation encoder (~40% fewer tokens than JSON) |
| `lopi-app` | Standalone GitHub App OAuth + Stripe webhook server |
| `lopi-github` | Thin GitHub REST client for write operations (PRs, labels, comments) |

Plus [`macos/`](./macos) (native SwiftUI dashboard), [`web/`](./web)
(SvelteKit source for the Forge), [`mcpb/`](./mcpb) (Claude Desktop
extension), and [`plugin/`](./plugin) (Claude Code plugin manifest).

## Configuration

Copy `lopi.toml.example` to `lopi.toml` and edit. Key sections:

- `[lopi]` — max concurrent agents, log level, SQLite DB path
- `[claude]` — `claude` CLI path and per-call timeout
- `[git]` — allowed/forbidden directories, auto-PR toggle
- `[remote.telegram]` / `[remote.whatsapp]` — bot token / Twilio credentials
- `[web]` — dashboard host/port
- `[[schedules]]` — cron-style recurring tasks (also editable live from the dashboard)

Per-repo budget and safety policy lives in `.lopi/loop.toml` (see `lopi loop
show|validate`). Two components — `lopi serve-webhooks` and `lopi serve-app`
— accept an optional standalone `ANTHROPIC_API_KEY` for their own automated
triage; this is separate from, and unrelated to, how the core agent loop
authenticates.

## Safety

- Git-isolated branches per attempt, auto-deleted on rollback; base branch
  is never touched.
- `DiffChecker`: off-limits glob patterns, max-diff-line cap, full path scan
  before any change is accepted.
- `allow_self_modify: false` by default — lopi's own `src/` and `crates/`
  are off-limits.
- Hard rollback (`git reset --hard`) on any safety violation; no retry
  allowed after a violation.
- lopi never auto-merges. Every PR requires human review — from the
  dashboard, the macOS app, or a Telegram approve button.

## Contributing / feedback

Issues and PRs welcome at [github.com/konjoai/lopi](https://github.com/konjoai/lopi).

## License

MIT © KonjoAI
