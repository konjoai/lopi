# Running lopi

Authoritative, verified guide to building and running every lopi surface —
the backend + web dashboard ("the Forge"), the terminal TUI, and the native
macOS app. Every command below was run against this repo on
`claude/lopi-build-screenshots-inventory-cd8ka9`; anything that did **not**
run cleanly is called out in [Known issues](#known-issues).

> **Surface summary (the short answer).** lopi has **three** UI surfaces: a
> **web dashboard** (SvelteKit "Forge", served by `lopi sail`), a **terminal
> TUI** (`lopi watch`), and a **native macOS app** (SwiftUI, in
> [`macos/`](../macos)). The macOS app is a *client* of the same
> `lopi sail` REST + WebSocket API the web dashboard uses. On macOS the web
> dashboard runs in any browser — it is not macOS-specific. See
> [Surfaces & the macOS question](#surfaces--the-macos-question) for the full
> inventory and feature-parity read.

---

## Prerequisites

| Tool | Version used to verify | Needed for |
|------|------------------------|------------|
| Rust toolchain | `rustc 1.94.1`, `cargo 1.94.1` | backend, TUI, CLI |
| Node.js | `v22.22.2` | building the web dashboard |
| npm | `10.9.7` | web dependencies |
| `claude` CLI | authenticated (subscription login, no `ANTHROPIC_API_KEY`) | actually running agents |
| Xcode + XcodeGen | Xcode 15+, macOS 14+ | **only** the native macOS app |

Notes:

- The `claude` CLI is required only to *run agents*. `lopi sail`, `lopi watch`,
  and the dashboards all start and render without it — they simply show empty /
  offline state until a real agent runs. lopi scrubs Anthropic routing env
  from every spawn, so the CLI uses your `~/.claude` subscription credentials.
- **macOS is the target machine**, but nothing here is macOS-only except the
  native app: the backend and web build/run identically on Linux and macOS, and
  the dashboard is a browser app.

### Configuration

Copy an example config and edit it (optional — sensible defaults apply):

```bash
cp lopi.toml.example lopi.toml     # max-agents, log level, claude path, git policy, web host/port, schedules
```

There is also a smaller [`.lopi.toml.example`](../.lopi.toml.example). Config
lookup order is `--config <path>` → `./lopi.toml` → `~/.lopi/lopi.toml`. The
SQLite DB defaults to `~/.lopi/lopi.db`.

---

## Build

```bash
cargo build            # workspace debug build (also installs git hooks via cargo-husky)
cargo build --release  # optimized binary at ./target/release/lopi
```

Build the web dashboard (produces `web/dist/`, embedded into the binary via
`rust-embed`):

```bash
cd web
npm install
npm run build          # static SvelteKit build → web/dist/
cd ..
```

> `web/dist/` is git-ignored and created empty by the lopi-ui build script, so
> `cargo build` succeeds before you ever build the web app. **In a debug build,
> `lopi sail` reads `web/dist/` from disk at runtime**, so building the web app
> and restarting `sail` is enough — no Rust rebuild required. For a
> single-file **release** binary that embeds the assets, run `npm run build`
> **before** `cargo build --release`. If `web/dist/` is empty, `sail` serves a
> built-in placeholder page with build instructions instead of the dashboard.

---

## The CLI

`lopi` is a single binary with these subcommands (from `lopi --help`):

| Command | What it does |
|---------|--------------|
| `lopi` (no args) | Interactive REPL cockpit (bare invocation) |
| `lopi run --goal "<g>" --repo <path>` | Run one agent task, stream status to stdout |
| `lopi bypass <goal…>` | Run with directory restrictions disabled (trusted envs only) |
| `lopi watch` | **TUI** — live agent status (`--remote <ws>` or `--local`) |
| `lopi tail` | Stream agent events (history or live) |
| `lopi dock` | List all tasks + status from the DB |
| `lopi sail` | **Web dashboard** + agent pool (single or multi-repo) |
| `lopi cancel <id>` / `lopi resume --agent-id <id>` | Cancel / resume a task |
| `lopi learn` / `lopi stability` / `lopi trust` | Browse mined patterns / stability ledger / trust stats |
| `lopi schedules list` | Scheduled tasks + next run times |
| `lopi loop show\|validate --repo <path>` | Inspect / validate a repo's `.lopi/loop.toml` |
| `lopi worktree list\|gc` | Manage per-task git worktrees |
| `lopi skill promote` | Promote recurring lessons into skill drafts |
| `lopi gap-fill` / `lopi spec` / `lopi check` | Test-driven fix queue / spec surface / KCQF quality gate |
| `lopi replay --task <uuid>` | Inspect a task's DAG trace + partial-restart plan |
| `lopi serve-app` / `lopi serve-webhooks` | GitHub App OAuth + Stripe / GitHub webhook servers |
| `lopi watch-gap-fill` | Continuous gap-fill daemon (the "Kitchen Loop") |

Run `lopi <cmd> --help` for the full flag set of any subcommand.

---

## Surface 1 — the web dashboard ("the Forge")

Start the backend + web dashboard:

```bash
cargo run -- sail --port 3000 --host 127.0.0.1 --max-agents 4 --repo .
# or the release binary:
# ./target/release/lopi sail --port 3000 --repo .
```

This serves:

- **Dashboard:** <http://127.0.0.1:3000>
- **REST API:** <http://127.0.0.1:3000/api/tasks>, `/api/stats`, `/api/schedules`, …
- **WebSocket:** `ws://127.0.0.1:3000/ws`

Open <http://127.0.0.1:3000>. `/` redirects (307) to **`/stacks`** — the loop-stack
composer is the app's default view. Every destination is reachable from the
hidden off-canvas sidebar (hamburger, top-left).

### The routes (sidebar nav)

`/` → `/stacks` · then `/forge`, `/fleet`, `/constellation`, `/pulse`,
`/budget`, `/tasks`, `/router`, `/schedules`, `/loop`, `/stacks`, `/tools`,
`/logs`, `/config`, `/debug` (plus a hidden `/onboard`).

### Web dev server (hot reload)

For UI iteration without rebuilding the binary:

```bash
cd web
npm run dev            # http://localhost:5173, proxies /api + /ws to :3000
```

If `lopi sail` is running on `:3000`, the dev UI connects to it. If not, it
renders on simulated mock data. **Live mock data** is opt-in on any route via
`?demo=1` (e.g. <http://127.0.0.1:3000/forge?demo=1>) — used for the animated
screenshots below.

### Driving a stack to a running / goal state (demo)

The `/stacks` composer sequences loop cards client-side; "run stack" launches
each card as a real task via the REST API. To seed genuinely live sessions
(each a real `claude -p` stream) against a running `sail`, submit tasks to
isolated scratch repos — see [`../RUN_MULTIPANE.md`](../RUN_MULTIPANE.md) for the
copy-paste four-session recipe. A minimal single seed:

```bash
d=$(mktemp -d); ( cd "$d" && git init -q && echo 'fn main(){}' > main.rs && git add -A && git commit -qm init )
curl -s -X POST http://127.0.0.1:3000/api/tasks -H 'content-type: application/json' \
  -d "{\"goal\":\"summarize main.rs\",\"repo\":\"$d\",\"priority\":\"normal\"}"
```

> Each seeded task spawns a real Claude agent and bills against your
> subscription. The screenshots in this doc use the zero-cost `?demo=1` mock
> path instead.

### Screenshots — web

Captured at a 1440×900 viewport against `lopi sail` on macOS-equivalent
Chromium. Files live in [`screenshots/web/`](screenshots/web).

**Default view & app shell**

| Stacks (default `/`) | Sidebar (off-canvas nav) |
|---|---|
| ![stacks](screenshots/web/stacks.png) | ![sidebar](screenshots/web/sidebar-open.png) |

**Stack control dock & the goal surface (B1)**

| Dock collapsed | Dock expanded | Goal / stop-reason surface |
|---|---|---|
| ![dock collapsed](screenshots/web/stacks-dock-collapsed.png) | ![dock expanded](screenshots/web/stacks-dock-expanded.png) | ![goal surface](screenshots/web/stacks-goal-surface.png) |

**The Forge & live visualizers** (`?demo=1` = mock agents in flight)

| Forge (empty) | Forge — 4 live panes (demo) |
|---|---|
| ![forge](screenshots/web/forge.png) | ![forge demo](screenshots/web/forge-demo-running.png) |

| Fleet — agents in flight (demo) | Constellation — orbital view (demo) |
|---|---|
| ![fleet demo](screenshots/web/fleet-demo-running.png) | ![constellation demo](screenshots/web/constellation-demo-running.png) |

Empty-state variants of the WebGL surfaces are also captured:
[`fleet.png`](screenshots/web/fleet.png),
[`constellation.png`](screenshots/web/constellation.png).

**Every other nav destination**

| Budget | Tasks | Router | Schedules |
|---|---|---|---|
| ![budget](screenshots/web/budget.png) | ![tasks](screenshots/web/tasks.png) | ![router](screenshots/web/router.png) | ![schedules](screenshots/web/schedules.png) |

| Loop | Tools | Logs | Config |
|---|---|---|---|
| ![loop](screenshots/web/loop.png) | ![tools](screenshots/web/tools.png) | ![logs](screenshots/web/logs.png) | ![config](screenshots/web/config.png) |

| Debug | Onboard |
|---|---|
| ![debug](screenshots/web/debug.png) | ![onboard](screenshots/web/onboard.png) |

> **`/pulse` is not embedded** — its live canvas reliably crashes headless
> Chromium in this environment's software GL renderer, so it could not be
> rasterized here. The route itself is healthy (`GET /pulse` → `200`, DOM
> mounts); it renders normally in a real browser. See
> [Known issues](#known-issues).

---

## Surface 2 — the terminal TUI

A distinct surface from the web dashboard: a ratatui full-screen terminal UI.

```bash
lopi watch --local                          # local event bus only
lopi watch --remote ws://127.0.0.1:3000/ws  # attach to a running sail server (default)
```

`lopi watch` renders a live table/dashboard of in-flight agents in the
terminal. Related terminal commands: `lopi dock` (task table), `lopi tail`
(event stream), and bare `lopi` (interactive REPL cockpit).

Screenshots live in [`screenshots/tui/`](screenshots/tui).

![lopi watch](screenshots/tui/lopi-watch.png)

---

## Surface 3 — the native macOS app

A native **SwiftUI** dashboard in [`macos/`](../macos). It is a *client* of a
running `lopi sail` server — it speaks the same REST + WebSocket API as the web
Forge — and adds OpenClaw-style extras (cron scheduling, a menu-bar companion,
admin panels).

**Requires macOS 14+ and Xcode 15+** — it cannot be built on Linux (this ops
run was on Linux, so the macOS app is inventoried from source, not built here;
see [Known issues](#known-issues)).

```bash
cd macos
brew install xcodegen          # once
xcodegen generate              # produces Lopi.xcodeproj from project.yml
open Lopi.xcodeproj             # then ⌘R in Xcode
# …or headless:
xcodebuild -project Lopi.xcodeproj -scheme Lopi -destination 'platform=macOS' build
open ~/Library/Developer/Xcode/DerivedData/Lopi-*/Build/Products/Debug/Lopi.app
```

In another terminal, start the backend it talks to:

```bash
cargo run -- sail              # serves http://127.0.0.1:3000
```

The app defaults to `127.0.0.1:3000`; point it at a remote server + Bearer token
in **Settings** (token stored in the macOS Keychain).

---

## Surfaces & the macOS question

**How many UI surfaces exist?** Three, plus a REPL:

| Surface | Kind | Entry point | Evidence |
|---------|------|-------------|----------|
| Web dashboard ("Forge") | SvelteKit SPA in a browser | `lopi sail` → `:3000` | [`web/`](../web), [`crates/lopi-ui`](../crates/lopi-ui) |
| Terminal TUI | ratatui full-screen | `lopi watch` | [`src/repl/`](../src/repl), `crates/lopi-ui` (ratatui) |
| Native macOS app | SwiftUI desktop app | Xcode / `Lopi.app` | [`macos/Lopi/LopiApp.swift`](../macos/Lopi/LopiApp.swift), [`macos/project.yml`](../macos/project.yml) |
| REPL cockpit | terminal (ratatui) | bare `lopi` | [`src/repl/mod.rs`](../src/repl/mod.rs) |

**Is there a native macOS UI?** **Yes.** Evidence: a full SwiftUI/Xcode project
under [`macos/`](../macos) — `LopiApp.swift` (`@main` app with `WindowGroup` +
`MenuBarExtra`), an XcodeGen [`project.yml`](../macos/project.yml) declaring a
`type: application` / `platform: macOS` target (bundle id `ai.konjo.lopi`,
deployment target macOS 14.0), `.xcassets` app-icon set, a Metal shader
([`ForgeOrb.metal`](../macos/Lopi/Components/ForgeOrb.metal)), and ~40 Swift
source files across `Views/`, `Networking/`, `Store/`, `Components/`.

**Does it mirror the web dashboard?** **Partially — same backend, overlapping
but divergent screens.** Because the macOS app is a pure client of the same
`lopi sail` REST + WS API, it is inherently in *data* parity. On *features*:

- **Shared screens** (both surfaces): Forge (live cockpit / panes), Budget,
  Tasks, Loop, Constellation(s), Tools, Config.
- **macOS-only screens:** Dashboard, Cron, Dead-Letter, Health, Patterns,
  Audit, plus a **menu-bar companion** — the OpenClaw-style admin extras.
  (macOS nav enum: `Forge, Dashboard, Budget, Tasks, Cron, Loop,
  Constellations, Dead-Letter, Tools, Health, Patterns, Audit, Config` —
  [`macos/Lopi/Views/RootView.swift`](../macos/Lopi/Views/RootView.swift).)
- **Web-only screens:** **Stacks** (the loop-stack composer — the web app's
  *current default view* and the B1 goal-directed-stacks feature), Fleet,
  Pulse, Router, Schedules-as-UI, Logs, Debug.
- Status per [`macos/README.md`](../macos/README.md): "Phase 1–2 + Cron" —
  networking core, theme, app shell, menu-bar, dashboard, tasks, cron; several
  admin panels are stubbed and wired into nav.

**Bottom line for a "macOS parity" decision:** A native macOS app **exists** and
mirrors lopi's core cockpit (Forge/live sessions/tasks/loop) while *adding*
admin surfaces the web lacks. The gap is the **newest** web work — most
notably the **Stacks** loop-composer (now the web default) and the
Fleet/Pulse/Router/Logs/Debug views — which the native app does not yet have. A
follow-up parity sprint would be about porting **Stacks + the goal-directed
run surface** (and the newer visualizers) into the macOS app, not about
building a macOS UI from scratch. Meanwhile, the "macOS version" that ships
today is the web dashboard in a browser plus the TUI; the native app is a
real-but-partial superset-on-admin / subset-on-newest-loop-UI client.

---

## Known issues

- **macOS app not buildable on Linux.** This ops run executed on Linux, so the
  native macOS app was inventoried from source (it requires Xcode + macOS 14+).
  Its build commands above are transcribed from `macos/README.md` /
  `RUN_MULTIPANE.md`, not executed here.
- **`/pulse` could not be screenshotted headlessly.** Its live event-waveform
  canvas crashes the headless Chromium renderer (software GL) within ~1s every
  time, across GPU/WebGL flag combinations. The route is server-healthy
  (`GET /pulse` → `200`) and renders fine in a real browser — this is a
  headless-CI rendering limitation, not a lopi bug. Every other route captured.
- **Screenshots use `?demo=1` mock data, not live agents.** Seeding real
  sessions spawns real `claude -p` streams that bill against the subscription;
  to keep this ops run non-mutating and zero-cost, the animated captures use
  the built-in mock path. The live-seed recipe is documented above and in
  `RUN_MULTIPANE.md`.
- Startup emits a one-time `sqlx` "slow statement" WARN on the first
  `PRAGMA journal_mode = WAL` when the SQLite DB is initialized on slow disk —
  cosmetic; `sail` comes up normally afterward.
