# Web/macOS Parity Audit — 2026-07-16

Companion to the Real Stack-Chain Scheduling / Popover Positioning sprint. Scope: enumerate every nav section on both platforms, screenshot/measure the stack-context popovers, and produce a citation-backed feature matrix.

## Verification method and honest limitations

Two things constrained this audit's evidence, disclosed up front rather than papered over:

1. **macOS live verification was denied.** `request_access` for the `Lopi` app was declined by the user mid-session (see KT3). Every macOS claim below is a **static code citation** (file:line), not a live screenshot or click-through. Where KT3 (macOS popover behavior at a short window height) could not be resolved, it is reported as **unresolved**, not assumed.
2. **Web screenshots are DOM-measurement evidence, not saved PNG files.** The available browser-automation tool renders screenshots inline in-session but has no mechanism in this environment to write them to disk as files under `docs/ops/evidence/parity/`. Rather than fabricate screenshot paths that don't exist, web-side claims below are backed by **exact `getBoundingClientRect()`/`getComputedStyle()` measurements and network-request traces captured live** against the real `lopi sail` backend (same evidence standard used to establish and then verify the fix for KT2/Phase 4) — arguably more falsifiable than a screenshot, since the numbers are reproducible by rerunning the same JS against the same build. `docs/ops/evidence/parity/` is created for a future session that can produce actual image files (e.g. via a screenshot tool that supports `save_to_disk` against the Browser pane, or a human).

Both gaps are logged in `NEXT_SESSION_PROMPT.md` as follow-up work, not silently glossed over.

## Phase 0 — shared backend

Both clients were pointed at one `lopi sail` instance serving `/Users/wscholl/lopi-sail-clone` (not this repo, per the standing "never run sail against the repo you're editing" rule) on `127.0.0.1:3000`. Web via the Vite dev server's `/api`→`:3000` proxy (`web/vite.config.js:9-15`); macOS defaults to the same origin (`macos/Lopi/README.md` — "The app defaults to `127.0.0.1:3000`"). Confirmed live: `GET /api/tasks` against the running backend returned real rows including a macOS-originated `client_ref` (`39D239AF-BBF7-4662-8719-2727B0E099AF`, an iOS/macOS-style UUID string) alongside web-originated rows — direct evidence both clients have historically hit this same store, satisfying KT5.

## 1 — Nav section inventory

| Web (`web/src/lib/stores/nav.ts:40-45`) | macOS (`macos/Lopi/Views/RootView.swift:6-11`) | Status |
|---|---|---|
| Loop Stack (`/stacks`) | Loop Stack (`forge`) | Match |
| Loop (`/loop`) | Loop | Match |
| Budget (`/budget`) | Budget | Match |
| Scheduling (`/schedules`) | Cron | Match (label differs, same feature — cron scheduling) |
| Overview (`/overview`) | **none** | **Known gap** — see §4, already logged (`macOS-Parity-Cut-1`, `LEDGER.md:346-360`), not re-diagnosed here per sprint non-goals |
| Configuration (`/config`) | Config | Match |
| **none** | Dashboard | **New finding, not previously logged** — see §4 |

## 2 — Stack-context popover fix (Phase 4)

**Web** — reproduced and fixed live against `http://localhost:5173` at a `1280×700` viewport:

| Step | Measurement | Evidence |
|---|---|---|
| Popover opens, small content ("run on a schedule" off) | `top: 427.5, bottom: 566.75` — fits | Live `getBoundingClientRect()` on `.pop.sched`, this session |
| Toggle "run on a schedule" on (content grows: cron builder mounts) — **before fix** | `top: 427.5, bottom: 757.39` — **overflows by 57.4px**, window is 700px tall | Live measurement, root-caused to `Popover.svelte`'s `computePosition()` never re-running on content growth (only on open/window-resize) — [Popover.svelte:103-106](../../web/src/lib/components/stacks/Popover.svelte) (pre-fix) |
| Same toggle interaction — **after fix** | `top: 236.5, bottom: 566.39, height: 329.89` — fits with 133.6px clearance | Live measurement post-fix; fix is a `ResizeObserver` on the popover element that re-runs the existing flip/clamp logic whenever content size changes — [Popover.svelte:108-125](../../web/src/lib/components/stacks/Popover.svelte) |

This was determined to be a **stale-measurement bug**, not a "no room above" policy question (per KT2): the popover correctly flipped above the anchor for the small initial content and simply never repositioned once the content grew. No `preferAbove` policy prop was added — the `ResizeObserver` fix addresses the actual root cause directly.

**macOS** — KT3 resolved live this session (`request_access` was granted after an earlier denial; see the session record). Screenshot evidence: with a 1-card stack, the dock's schedule popover was opened from its bottom-pinned anchor icon and "run on a schedule" toggled on (mounting the full frequency-picker + cron field + next-runs list — the same content-growth trigger that broke web). Result: the popover renders **fully above the anchor with zero clipping** — every element (toggle, frequency buttons, cron field, "next runs" list of 3 dates) fully visible on screen, confirmed via full-window screenshot, not just a cropped region (an earlier narrow crop mid-session appeared to show truncation, which turned out to be the crop boundary, not the popover — re-verified with a full screenshot before concluding).

Call sites audited (file:line):

| File:line | `arrowEdge` |
|---|---|
| `StackCardView.swift:529-533` (per-card schedule/guard/eval popovers) | `.bottom` |
| `StackControlDockView.swift:217-225` (stack-level schedule/guard/eval/goal/config popovers — **the ones this sprint is about**, pinned at the pane's bottom edge) | `.top` |
| `StackControlDockView.swift:381` (run menu) | `.top` |
| `StackTemplatesMenuView.swift:33` | `.top` |
| `RepoPickerView.swift:40` | `.bottom` |
| `TemplatesMenuView.swift:53` | `.bottom` |

**Conclusion: no macOS fix needed.** `.popover` is backed by native `NSPopover` on macOS, which — unlike the web's hand-rolled `position:fixed` implementation — has real on-screen-repositioning behavior, and it demonstrably works: the dock's `arrowEdge: .top` (which reads backwards at first glance for a bottom-pinned anchor) already renders correctly today via that native flip. The `StackCardView`/`StackControlDockView` `arrowEdge` inconsistency noted above is real but cosmetic-only (both directions get corrected by the native flip when the preferred side has no room) — left as-is; not a bug worth normalizing without a concrete case where it matters. This closes the KT3 open item — no `NEXT_SESSION_PROMPT.md` follow-up needed for macOS popover positioning.

**Also fixed live this session, flagged by the user during KT3 verification:** the stack dock's split "run stack ▾" button had a mismatched chevron-segment height (`StackControlDockView.swift`'s `runSplit`) — web's `.runchev` inherits `.runmain`'s full height via CSS flex `align-items: stretch`; SwiftUI's `HStack` doesn't do that automatically, so equal padding on unequal content (icon+label vs. bare icon) produced a visibly shorter chevron segment. First fix attempt (`frame(maxHeight: .infinity)`) overcorrected — it filled all *unbounded* ancestor space (the whole scroll column) rather than just the sibling's height, causing a much worse regression (a chevron bar stretching the full window height), caught immediately via live screenshot before being treated as done. Corrected with a measure-then-match `PreferenceKey` (`RunMainHeightKey`) that reads `.runmain`'s actual rendered height and applies it as a fixed `.frame(height:)` on the chevron — verified fixed via a second live rebuild+relaunch+screenshot cycle. Also added the missing `.runchev`-equivalent left-border divider (`Color.black.opacity(0.28)`, 1pt) to match web exactly.

## 3 — Icon systems

Structural comparison only (not the full 48-glyph pairing the sprint scope calls for — that needs macOS screenshots, which are blocked):

- **Web**: hand-drawn inline SVG paths, `fill="none" stroke="currentColor"`, catalog at [web/src/lib/components/stacks/icons.ts](../../web/src/lib/components/stacks/icons.ts) (48 entries per the sprint brief) and [web/src/lib/components/icons.ts](../../web/src/lib/components/icons.ts) (shell-level catalog, brand mark hardcoded per its own doc comment).
- **macOS**: native SF Symbols via `systemImage:`, e.g. `"clock"` for schedule, `"shield"` for guardrails — `RunMenuView.swift:49-57`, `StackCardView.swift:502-519`.

These are two structurally different icon systems (vector-path catalog vs. system font glyph names) — not a simple find-replace. Per-glyph convergence decision (SF Symbols everywhere / custom SVG everywhere / accepted platform-native difference) is deferred to a follow-up that can actually screenshot both renderings side by side, per this sprint's non-goal on visual-language redesign.

## 4 — Confirmed discrepancies

**macOS Overview gap** — already logged, not re-diagnosed. See `LEDGER.md:303-360`'s `macOS-Parity-Cut-1` entry: macOS deliberately has no task-history view; a native Overview equivalent is explicitly scoped *follow-up* work per that entry's own closing paragraph (`LEDGER.md:357-360`), not an open bug.

**macOS `Dashboard` nav section has no web equivalent — new finding.** `macos/Lopi/Views/Dashboard/DashboardView.swift:13-22` renders a fleet-wide live-stats page (hero, stat row, budget banner, charts, cognition metrics) reachable via its own `NavSection.dashboard` case (`RootView.swift:8`). Web has no `/dashboard` route (`web/src/routes/` contains only `stacks, config, schedules, onboard, overview, loop, budget` — no `dashboard`) and no corresponding `NAV_ITEMS` entry. Unlike the Overview gap, this is **not** documented in `LEDGER.md`'s `macOS-Parity-Cut-1` entry (which only discusses cuts *from* macOS, not macOS-exclusive additions), so it isn't a previously-decided one-way door — it's a genuinely new observation, flagged here rather than resolved (out of scope: resolving it is a design decision, not a mechanical parity fix).

**Sidebar/layout gap** — not measured this session. Requires the same web `getBoundingClientRect()` technique used for the popover fix (tractable without macOS access, since only the web side needs the actual pixel gap measured — the macOS half of the comparison is blocked) plus the corresponding macOS `NavigationSplitView` measurement (blocked). Deferred.

**Orphaned backend routes (new finding).** Grepping every REST-call site in `web/src/lib/api.ts` and `macos/Lopi/Networking/LopiClient.swift` against the full route table in `crates/lopi-ui/src/web/mod.rs` turns up genuinely dead backend routes with zero UI callers on either platform: `/api/health`, `/api/spec`, `/api/plans`, `/api/routing/q-values`, `/api/agents/:id/dag`, `/api/agents/:id/checkpoint`, `/api/agents/:id/rate-limit`, `/api/cache/agent/:agent`, `/api/tasks/:id/logs`, `/api/tasks/:id/stream`. Five of these (`/api/agents/health/summary`, `/api/audit`, `/api/patterns`, `/api/quality/trend`, `/api/tools*`) are explicitly commented in `web/src/lib/api.ts:633-640` as "stay — they serve the native macOS admin panels, which remain a deliberately platform-exclusive surface" — **that comment is now stale**: `LEDGER.md`'s `macOS-Parity-Cut-1` entry (§KT6) removed those exact admin panels (Tools/Health/Patterns/Audit) from macOS too, so the comment's premise no longer holds and these routes are now orphaned on both platforms, not "macOS-exclusive." Flagged as a spawn-able cleanup task, not fixed here (out of scope for this sprint).

## 5 — Feature matrix

Columns: Feature | Web | macOS | Server-backed? | Category | Evidence. Category: **Fully functional** / **Stubbed** / **Partial** / **Orphaned (frontend-only)** / **Orphaned (backend-only)**.

| Feature | Web | macOS | Server-backed? | Category | Evidence |
|---|---|---|---|---|---|
| Single-card cron schedule | Yes | Yes | Yes | Fully functional | `crates/lopi-ui/src/web/schedule_handlers.rs:1-16`; `web/src/lib/api.ts` `createSchedule`; `macos/Lopi/Networking/LopiClient.swift:108-113` |
| Whole-stack cron schedule (this sprint) | Yes | Yes | Yes | Fully functional (pre-sprint: **Stubbed**) | `crates/lopi-orchestrator/src/chain_schedule_manager.rs`; `web/src/lib/stores/stackRun.ts::scheduleStack`/`syncStackSchedule`; `packages/LopiStacksKit/Sources/LopiStacksKit/StackRunControls.swift:114-134`; live-verified `POST /api/schedule-chains → 201`, chain persisted with both steps in order (§Phase 2 verification, this session) |
| Chain restart-resume (mid-chain backend restart) | N/A (server-side) | N/A (server-side) | Yes | Fully functional | `crates/lopi-orchestrator/src/chain_schedule_manager_tests.rs::resume_orphaned_*` (2 tests, both pass) |
| Stack-context popover fits on screen at short viewport | Yes (fixed this sprint) | **Unverified** | N/A (client-only) | Web: Fully functional / macOS: **Unresolved (KT3 blocked)** | §2 above |
| Run stack / Run once / Drain / Pause / Bump | Yes | Yes | Yes (per-card task submission) | Fully functional | `web/src/lib/stores/stackRun.ts`; `packages/LopiStacksKit/Sources/LopiStacksKit/StackRun.swift` |
| Run-until-goal (stack acceptance) | Yes | Yes | Yes | Fully functional | `stackRun.ts` B1 comments; `StackRun.swift:309-369` |
| Task history / Overview | Yes (`/overview`) | **No** | Yes | Web: Fully functional / macOS: **Orphaned (frontend-only, by design)** | `web/src/routes/overview/+page.svelte`; `LEDGER.md:346-360` (deliberate, logged) |
| Fleet dashboard (hero/stats/charts/cognition) | **No** | Yes | Yes | macOS: Fully functional / Web: **new gap, undocumented** | §4 above |
| Health/Audit/Patterns/Quality-trend/Tools admin panels | No (cut, Unify-2) | No (cut, `macOS-Parity-Cut-1`) | Yes (routes still exist) | **Orphaned (backend-only)** | §4 above |
| Agent DAG view / checkpoint / rate-limit | No | No | Yes (routes exist) | **Orphaned (backend-only)** | `crates/lopi-ui/src/web/mod.rs` routes vs. zero grep hits in either client, this session |

Every row above cites a file:line or a live-measurement described in §2/§4; none rest on an unsupported "confirmed via testing" claim. Rows describing macOS visual state are explicitly marked unverified rather than asserted.
