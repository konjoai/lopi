# Lopi for macOS

A native **SwiftUI** dashboard for [lopi](../). It is a client of a running
`lopi sail` server — it speaks the same REST + WebSocket API the web dashboard
uses, so the two stay in *data* parity, and adds OpenClaw-style extras (cron
scheduling, a menu-bar companion, admin panels). Since the web nav collapse
(Unify-2), several of those admin panels — Tasks, Tools, Health, Patterns,
Audit, Dashboard — are deliberately **native-exclusive**: the web folded them
into Overview or cut them, macOS keeps them as first-class screens.

> Status: **all 13 nav sections live.** Ops-2 swept every section against a
> running `lopi sail` and found **12 of 13 fully wired**; the 13th
> (Constellations) was the one broken screen and has since been removed, leaving
> 12 wired sections. Networking core, Konjo theme, app shell, menu-bar
> companion, dashboard, tasks, cron, and every admin panel are implemented and
> backed by live API calls — none are stubs.

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
| Dashboard     | `/ws` (live), `/api/stats` |
| Tasks         | `/api/tasks`, `/api/tasks/:id`, `/api/tasks/:id/logs`, `/api/tasks/:id/stream` |
| Cron          | `/api/schedules` (+ `:id`, `/enable`, `/disable`, `/run-now`) |
| Settings      | `/api/version`, `/api/config` |
| Admin panels  | `/api/tasks/dead-letter`, `/api/tools`, `/api/agents/health/summary`, `/api/audit`, `/api/patterns` |
```
