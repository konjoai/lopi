# iOS ↔ Web Parity Plan — 2026-07-23

## Scope and method

This is a plan, not a shipped sprint — no Swift was changed to produce it. Every
claim below is a static code citation (file:line) against `origin/main`, read on
a Linux host with no Xcode (the same standing constraint every prior iOS/macOS
session has carried — see `docs/ops/IOS_RESEARCH_1_SPIKE.md`,
`docs/ops/PARITY_AUDIT_2026-07-16.md`). Nothing here should be read as "this
builds" or "this was measured live." Where a claim needs a real device/simulator
to confirm, it's flagged as such rather than asserted.

**Why iOS, not "mobile" generically:** `macos/LopiIOS/` is the only mobile
client in this repo. It already exists — this is a parity-completion plan for a
real, partially-built app, not a from-scratch mobile proposal.

## The one finding that shapes this whole plan

**iOS parity is almost entirely a UI-screens problem, not a networking or
data-model problem.** `macos/project.yml:83-94`'s `LopiIOS` target already
compiles in `Lopi/Networking`, `Lopi/Store`, `Lopi/Theme`, `Lopi/Stacks`, and
`Lopi/Components` verbatim (plus the shared `LopiStacksKit` package) — the exact
same sources the macOS app's Budget/Loop/Scheduling/Overview/Config screens read
from (`LopiClient+Admin.swift`, `LoopModels.swift`, `AdminModels.swift`,
`AppModel+Admin.swift`, `Store/Overview.swift`). None of those four macOS views
import AppKit or anything iOS can't render — `BudgetView.swift`, `LoopView.swift`,
`CronView.swift`, `OverviewView.swift`, `ConfigView.swift` all open with plain
`import SwiftUI` (+ `LopiStacksKit`), no `NSPopover`/`NSColor`/`NavigationSplitView`
dependency (grep, this session). So the missing surfaces below are a **narrow-width
adaptation of an already-working macOS view**, not a networking port and not a
rewrite against the web app. This is the opposite of the `Stacks/` extraction
question `IOS_RESEARCH_1_SPIKE.md` answered for the domain layer — here, the
data/network layer was *already* shared before any of these screens existed,
because it was pulled in wholesale by `project.yml`'s source list rather than
per-file.

## 1 — Nav-surface inventory

| Web nav (`web/src/lib/stores/nav.ts:39-46`) | iOS today | Status |
|---|---|---|
| Loop Stacks (`/stacks`) | `StackOverviewScreen` + `StackDetailScreen` (`macos/LopiIOS/Views/`) | **Present**, see §2 for in-surface gaps |
| Loop (`/loop`) | **None** | **Missing** — no iOS screen; macOS equivalent exists (`Views/Loop/LoopView.swift`, 644 lines) |
| Budget (`/budget`) | **None** | **Missing** — macOS equivalent exists (`Views/Admin/BudgetView.swift`, 229 lines) |
| Scheduling (`/schedules`) | **None** (only the per-card/per-stack schedule *popover*, a different feature) | **Missing** — macOS equivalent exists (`Views/Cron/CronView.swift`, 300 lines) |
| Overview (`/overview` — read-only, whole-account kanban rollup) | **None** — `StackOverviewScreen` is the management surface (`/stacks` equivalent: swipe-to-pause/delete, tap-to-open), not the read-only rollup | **Missing** — no macOS equivalent either (`LEDGER.md`'s `macOS-Parity-Cut-1` entry: macOS deliberately has no Overview; this is a two-platform gap, not iOS-only) |
| Configuration (`/config` — effective `LoopConfig`, theme, server identity) | `ServerConfigScreen` covers only host/port/token + connection status | **Partial** — macOS equivalent exists (`Views/Admin/ConfigView.swift`, 156 lines) but iOS's screen is scoped to connection settings, not app config |
| *(no web equivalent)* Dashboard | **None** | N/A — macOS-only surface per `PARITY_AUDIT_2026-07-16.md` §4; out of scope here, not a web-parity gap |

**Net: iOS has 1 of web's 6 nav surfaces fully built (Loop Stacks), plus a
connection-settings screen that only partially covers the 6th (Configuration).**

## 2 — Gaps inside the one surface iOS does have (Loop Stacks)

Even the built surface trails both web and macOS in a few concrete ways:

- **No run-control menu.** Web's `RunMenu.svelte` and macOS's `RunMenuView.swift`
  both offer Run now / Run once / Schedule stack / Dry run (idle) or
  Pause/Resume + Drain (running) — a real dropdown wired to `StackRunEngine`/
  `stores/stackRun.ts`. iOS's `StackDockView` (`StackCommandBar.swift:54-71`) has
  only a single bare "run stack" button that always calls `.run` directly — no
  Run once, no Dry run, no Schedule stack, no Pause/Drain, no menu at all. This
  is an iOS-specific regression versus *both* other platforms, not a
  web-vs-native platform difference.
- **No bump (reorder-by-priority) UI.** Web's `stackRun.ts` `bumpCard` got a UI
  trigger per `NEXT.md`'s Loop-Stack-Connect-1 entry; grepping
  `macos/LopiIOS` for `bump` returns nothing. Drag-to-reorder exists
  (`StackDetailScreen.swift:344-354`, native `.draggable`/`.dropDestination`) but
  that's a different gesture (manual reorder mid-list) from the ▲/▼ bump
  buttons on a still-queued card.
- **Old command grammar.** iOS's composer/dock chips read `:alias`, `@repo`,
  `/model`, `/effort`, `×N` (`StackDetailScreen.swift:149-153`,
  `StackCommandBar.swift:45-49`) — the *pre*-`Composer-Grammar-1` grammar. Web
  moved every lopi-specific command (`model`/`effort`/`branch`/`autonomy`/`eval`/
  `guard`/`schedule`/`maxx`) behind a `;` prefix and vacated `/` for real Claude
  Code slash commands (`NEXT.md`'s Composer-Grammar-1 entry). This isn't an
  iOS-only gap — `packages/LopiStacksKit/Sources/LopiStacksKit/StackOps.swift:70-77`'s
  `CARD_COMMANDS`/`STACK_COMMANDS` are the shared domain layer both macOS and iOS
  read from, so **fixing this once in `LopiStacksKit` fixes both native
  platforms**, per `NEXT.md`'s own carried-forward note ("macOS still speaks the
  old `/`-prefixed grammar").

## 3 — The missing surfaces, one at a time

For each, "port cost" assumes the finding in the box above (network/data layer
already shared) and estimates only new SwiftUI screen work plus any iOS-only
layout adaptation (macOS views are built for a resizable multi-hundred-pixel
window; iOS is portrait-only, single column, per `project.yml:102`'s
`TARGETED_DEVICE_FAMILY: "1"`).

### 3.1 Budget (`/budget`, `BudgetView.swift`)
Fleet spend, burn-rate vs. hourly cap, 7-day trend sparkline, by-repo/by-model
breakdown, top spenders with stop controls. Backend: `/api/stats` /
budget-breakdown endpoints, already called by `AppModel+Admin.swift` — no new
wire work. **Port shape:** single scrolling column (stat cards stack instead of
a grid row, sparkline and breakdown tables reflow to full width) — no new data
plumbing. Lowest-risk of the four; recommend first.

### 3.2 Scheduling (`/schedules`, `CronView.swift`)
Standalone cron schedule CRUD (name/cron/goal/repo/priority/enabled), distinct
from the per-stack schedule popover iOS already has. Backend:
`listSchedules`/`createSchedule`/`updateSchedule`/`deleteSchedule`/
`enableSchedule`/`disableSchedule`/`runScheduleNow` — all already used by the
existing schedule *popover* facet (`FacetPopovers.swift`), so the wire calls are
proven working from iOS already; this is "reuse the same calls in a
list+form screen" rather than new integration risk.

### 3.3 Loop Engineering (`/loop`, `LoopView.swift`)
Effective `.lopi/loop.toml` (read-mostly), L1–L4 autonomy ladder per schedule,
discovered skills/rules, run traces + drill-down, Konjo quality gate status.
The most complex of the four (644 lines on macOS) — recommend doing this
*after* Budget/Scheduling establish the narrow-layout pattern, and consider
whether the run-trace drill-down needs a simplified first cut on a phone-width
screen (a full trace table may need to become a stacked detail view rather than
a side-by-side layout).

### 3.4 Overview (`/overview`)
The one surface **neither** native platform has today — this is genuinely new
work on both, not a macOS→iOS port. Recommend building it web-first-informed:
`stores/stackOverview.ts`'s `buildStackOverviewCards`/`groupByLifecycle` logic
is pure and already has a Swift-shaped precedent in how `StackDisplay.swift`
computes `overviewPhase` for the existing `StackOverviewScreen`. A mobile
Overview would most naturally reuse iOS's existing swipe-list *chrome*
(`StackOverviewScreen.swift`'s `List`/`Section` pattern) but backed by
read-only whole-account data instead of the user's own open panes — needs a
design decision (own screen vs. a mode-toggle on the existing Stack Loops
screen) before implementation, not assumed here.

### 3.5 Configuration (`/config`, `ConfigView.swift`)
Effective server config (flattened tree/raw toggle), theme picker, version/
uptime. Lower priority — `ServerConfigScreen` already covers the connection
half; this closes the "app config" half. Reasonable to fold into
`ServerConfigScreen` as an additional section rather than a new screen, given
how small `ConfigView.swift` is (156 lines) and that both are already
settings-shaped.

## 4 — Recommended phased plan

1. **Phase 0 — grammar unification in `LopiStacksKit`.** Port
   `Composer-Grammar-1`'s `/` → `;` rename (+ `/loop/N` removal) into
   `StackOps.swift`'s shared `CARD_COMMANDS`/`STACK_COMMANDS`, with
   `stack.test.ts`'s kill-test-1 table (`;model/sonnet`, `;effort/high`,
   `;branch/main`, `;autonomy/L2`, `;eval/kcqf`) ported as Swift assertions —
   the acceptance bar `NEXT.md` already specifies. Fixes macOS and iOS in one
   change; do this before building new iOS surfaces so they're not built
   against a grammar already scheduled to change.
2. **Phase 1 — RunMenu + bump on iOS.** Closes the biggest in-surface behavior
   gap (§2) before adding new screens elsewhere — a user who reaches for "run
   once" or "dry run" on the one surface iOS already has is a sharper gap than
   a whole missing nav tab.
3. **Phase 2 — Budget.** Lowest risk, proves the narrow-layout adaptation
   pattern for the remaining three.
4. **Phase 3 — Scheduling.** Reuses already-proven wire calls; mechanical once
   Phase 2's layout pattern exists.
5. **Phase 4 — Loop Engineering.** Highest complexity; do last of the ports so
   the narrow-layout idioms are settled.
6. **Phase 5 — Overview.** Genuinely new (not a port) on both platforms; scope
   as its own design decision, not a mechanical follow-on to Phase 4.
7. **Phase 6 — Configuration fold-in.** Smallest; append to
   `ServerConfigScreen` once the pattern for surfacing macOS admin data on iOS
   is established by Phases 2–4.

Each phase's actual acceptance bar is the same discipline as every prior
Swift round in this repo: written on this host, then **compiled and run for
real** (`xcodegen generate && xcodebuild -scheme LopiIOS build`, then a
simulator/device pass) before being called done — nothing here should be
shipped on the strength of a Linux-authored diff alone.

## 5 — Explicit non-goals

- **iPad support.** `project.yml:102` scopes `TARGETED_DEVICE_FAMILY` to iPhone
  only "for the first pass" — this plan does not revisit that.
- **Visual-language unification (SF Symbols vs. custom SVG icon systems).**
  Flagged as deferred in `PARITY_AUDIT_2026-07-16.md` §3 for macOS; the same
  deferral applies to iOS, unchanged here.
- **A native Dashboard.** macOS-exclusive per the existing audit; not a
  web-parity item, not added to iOS either.
- **Re-deciding already-closed one-way doors** — the DLQ removal, the
  Tools/Health/Patterns/Audit admin-panel cuts, and the client-side-only
  scope of stack goal/eval execution all carry forward unchanged (see
  `LEDGER.md`'s `macOS-Parity-Cut-1` and `NEXT.md`'s standing notes).

## 6 — Open questions for whoever picks up Phase 0+

1. Does Overview (§3.4/Phase 5) become its own iOS screen or a mode-toggle on
   `StackOverviewScreen`? Both are defensible; not decided here.
2. Should the Loop Engineering run-trace drill-down (§3.3) get a simplified
   phone-width variant, or does the full macOS layout survive a straightforward
   reflow? Needs a real device pass to answer, not a guess from a Linux host.
3. Config fold-in (§3.6/Phase 6) assumes appending to `ServerConfigScreen` is
   preferable to a new screen — worth a two-minute gut check with whoever owns
   the iOS design language before building it either way.
