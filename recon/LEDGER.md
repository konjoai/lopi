# Sprint U-0 Recon — LEDGER

Reproducibility record for the screenshots, videos, and census data under `recon/`.
Read this before diffing a future "after" run against these shots.

## Base commit

`0873f60b3dbef40f5ba01ad298a890ed7398d422` (2026-07-29), branch
`claude/lopi-dashboard-recon-rnvvg7`, no other changes on top.

## Environment

| Component | Version |
|---|---|
| rustc / cargo | 1.88.0 |
| node | v22.22.2 |
| npm | 10.9.7 |
| @playwright/test | 1.62.0 (resolved from `^1.61.1` in `tools/recon/package.json`) |
| Chromium (launched explicitly, see below) | 141.0.7390.37 (`/opt/pw-browsers/chromium-1194`) |
| SvelteKit build | `web/` built via `npm run build` immediately before compiling `fixture-server`, so `rust-embed` bundles the real Forge dashboard, not `placeholder.html`. `web/dist/` is gitignored — rebuild it before re-running capture. |

The sandbox's pre-installed Playwright browser (`/opt/pw-browsers`) is pinned to a
different internal build id than `@playwright/test@1.62.0` expects, so
`chromium.launch()` fails with the default executable resolution
("Executable doesn't exist at .../chromium_headless_shell-1234/..."). Every
launch in `tools/recon/scripts/capture.mjs` passes
`executablePath: '/opt/pw-browsers/chromium-1194/chrome-linux/chrome'`
explicitly. Re-verify this path if the sandbox image changes.

## Fixture mechanism

`tools/recon/fixture-server` — a detached Cargo workspace (own `[workspace]`
table, mirrors `fuzz/Cargo.toml`'s own rationale) that path-depends on
`lopi-core`, `lopi-memory`, `lopi-orchestrator`, `lopi-ui`. It serves the
real, unmodified `lopi_ui::web::build_app` router — same `AppState`, same
routes, same static-asset embedding as `lopi sail` — merged with one
recon-only control route, `POST /recon/pump`, that is not reachable from
production (it lives entirely in this binary, not in `crates/lopi-ui`).
`AgentPool::run()` (the dispatch loop) is never spawned, so no `claude`
subprocess and no git call is reachable from this binary at all, matching
`lopi demo`'s own safety posture (`docs/adr/0001-demo-mode-and-measurement.md`).

**Why a live pump instead of DB seeding.** `web/src/lib/stores/stack.ts`'s
`panes`/`cards` are pure client-side session state — `makeDefaultPanes()`
always starts from two empty panes, and nothing in the SvelteKit app ever
rehydrates a pane from `GET /api/tasks` or `GET /api/tasks/:id/logs` (grepped
zero call sites for both). A historical task row seeded straight into
SQLite is therefore invisible on `/stacks` and `/overview` — the only way to
get a real `StackCard` on screen is to drive the real composer UI to submit
a real task through the real `POST /api/tasks` handler, then feed
`AgentEvent`s for that task's real (server-assigned, not fixed) id over the
real event bus. `tools/recon/scripts/capture.mjs` does exactly that: fill
the goal `contenteditable`, click "add", click "run stack", read the created
`id` off the `POST /api/tasks` response, then call `/recon/pump` with
`{task_id, scenario}`.

**What is and isn't byte-reproducible.**
- Deterministic: every pump's *content* (goal text, log line text, tool
  names/args, gap/fix-hint text, the S10 4000-char line and ANSI line, the
  S9 2200-line template cycle) and its *cadence* (every delay in
  `fixture-server/src/main.rs` is a fixed millisecond constant — no
  `rand`, no jitter).
- Not byte-reproducible: the task `id` (server assigns a fresh `Uuid::new_v4()`
  on every `POST /api/tasks` — this binary cannot pin it), and the exact
  wall-clock second `created_at`/`completed_at` land on (both are stamped by
  `lopi-memory`'s real `Utc::now()` inside the production `create_task`
  handler and `mark_completed`, neither overridable from outside). Playwright's
  clock is frozen to a fixed future instant (`2026-07-30T00:00:00.000Z`,
  chosen to postdate any real capture-time "now") so every screenshot's
  relative-time labels ("Ns ago") are internally consistent within one run
  and will read the same *bucket* (e.g. "a few seconds ago") on a future rerun,
  but not the identical literal second.

**Determinism protocol applied** (`tools/recon/scripts/capture.mjs`):
- Viewports: exactly `1920x1080`, `1440x900`, `1280x800`, `768x1024`, `390x844`.
- `deviceScaleFactor: 2`, fixed.
- Clock frozen via Playwright's Clock API (`page.clock.install`) before every
  navigation.
- Google Fonts requests (`fonts.googleapis.com`, `fonts.gstatic.com`) are
  intercepted and aborted — `web/src/app.html` loads Inter/JetBrains Mono
  from Google Fonts asynchronously, which is both a network dependency and a
  swap-timing source of non-determinism. A fixed system-font stack is
  injected instead via `page.addStyleTag` (see `FIXED_FONT_CSS` in
  `capture.mjs`) — screenshots use `-apple-system, BlinkMacSystemFont,
  "Segoe UI", Helvetica, Arial, sans-serif` for body text and
  `ui-monospace, SFMono-Regular, Menlo, Consolas, monospace` for anything
  matching `.font-mono`/`[class*="mono"]`/`code`/`pre`, never the real
  Inter/JetBrains Mono webfonts.
- Motion=off stills: `* { animation: none !important; transition: none !important; }`
  injected after navigation, one paint frame (250ms) before the shot.
- Motion=on: no override; 20 frames captured 100ms apart from a fresh
  navigation (state rebuilt from scratch, not reusing the "off" context).
- No element masking was needed beyond the above — every other source of
  variance (task ids, tool-call ordering, log text) is fixed content driven
  by the deterministic pump, not left to chance.

## Naming

`recon/shots/<state>_<viewport>_off.png` — motion=off reference still.
`recon/shots/<state>_<viewport>_on_frame<01-20>.png` — motion=on frame strip,
100ms apart (an extension of the brief's two-file naming scheme to a 20-file
strip, noted here rather than silently deviating).
`recon/shots/S4_streaming_30s.webm` — one 30s video of S4 at 1440x900.

## Unreachable states (no fixture-server variant exists for these)

See `recon/REPORT.md` section 7 for the full reasoning. Summary:
- **S6** (dead-letter with failure record displayed) — the dead-letter
  client (`listDlq`/`retryDlq`/`deleteDlq`) was removed outright in
  macOS-Parity-Cut-1 (`web/src/lib/api.ts:229-234`); no frontend surface
  reads `audit_log`/dead-letter rows at all today.
- **S7** (budget tier degraded to Conserve/Drain) — the backend feature is
  real and complete (`crates/lopi-orchestrator/src/budget/ladder.rs`,
  `GET /api/economics`'s `tier` field), but zero call sites in `web/src`
  ever fetch `/api/economics` — there is no dashboard element that would
  change if the tier changed.
- **S8** (cache hit ratio degradation warning) — no backend metric, no
  threshold, no event, no frontend surface; the only "cache hit" text
  anywhere in `web/src` is decorative demo copy in `wsClient.ts`.
