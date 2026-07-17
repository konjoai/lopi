# Lopi for macOS

A native **SwiftUI** dashboard for [lopi](../). It is a client of a running
`lopi sail` server — it speaks the same REST + WebSocket API the web dashboard
uses, so the two stay in *data* parity, and adds a cron scheduler and a menu-bar
companion. `macOS-Parity-Cut-1` brought the nav in line with web: the admin
panels web no longer surfaces — **Tools, Health, Patterns, Audit** (cut outright
in Unify-2) and **Tasks + Dead-Letter** (folded into web's Overview) — were
removed here too, along with their now-orphaned backend endpoints. The dead-letter
queue was then retired entirely — front, back, storage, and web client — so it is
a removed feature, not a deferred one. macOS now has its own Overview section —
a dense, read-only, filterable rollup of every live agent (goal/repo/branch/
phase/elapsed/cost/score), click-to-focus onto the Forge grid — mirroring web's
`/overview` route.

> Status: **7 nav sections live** — Loop Stack (Forge), Dashboard, Budget, Loop,
> Cron, Overview, Config — all backed by live API calls, none stubs. Tasks that
> exhaust their retry budget are still marked `failed`, but are no longer
> recorded in a dead-letter store or retryable — that subsystem was removed.

## Requirements

- macOS 14 (Sonoma) or later — uses the Observation framework (`@Observable`)
  and `MenuBarExtra`.
- Xcode 15+.
- [XcodeGen](https://github.com/yonyk/XcodeGen) to generate the project from
  `project.yml` (keeps the project text-defined and reviewable):
  ```bash
  brew install xcodegen
  ```

## Build & run

```bash
cd macos
xcodegen generate        # produces Lopi.xcodeproj from project.yml
open Lopi.xcodeproj       # then ⌘R in Xcode
```

In another terminal, start the backend the app talks to:

```bash
cargo run -- sail          # serves http://127.0.0.1:3000
```

The app defaults to `127.0.0.1:3000`. Point it at a remote server (and set a
Bearer token) in **Settings**; the token is stored in the macOS Keychain.

## Architecture

```
LopiApp ──┬── WindowGroup ── RootView (NavigationSplitView)
          └── MenuBarExtra ── MenuBarView

AppModel (@Observable)  ← single source of UI state
  ├── LopiClient        REST over URLSession (Bearer auth, 429 backoff)
  └── EventStream       /ws WebSocket → AsyncStream<AgentEvent> (auto-reconnect)
```

- `Networking/Models.swift` — `Codable` mirrors of the lopi-core wire types
  (`Task`, `TaskStatus`, `AgentEvent`, `PoolStats`, `Schedule`, …). The
  `AgentEvent` decoder matches Rust's `#[serde(tag = "type", rename_all =
  "snake_case")]`; `TaskStatus` tolerates both the bare-string and tagged-object
  forms serde emits.
- `Theme/KonjoTheme.swift` — the exact palette/typography of the web UI
  (`#06060f` bg, `#7c3aed` Konjo purple, SF Mono for code).

## Mapping to the lopi API

| Screen        | Endpoints |
|---------------|-----------|
| Loop Stack (Forge) | `/ws` (live), `/api/tasks` (+ `:id`, `/plan/*`, `/checkpoint`) |
| Dashboard     | `/ws` (live), `/api/stats` |
| Budget        | `/api/stats`, `/ws` |
| Loop          | `/api/loop-engineering/*` (health, runs, strategy) |
| Cron          | `/api/schedules` (+ `:id`, `/enable`, `/disable`, `/run-now`) |
| Config / Settings | `/api/version`, `/api/config`, `/api/cache/stats`, `/api/cache` |
```
