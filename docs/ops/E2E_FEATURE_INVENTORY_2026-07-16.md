# E2E Feature Inventory — Web + macOS — 2026-07-16

Reference checklist for the Playwright (web) + XCUITest (macOS) coverage effort. Companion to `PARITY_AUDIT_2026-07-16.md` (nav-section matrix, popover-fix writeup). This document enumerates every distinct interactive feature on both platforms, tags each with its current test status, and cross-references the underlying Rust backend logic where relevant.

## Method

Built from direct reads of every file in `web/src/routes/*/+page.svelte`, `web/src/lib/components/stacks/*.svelte`, `web/src/lib/components/{AppSidebar,HelpOverlay,TileGrid}.svelte`, the 3 Playwright specs (full contents), every file in `macos/Lopi/Views/Forge/*.swift` + the other macOS view dirs, `packages/LopiStacksKit/{Sources,Tests}/**`, `macos/LopiTests/*.swift`, the 1 XCUITest spec, and targeted `grep`/read passes over `crates/lopi-ui/src/web/*.rs` and `crates/lopi-orchestrator/src/*.rs` test modules. Three research passes (web components, macOS views, Rust backend) were run in parallel and cross-checked against direct spot-reads of the largest files (`StackCard.svelte`, `StackControlDock.svelte`, `StackPane.svelte`, `StackOutput.svelte`, `StackConnector.svelte`, `MaxxPopover.svelte`, `GoalPopover.svelte`, all 3 `LopiStacksKitTests` files, all 4 `macos/LopiTests` files) plus independent `grep` verification of every "zero coverage" and "no backend route" claim below.

## Legend

- **none** — no test coverage of any kind.
- **unit-only** — the underlying pure logic has a unit test, but no UI-driving test (Playwright/XCUITest) exercises the component/flow itself.
- **E2E** — a Playwright spec or XCUITest spec drives this exact flow.
- **UT** = unit test. **XC** = XCUITest. **PW** = Playwright.
- Existing tests, confirmed exhaustively (nothing else exists beyond these):
  - Playwright (`web/e2e/`, 3 files): `nav-parity-smoke.spec.ts` (all 6 nav routes load with no console error — load-only, no feature assertions), `popover-visibility.spec.ts` (stack-scope schedule popover stays on-screen after the cron builder mounts), `stack-chain-schedule.spec.ts` (schedule-the-stack wires every card into one `/api/schedule-chains` POST, in order).
  - XCUITest (`macos/LopiUITests/`, 1 file, 2 tests): `StackChainScheduleUITests.swift` — same two flows as the Playwright pair above, native-side.
  - Web unit tests wired into `npm test` (`web/package.json`'s `test` script, 20 files): `parser`, `forge/connections`, `forge/excitement`, `api`, `components/ui/badges`, `stores/events`, `stores/layout-core`, `stores/agentReducer`, `stores/session-groups`, `stores/transcript`, `render/markdown`, `forge/orbState`, `forge/cardOrb`, `stores/stack`, `stores/stackDefaults`, `stores/repoMenu`, `stores/stackGoal`, `stores/stackRun`, `stores/overview`, `stores/nav`, `stores/modelCatalog`. Notably **not** unit-tested anywhere: `stores/budget.ts`, `stores/theme.ts`, `stores/keyboard.ts`, `stores/templates.ts`, `stores/layout.ts` (dead code), any `.svelte` component's own logic (only the pure stores they call).
  - macOS unit tests (`packages/LopiStacksKit/Tests/LopiStacksKitTests/`, 3 files: `StackStoreTests`, `StackRunTests`, `StackGoalTests`; `macos/LopiTests/`, 4 files: `RepoMenuTests`, `StackBranchTests`, `StatsParityTests`, `AgentEventGoldenTests`).

---

## 0. Cross-platform structural findings (read this before the tables)

- **Web-only feature, no macOS equivalent:** **MAXX** (`StackCard.svelte`'s flame button, `MaxxPopover.svelte`, `/api/maxx` CRUD). Confirmed by `StackCardView.swift`'s own doc comment ("No `/maxx` here — macOS `StackCard` has no MAXX field yet") and by `CARD_COMMANDS`/`STACK_COMMANDS` in `StackOps.swift` containing no `maxx` entry. Not a bug — an acknowledged one-way gap.
- **macOS dead code:** `AgentPaneView.swift` and `TranscriptView.swift` are unreferenced anywhere outside their own files (`grep -rl "AgentPaneView" macos/` finds nothing else) — `ForgeView` mounts `StackPaneView`/`StackCardView` exclusively. Neither is reachable from any live nav path; exclude from XCUITest planning unless they're about to be wired in, and consider flagging as dead code per the repo's own zero-dead-code gate (this applies to the Konjo framework generally, not this Swift codebase specifically, but the principle is the same).
- **Web dead code:** `web/src/lib/components/Composer.svelte` and `LaunchControls.svelte` are not imported anywhere in `web/src/` — retired Forge-era components. `web/src/lib/stores/layout.ts` (the stateful pane-slot/session-tombstone store) is likewise unused by any current route; only `layout-core.ts::tileDims` (the pure auto-tiling math) is actually wired into `/stacks`.
- **No cross-pane card drag exists on either platform** — only whole-*stack* (pane) reordering between grid cells is implemented. Card drag-reorder is explicitly same-pane-only on web (`dnd.ts`'s `dragging` store, every consumer checks `paneKey` matches) and implicitly the same on macOS (`CardDragPayload`/`.dropDestination` scoped inside `StackPaneView`). If your test plan mentions "drag-reorder whole stacks between panes" it means pane reordering, not per-card cross-pane moves — that flow doesn't exist to test.
- **`bumpCard`/"bump sooner/later"** (mid-run reorder of a still-queued card) is fully implemented and heavily unit-tested on both platforms (`stack.test.ts`/`stackRun.test.ts`'s bump sections; `StackStoreTests.testBumpInOrder`/`StackRunTests.testBumpReflectsIntoPane`/`testBumpRejectsIllegal`) but has **zero E2E coverage on either platform** — a good first Playwright/XCUITest candidate given how well the logic underneath it is already proven.
- **`window.prompt()` blocking dialogs**: both "save as template" flows (card-scope `TemplatesMenu.svelte`, stack-scope `StackTemplatesMenu.svelte`) use the browser's native `window.prompt()` for the template name. Playwright needs `page.on('dialog', ...)` handling for these — flag in the test-plan design, not just the coverage table.
- **`templates.ts` (web) has no unit-test file at all** — only the pure conversion functions it calls (`promptTemplateFromCard`, `stackTemplateFromCards`) are tested via `stack.test.ts`; the actual `localStorage` persistence (`savePromptTemplate`/`saveStackTemplate`) is untested. Same gap on macOS: `StackTemplateStore`'s `savePrompt`/`saveStack` persistence methods have no test file (only the pure functions they call are tested).
- **`stores/keyboard.ts` (web) has zero unit tests** for any of its 5 shortcuts, and `HelpOverlay.svelte`'s toggle is untested at both unit and E2E level.

---

## 1. Global Shell

Web: `web/src/routes/+layout.svelte`, `AppSidebar.svelte`, `HelpOverlay.svelte`, `stores/nav.ts`, `stores/keyboard.ts`, `stores/events.ts`. macOS: `RootView.swift`, `MenuBarView.swift`, `SettingsView.swift`.

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Hamburger → sidebar open/close | `+layout.svelte:47-58` | none | none | n/a (macOS uses a permanent `NavigationSplitView` sidebar, no hamburger) | — | — |
| 2 | Wordmark → `/stacks` | `+layout.svelte:59-61` | none | none | n/a | — | — |
| 3 | Active-route breadcrumb label | `+layout.svelte:36-37`; `nav.ts::activeNavItem` | unit (`nav.test.ts`) | none | `RootView.swift:91-135` nav section switch | none | none |
| 4 | Connection-state indicator (online/preview/connecting/offline) | `+layout.svelte:21-32,67-74` | none | none | `RootView.swift:197-211` connection LED + live-count | none | none |
| 5 | Topbar `+` add-pane (visible on `/stacks` only) | `+layout.svelte:74-85` → `addStackPane()` | none (pane-array wrapper untested; underlying `addStack` is — `stack.test.ts`) | none | `ForgeView.swift:56-60` → `addStack` (max 12 panes) | **yes** — `StackStoreTests.testPaneIsBareAndCreation` | none |
| 6 | Immersive vs. scrollable route layout | `+layout.svelte:39,92-100`; `nav.ts::isImmersiveRoute` | unit (`nav.test.ts`) | indirectly touched by `nav-parity-smoke` (no direct assertion) | n/a (native window chrome, not applicable) | — | — |
| 7 | Sidebar open/close: scrim click, Esc, nav-item click | `AppSidebar.svelte:34-57,74,95` | none | none | n/a | — | — |
| 8 | Sidebar focus trap + focus-return | `AppSidebar.svelte:39-67` | none | none | n/a | — | — |
| 9 | 6 sidebar nav links, active highlight | `AppSidebar.svelte:93-101`; `nav.ts:39-46 NAV_ITEMS`, `isActiveRoute` | **yes**, thoroughly — `nav.test.ts` (item order/labels/count, cut-route exclusion, exact+sub-route+no-prefix-bleed) | none (`nav-parity-smoke` uses `page.goto`, never clicks the sidebar) | `RootView.swift` nav sections (Loop Stack/Dashboard/Budget/Cron/Loop/Config) | none | none |
| 10 | Sidebar icon-rail mode (currently dead, `SIDEBAR_MODE` pinned to `'hidden'`) | `AppSidebar.svelte:142-152`; `nav.ts:78` | unit (constant-value assertion only) | n/a — unreachable in UI | n/a | — | — |
| 11 | Help overlay toggle (`?` key), dismiss (Esc/outside-click) | `HelpOverlay.svelte`; `keyboard.ts:72-78` | none — **no `keyboard.test.ts` exists** | none | n/a (no keyboard-shortcut help surface found in macOS views read) | — | — |
| 12 | Keyboard shortcuts: `j`/`k`/arrows (cycle agent), `⌘K` (toggle Stacks↔Overview), Esc (clear focus) | `keyboard.ts:33-78` | none | none | n/a | — | — |
| 13 | Budget-breach toast (appears on `budget_exceeded`, dismiss button) | `+layout.svelte:102-130`; `events.ts::budgetAlerts`/`describe()` | partial — `events.test.ts` covers `describe()`'s tier/summary mapping (`budget_exceeded`→tier `'bad'`, summary names scope) but not `recordEvent`'s alert-list cap/dedup or `dismissBudgetAlert` | none | `BudgetView.swift:175-186` breach banner (in-page, not a global toast) | none | none |
| 14 | Live session list (park/reopen a session) | n/a (web has no equivalent sidebar session list) | — | — | `RootView.swift:137-158,160-186` open/delete session | none | none |
| 15 | Nav badges (running count, schedule count) | n/a | — | — | `RootView.swift:189-195` | none | none |
| 16 | Menu-bar quick-submit goal field + recent-tasks list | n/a (web has no menu-bar equivalent) | — | — | `MenuBarView.swift:35-69` | none | none |
| 17 | Settings: host/port/token fields, Apply (reconnect + Keychain token) | n/a (web has no client-settings screen — config is server-side only) | — | — | `SettingsView.swift:14-59` | none | none |

---

## 2. `/stacks` — Loop Stacks (the primary surface)

Web: `web/src/routes/stacks/+page.svelte` + all of `web/src/lib/components/stacks/*.svelte` + `TileGrid.svelte` + `dnd.ts`. macOS: `ForgeView.swift`, `PaneGridView.swift`, and everything in `Views/Forge/*.swift`. Backing pure-logic layer: web `stores/stack.ts`/`stackRun.ts`/`stackGoal.ts`/`stackDefaults.ts` (all unit-tested, `*.test.ts` alongside); macOS `LopiStacksKit` (all unit-tested, `LopiStacksKitTests`) — **this pure layer is the best-tested code in the whole repo on both platforms**; the E2E gap is almost entirely about proving the UI is wired to it, not about the logic itself.

### 2.1 Page shell / pane grid

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Auto-tiling layout by pane count (1 full / 2 halves / 3 thirds / 4 quarters / 6=3×2 / 9=3×3 / 12=4×3) | `TileGrid.svelte:31`; `layout-core.ts::tileDims` | **yes** — `layout-core.test.ts` (all 7 count cases) | none (routes load per `nav-parity-smoke`, no layout assertion) | `PaneGridView.swift:23,69` `PaneLayout.dims(count)` | not in LopiStacksKit — unverified/out of kit scope | none |
| 2 | Gutter drag-resize (column/row divider, floors at `minFrac`) | `TileGrid.svelte:58-92` | none (component-local pointer state) | none | `PaneGridView.swift:89-127` `resize(fr:index:extent:isCol:)` | none | none |
| 3 | Add/remove pane FLIP animation | `TileGrid.svelte:105-114` | n/a (visual) | none | `PaneGridView.swift:62-63` spring transition | n/a | none |
| 4 | Add pane (topbar `+`, max — web: unbounded via `panes` store; macOS: capped at 12) | see Global Shell #5 | see #5 | none | `ForgeView.swift:56-60` | **yes** `testPaneIsBareAndCreation` | none |
| 5 | Remove pane (floor at 1) | `stacks/+page.svelte:48-52` → `deleteStackFromPanes` | **yes** — `stack.test.ts` ("refuses to empty the last pane") | none | `ForgeView.swift:50-54,142-145` → `closePane` | **yes** `testStackLevelOps` | none |
| 6 | Repo catalog fetch on mount (falls back to "auto" if `/api/repos` unreachable) | `stacks/+page.svelte:26-38` | none (network) | none | `ForgeView.swift:42` `refreshRepos()` | n/a (network) | none |
| 7 | Drag-reorder whole stack panes between grid cells | `StackControlDock.svelte` drag handle (arms `armedPaneKey`) + `StackPane.svelte` root as drag source; drop target `StackControlDock.svelte:366-387` | **yes**, pure core — `stack.test.ts` (`reorderStacks`/`moveStackBeforeOrAfter`, before/after/out-of-range) | none | `ForgeView.swift:78-111` `.dropDestination(for: StackDragPayload.self)`, armed via `model.armedStackDragIndex` | **yes** `testStackLevelOps` (moveStackBeforeOrAfter cases) | none |
| 8 | Backend-offline banner | n/a (web shows this at layout level via connection indicator, not a stacks-specific banner) | — | — | `ForgeView.swift:115-135` | n/a | none |

### 2.2 Draft card / composer (top-pinned, `card.status === 'draft'`)

The draft *is* the composer on both platforms — no separate composer component exists in the live app (both `Composer.svelte`/`LaunchControls.svelte` on web are dead code, and no analogous macOS composer view exists outside `StackCardView`'s draft branch).

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Goal text field | `StackCard.svelte:521-532` `.goalinput` | n/a (view) | **yes** — both `popover-visibility.spec.ts:20-23` and `stack-chain-schedule.spec.ts:13-18` fill `.goalinput` as setup | `StackCardView.swift:278-359`, a11y id `stack.goalField` | n/a (view) | **yes** — both `StackChainScheduleUITests` methods |
| 2 | "Hot" state gating commit eligibility (goal text, alias, or template origin) | `StackCard.svelte:90` `draftIsHot` | **yes** — `stack.test.ts` (`testMakeDraftAndHot`-equivalent cases) | none | `StackCardView.swift:105-106` | **yes** `StackStoreTests.testMakeDraftAndHot` | none |
| 3 | `:alias` autocomplete (arrow-nav, Tab/Enter select, Esc dismiss) | `StackCard.svelte:139-168,326-348,533-539` `aliasAutocomplete`/`resolvePresetAlias` | **yes** — `stack.test.ts` | none | `StackCardView.swift:32-34,306-330,374-408` | **yes** `testAliasAutocomplete`, `testLegacyAliasResolves` | none |
| 4 | `@repo` autocomplete (trailing token, resolves label→path onto `config.repo`) | `StackCard.svelte:170-208,349-370,540-546` `repoAutocomplete`/`resolveRepoToken` | **yes** — `repoMenu.test.ts` | none | `StackCardView.swift:37-49,410-428,505-529` | **yes** `RepoMenuTests.testRepoAutocomplete`, `testResolveRepoTokenAndLabelForPath` | none |
| 5 | `/command` 2-level autocomplete (`/model`,`/effort`,`/branch`,`/autonomy`,`/eval` = value-pickers → `/command/value`; `/guard`,`/schedule`,`/maxx`(web-only) fire immediately) | `StackCard.svelte:210-324,371-392,547-553` | **yes** — `stack.test.ts` (level-1 and level-2) | none | `StackCardView.swift:57-102,318-323,433-503` `CARD_COMMANDS` (no `maxx` entry) | **yes** `StackStoreTests.testInlineCommandAutocomplete`, `testDetectPendingCommand` | none |
| 6 | Keyboard nav across all 3 suggestion lists (↓/↑/Tab/Esc) | `StackCard.svelte:326-397` | n/a (DOM) | none | `StackCardView.swift:306-329` `.onKeyPress` | n/a | none |
| 7 | Commit draft → new card, re-focus emptied input | `StackCard.svelte:133-137,393-397,666-668` `commitDraft` | **yes**, core (`finalizeDraft`) — `stack.test.ts` (inline-token stripping, `:alias`/`@repo`/`×N` parsing, repo label→path resolution) | **yes, partially** — both existing specs use `.goalinput` fill + "add" click as setup (side-effect, not directly asserted) | `StackCardView.swift:124-128,583` → `store.commitDraft` | **yes**, extensively — `testCommitDraftFlowAndDraftNeverInCards`, `testFinalizeDraftFoldsInlineTokens`, `testFinalizeDraftResolvesRepoLabelToPath`, `testAdoptRepoDefaultIfUnset`, `testFinalizeDraftKeepsConfiguredDraft` | **yes** — both `StackChainScheduleUITests` methods click `"add to stack"` |
| 8 | Templates menu trigger (draft: labeled "templates" pill) | `StackCard.svelte:516` → `TemplatesMenu.svelte` | see §2.6 | none | `StackCardView.swift:270` → `TemplatesMenuView` | see §2.6 | none |
| 9 | Provenance chips on the draft (alias/template/repo) | `StackCard.svelte:517` → `ProvenanceChips.svelte` | indirect (store-level, chip render itself untested) | none | `StackPrimitives.swift:233-269` | n/a (display) | none |

### 2.3 Committed card

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Inline goal edit (auto-grow textarea, disabled while running) | `StackCard.svelte:103-122,556-573` | none (DOM auto-grow action) | none | `StackCardView.swift` committed spec display, editable equivalent | n/a | none |
| 2 | Iteration-count stepper (± , floors at "off"=0, never wraps to ∞) | `StackCard.svelte:424-426,618-624` `stepCardIterations`/`cardIterationsLabel` | **yes** — `stack.test.ts` | none | `StackCardView.swift:571-573` `IterationPill` | **yes** `StackStoreTests.testIterationStepper` | none |
| 3 | Schedule popover (card scope) | `StackCard.svelte:625-633,707-714` → `SchedulePopover.svelte` | see §2.7 | see §2.7 (stack-scope only) | `StackCardView.swift:574-575,620-624` → `SchedulePopoverView` | see §2.7 | none (only stack-scope covered) |
| 4 | Guardrails popover (card scope — gate/until/on-fail/budget/iter) | `StackCard.svelte:634-642,724-740` → `GuardrailsPopover.svelte` | see §2.8 | none | `StackCardView.swift:576-577,625-629` | see §2.8 | none |
| 5 | Evals popover (card scope, live count badge) | `StackCard.svelte:643-651,741-743` → `EvalsPopover.svelte` | see §2.9 | none | `StackCardView.swift:578-579,630-632` | see §2.9 | none |
| 6 | MAXX popover (card scope — **web-only**, no macOS equivalent) | `StackCard.svelte:652-660,715-723` → `MaxxPopover.svelte` | see §2.10 | none | n/a — confirmed absent | n/a | n/a |
| 7 | Config-drawer toggle (inline expand, not a popover — model/effort/repo/branch/autonomy overrides) | `StackCard.svelte:661-663,702-704` → `ConfigDrawer.svelte` | `resolveBranch` unit-tested (`stackDefaults.test.ts`) | none | `StackCardView.swift:580` → `ConfigDrawerView` (`StackConfigViews.swift:165-205`) | **yes** `configActive` (`testActivePredicates`), `testDefaultResolutionPrecedence` | none |
| 8 | Templates menu (committed, icon-only) | `StackCard.svelte:670` | see §2.6 | none | `StackCardView.swift:585` | see §2.6 | none |
| 9 | Bump sooner / bump later (mid-run reorder of a queued card) | `StackCard.svelte:454-458,671-687` `bumpCard`/`bumpUiState` | **yes, extensively** — `stackRun.test.ts` bump section + pure `bumpInOrder` core `stack.test.ts` (5 distinct rejection paths) | **none** | `StackCardView.swift` — **logic implemented, no wired UI trigger found in any Forge view file** | **yes** `StackRunTests.testBumpReflectsIntoPane`/`testBumpRejectsIllegal`; `StackStoreTests.testBumpInOrder` | **none** |
| 10 | Duplicate card (resets run state) | `StackCard.svelte:428-430,689` `duplicateInPane`→`duplicateCard` | **yes** — `stack.test.ts` | none | `StackCardView.swift:586` → `duplicateCard` | **yes** `testDuplicateResetsRunState` | none |
| 11 | Drag-to-reorder within pane (arm on mousedown, HTML5 DnD, before/after by cursor Y) | `StackCard.svelte:130-166,435-493,594-618` `dnd.ts::dragging` | **yes**, pure core — `moveCardBeforeOrAfter` (all 4 direction/position combos + self-drop no-op), "reorder is provably within-pane only" | **none** | `StackCardView.swift:130-166,594-618` `.dropDestination(for: CardDragPayload.self)` | **yes** `testDragRelativeReorder` | **none** |
| 12 | Delete card | `StackCard.svelte:431-433,698` `removeFromPane`→`removeCard` | **yes** — `stack.test.ts` | none | `StackCardView.swift:588` | **yes** `testRemove` | none |
| 13 | Live iteration progress bar (visual, running only) | `StackCard.svelte:575-581` | n/a (visual) | none | `StackCardView.swift:259-261,539-548` | n/a | none |
| 14 | Hide-inactive summary lines (schedule/MAXX/guards/evals) | `StackCard.svelte:399-403,583-615` `guardActive`/`evalActive`/`configActive` | **yes** — `stack.test.ts` | none | `StackCardView.swift:552-565` `scheduleSummary`/`guardSummary`/`evalsSummary` | **yes** predicates (`testActivePredicates`); summary *strings* untested directly on either platform | none |
| 15 | Status runtag badge (new/running·iter N/M/queued/done) | `StackCard.svelte:419-422,512` | n/a (display) | none | `StackCardView.swift:215-249` | n/a | none |
| 16 | Live output panel attached below a card that has ever run | `StackPane.svelte:104-134` (see §2.11) | see §2.11 | none | `StackCardView.swift:262-265` → `LiveOutputView` (see §2.11) | see §2.11 | none |

### 2.4 Connector between cards (`StackConnector.svelte` / `StackConnectorView.swift`)

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Cadence (cron) badge, suppressed when stack-level schedule governs | `StackConnector.svelte:29,41-42` `perLoopScheduleGoverned` | **yes** — `stack.test.ts` | none | `StackConnectorView.swift:20,26-27` `cronHuman` | **yes** `testCronHelpers` | none |
| 2 | Budget badge (only when `budgetToTokens` returns a real cap) | `StackConnector.svelte:32,43-44` | **yes** — `stack.test.ts` (A3 budget test) | none | `StackConnectorView.swift:21,28-29` `budgetToTokens` | **yes** `testBudgetToTokens` | none |
| 3 | Hover-reveal "insert card here" | `StackConnector.svelte:34-36,46-48` `insertCardIntoPane`→`insertCardAt` | **yes** — `stack.test.ts` (insert-at-index, pane-keyed dispatch) | none | `StackConnectorView.swift:31-40` | **yes** `testInsertAndPatch`, `testPaneKeyedDispatch` | none |
| 4 | Dashed vs. solid spine styling by governed/scheduled state | CSS only | n/a | none | `StackConnectorView.swift:49-57` | n/a | none |

### 2.5 Stack control dock (`StackControlDock.svelte` / `StackControlDockView.swift`) — purple stack-scope control area

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Collapsible dock header (chip + summary + chevron); auto-collapses on run start only | `StackControlDock.svelte:88,311,403-410` | none (local UI state) | **yes, as setup** — both existing specs click "stack controls" to expand (not directly asserted) | `StackControlDockView.swift:179-190`, a11y id `stack.dockExpand` | n/a | **yes** — both `StackChainScheduleUITests` methods |
| 2 | Dock summary line (collapsed state) | `StackControlDock.svelte:123` `dockSummary` | `maxIterationsLabel` unit-tested | none | `StackControlDockView.swift:144-146,176` | **yes** (label fn) | none |
| 3 | Stack command bar (`@repo`/`/command` — adds `loop`,`goal`; no per-card equivalent) | `StackControlDock.svelte:129-301,420-447` `STACK_COMMANDS` | **yes** — same autocomplete core as card-scope, tested | none | `StackControlDockView.swift:34-124,274-334` `STACK_COMMANDS` | **yes** `testInlineCommandAutocomplete` ("`loop` is stack-scope only"), `RepoMenuTests.testRepoAutocomplete` | none |
| 4 | Loop-count stepper (chain repeats, off@1, ∞@0) | `StackControlDock.svelte:125-127,484-491` `stepMaxIterations` | **yes** — `stack.test.ts` | none | `StackControlDockView.swift:225-227` `IterationPill` | **yes** `testIterationStepper` | none |
| 5 | Stack schedule popover ("schedule the entire stack") | `StackControlDock.svelte:576-589` → `SchedulePopover.svelte`, `syncStackSchedule` | see §2.7 | **yes** — both existing Playwright specs | `StackControlDockView.swift:228-229,490-500`, button `"Schedule the entire stack"` | see §2.7 | **yes** — both `StackChainScheduleUITests` methods |
| 6 | Stack guardrails popover (narrower — onFail/budget only, no gate/until) | `StackControlDock.svelte:501-507` | `stackGuardActive` unit-tested | none | `StackControlDockView.swift:230-231,501-507` | **yes** `testStackPredicatesAndGoalFacet` | none |
| 7 | Stack evals popover ("chain acceptance") | `StackControlDock.svelte:508-510` | `stackEvalActive`/`evalsToAcceptance` unit-tested (A1) | none | `StackControlDockView.swift:232-233,508-510` | **yes** (`stackEvalActive` indirectly) | none |
| 8 | Stack goal popover ("pursue goal" — the closest thing to a stack-level MAXX; explicitly not a MAXX port) | `StackControlDock.svelte:511-513` → `GoalPopover.svelte` | see §2.12 | none | `StackControlDockView.swift:234-235,514-518` → `GoalPopoverView` | see §2.12 | none |
| 9 | Stack default-config popover (model/effort/repo/branch/autonomy — "every loop inherits") | `StackControlDock.svelte:511-513` → `StackConfigPopover.svelte` | `stackDefaultsActive` unit-tested | none | `StackControlDockView.swift:236-237,511-513` → `StackConfigPopoverView` | **yes** (`resolveBranch` — `StackBranchTests`; `testDefaultResolutionPrecedence`) | none |
| 10 | Stack templates menu | `StackControlDock.svelte:516` | see §2.6 | none | `StackControlDockView.swift:239` | see §2.6 | none |
| 11 | Duplicate whole stack | `StackControlDock.svelte:350-352,517` `duplicateStackInPanes`→`duplicateStack` | **yes**, thoroughly — `stack.test.ts` (clone-after-original, fresh ids, running-state reset) | none | `StackControlDockView.swift:240` | **yes** `testStackLevelOps` | none |
| 12 | Drag handle to reorder whole stacks | `StackControlDock.svelte:361-387,518-526` | see §2.1 #7 | none | `StackControlDockView.swift:241,256-270` | see §2.1 #7 | none |
| 13 | Delete whole stack | `StackControlDock.svelte:353-360,527` | **yes** — `stack.test.ts` (incl. last-pane guard) | none | `StackControlDockView.swift:242-244` | **yes** `testStackLevelOps` | none |
| 14 | Run/Pause split button ("run stack"/"pursue goal" label switch, pause↔resume) | `StackControlDock.svelte:303-339,556-563` `runMain()` | **yes, extensively** — `stackRun.test.ts` (~20+ named cases: run/pause/resume/drain/goal-pursuit outcomes) | **partial** — `stack-chain-schedule.spec.ts` exercises the dock but via the Schedule menu item, not the main run button itself | `StackControlDockView.swift:420-451,453-469` `runMain()` | **yes** `StackRunTests.testPauseThenResume` et al. | none |
| 15 | Run-menu chevron dropdown | `StackControlDock.svelte:560-572` → `RunMenu.svelte` | see below | see below | `StackControlDockView.swift:443-446` → `RunMenuView` | see below | none |
| 16 | Dismissible run-error / stop-reason / dry-run banners | `StackControlDock.svelte:341-347,533-555` | stop-reason labels unit-tested (`stackGoal.test.ts`); dismiss action itself untested | none | `StackControlDockView.swift:392-397,476-486` | **yes** `StackGoalTests.testEveryReasonHasDistinctNonEmptyLabel`; `dryRunText` formatting untested | none |
| 17 | "Pursue goal" vs. "run stack" label switch | `StackControlDock.svelte:453-460` `pursues` | **yes** — `stackPursuesGoal` (`stack.test.ts`) | none | `StackControlDockView.swift:453-460` | **yes** `testStackPredicatesAndGoalFacet` | none |

**Run-menu items** (`RunMenu.svelte` / `RunMenuView.swift`): Run now, Run once, Schedule stack, Dry run, Pause, Resume, Drain.

| Item | Web UT | Web E2E | macOS UT | macOS XC |
|---|---|---|---|---|
| Run now | **yes** — `stackRun.test.ts` | none | **yes** `StackRunTests.testOrderingBottomToTop` | none |
| Run once | **yes** — `stack.test.ts` (forces `max_iterations=1`) + `stackRun.test.ts` | none | **yes** `testRunOnceNeverPursues` | none |
| Schedule stack | **yes** — `stackRun.test.ts` ("exactly one POST, not one per card") | **yes** — both existing Playwright specs | **yes** `testScheduleStackWiresEveryCard` | **yes** — both `StackChainScheduleUITests` methods |
| Dry run | **yes** — `stack.test.ts` ("validates without ever calling createTask") | none | **yes** `StackStoreTests.testDryRun` | none |
| Pause / Resume / Drain | **yes** — `stackRun.test.ts` | none | **yes** `testPauseThenResume`, `testDrainNotResumable` | none |
| Close menu on outside-click/Esc | none | none | n/a | none |

### 2.6 Templates (card-scope `TemplatesMenu`/`TemplatesMenuView`; stack-scope `StackTemplatesMenu`/`StackTemplatesMenuView`)

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Apply a built-in preset (`:alias`) | `TemplatesMenu.svelte:99-102` `applyPreset` | **yes** — `stack.test.ts` | none | `TemplatesMenuView.swift:98-109` | **yes** `testApplyPresetClearsProvenance` | none |
| 2 | Apply a saved prompt template | `TemplatesMenu.svelte:103-106` `applyPromptTemplate` | **yes** — `stack.test.ts` | none | `TemplatesMenuView.swift:111-125` | **yes** `testProvenanceSurvivesEdit` | none |
| 3 | Save current card as a prompt template (disabled unless "hot"; `window.prompt()` dialog) | `TemplatesMenu.svelte:107-112` `savePromptTemplate` → `templates.ts` (localStorage) | pure conversion (`promptTemplateFromCard`) tested; **persistence itself untested — no `templates.test.ts`** | none | `TemplatesMenuView.swift:54-58,127-134,172-176` → `StackTemplateStore.savePrompt` | pure `promptTemplate(from:)` tested (`testPromptTemplateFromCard`); **`StackTemplateStore.savePrompt` persistence itself has no test file** | none |
| 4 | Apply a saved stack template (whole chain) | `StackTemplatesMenu.svelte:77-80` `applyStackTemplateToPane`→`applyStackTemplate` | **yes** — `stack.test.ts` (bottom-first round-trip) | none | `StackTemplatesMenuView.swift:68-76` | **yes** `testStackTemplateBottomFirstRoundTrip`, `testStackTemplateLoopProvenance` | none |
| 5 | Copy another open pane's cards ("saved stacks") | `StackTemplatesMenu.svelte:81-84` `loadStackCardsIntoPane`→`loadStackCardsInto` | **yes** — `stack.test.ts` (fresh ids, source untouched, running-state reset, self-copy/unknown-source no-ops) | none | `StackTemplatesMenuView.swift:85-91` | **yes** `testLoadStackCardsInto` | none |
| 6 | Save whole stack as a template (disabled when no cards; `window.prompt()` dialog) | `StackTemplatesMenu.svelte:85-90` `saveStackTemplate` → `templates.ts` | pure conversion tested; **persistence untested** | none | `StackTemplatesMenuView.swift:95-102,141-145` → `StackTemplateStore.saveStack` | pure `stackTemplate(from:)` tested; **`saveStack` persistence untested** | none |
| 7 | Menu positioning (flip-above, outside-click/Esc/scroll dismiss) | both menu files | none | none | both view files | n/a | none |

### 2.7 Schedule popover (`SchedulePopover.svelte` / `SchedulePopoverView.swift`) — used at both card and stack scope

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Enable toggle ("run on a schedule") | `:57-60` | none directly (round-trip via `guardActive`/summary fns) | **yes** — both existing Playwright specs toggle this exact control | `:27-30`, a11y id `stack.scheduleToggle` | n/a | **yes** — both XCUITest methods |
| 2 | Frequency chips (minute/hourly/daily/weekly/custom) | `:63-69` | **yes** — `buildCronString`/`cronHuman` per-freq (`stack.test.ts`) | none | `:44-58` | **yes** `testCronHelpers` | none |
| 3 | Weekly: day-of-week + hour/min + AM/PM | `:71-83` | **yes** (weekly case) | none | `:62-68,84-104` | **yes** (weekly case) | none |
| 4 | Daily: hour/min + AM/PM | `:84-94` | **yes** | none | `:69-73` | **yes** | none |
| 5 | Hourly: minute-only | `:95-99` | **yes** | none | `:74-78` | **yes** | none |
| 6 | Raw cron text field (custom escape hatch — regression-guarded against "snapping" a custom expr to a preset) | `:102-105,36-39` | **yes** — `stack.test.ts` (the V&V-flagged regression case) | none | `:106-116` | **yes** (`custom` case) | none |
| 7 | Human-readable + cron-string preview | `:36-37` | **yes** — `cronHuman`/`buildCronString` | none | `:36-37` | **yes** | none |
| 8 | "Next runs" preview (up to 3) | `:110-121` `computeNextRuns` | **yes** — `stack.test.ts` (table-driven) | none | `:118-128` `computeNextRuns` | **yes** `testComputeNextRuns` | **yes** — both XCUITest methods assert `"next runs:"` renders |
| 9 | Stack-scope variant additionally calls `syncStackSchedule` | `StackControlDock.svelte` reuse | real chain-schedule path (`scheduleStack`) tested; `syncStackSchedule` itself is a documented **stub** | **yes** — both existing Playwright specs | `AppModel.syncStackSchedule` (app-level, not in kit) | untested at kit level | **yes** — both XCUITest methods |

### 2.8 Guardrails popover (`GuardrailsPopover.svelte` / `GuardrailsPopoverView.swift`) — card (`.loop`) and stack (`.stack`) scope

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Gate toggle + shell-cmd field (loop scope only) | `:48-50,71-82` | **yes**, via WIRED-fields round-trip table (`stack.test.ts`) | none | `:48-50,71-82` | **yes** `testWiredTableRoundTrip`, `testActivePredicates` | none |
| 2 | Until toggle + shell-cmd field (loop scope only) | `:51-53` | **yes** (same table) | none | `:51-53` | **yes** (same) | none |
| 3 | On-fail segmented control (stop/continue/backoff) | `:55-57` | **yes** — chain on-fail behavior end-to-end (`stackRun.test.ts`) | none | `:55-57` | **yes** `StackRunTests.testChainOnFailStop/Continue/Backoff` | none |
| 4 | Budget segmented control (auto/200k/none) | `:58-60` | **yes** — `budgetToTokens` (A3) | none | `:58-60` | **yes** `testBudgetToTokens` | none |
| 5 | Max-iterations stepper | `:88-96` | **yes** | none | `:92-102` | **yes** `testIterationStepper` | none |
| 6 | `scope='stack'` suppresses gate/until rows | `:55-66` | n/a | none | same pattern | n/a | none |

### 2.9 Evals popover (`EvalsPopover.svelte` / `EvalsPopoverView.swift`) — card and stack ("chain acceptance") scope

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Per-eval checkbox toggle (baseline locked-on) | `:36-50` `toggleEval` | **yes** — `stack.test.ts` | none | `:44-68` `toggleEval` | **yes** `testEvalOps` | none |
| 2 | Suite-apply buttons (kcqf/security/research) | `:52-59` `applySuite` | **yes** — `stack.test.ts` | none | `:26-38` `applySuite` | **yes** `testEvalOps`, `testInlineCommandAutocomplete` | none |
| 3 | Tier badge coloring | display only | n/a | none | `:70-77` | n/a | none |
| 4 | Stack-scope reuse → "chain acceptance" (drives `evalsToAcceptance`) | `stack.ts::evalsToAcceptance` | **yes** — `stack.test.ts` (A1 section) | none | `evalsToAcceptance` (kit) | **yes** `testEvalsToAcceptance` | none |

### 2.10 MAXX popover (`MaxxPopover.svelte`) — web-only, card scope

| # | Feature | Web impl | Web UT | Web E2E |
|---|---|---|---|---|
| 1 | Enable/disable toggle, wired to real `/api/maxx` CRUD (create-on-first-enable) | `:40-71` | **partial** — `api.test.ts` covers `createMaxx`/`enableMaxx` transport (path/method/body); **not `disableMaxx`, not the popover's own `toggle()` orchestration** (busy-state guard, error handling, entryId threading) | none |
| 2 | Quota display (5h/7d window bars, fetched via `getQuota()`) | `:32-38,120-132` | `getQuota` transport tested (`api.test.ts`); popover's own formatting helpers (`fmtHour12`/`resetIn`/`resetOn`/`windowText`/`pct`) are component-embedded and untested | none |
| 3 | Fixed "run" policy text (quiet hours + headroom gate) | `:114-118` | n/a (static text) | none |
| 4 | Duplicate-card MAXX-entry non-sharing | n/a (cross-cutting, see §2.3 #10) | **yes** — `stack.test.ts` ("duplicate never shares the original's backend entry") | none |

Backend: `/api/maxx` full CRUD is the **most exhaustively tested route in the whole Rust surface** — see §4.10.

### 2.11 Live output panel (`StackOutput.svelte` / `LiveOutputView.swift`)

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Expand/collapse (strip ↔ full panel) | `:34,85-105` | none (local boolean) | none | `:41-54,51,64` | none | none |
| 2 | Category filter chips (all/thinking/actions/tools/output) | `:26,35,97-101` | none — `categorize()` is component-embedded, not exported | none | `:63,118-127` | none — `TranscriptBlock` categorization lives in the app target, not the kit | none |
| 3 | Per-section collapse/expand (independently toggled) | `:36-41,68-70,106-126` | none | none | `:80-110` | none | none |
| 4 | **No copy-to-clipboard control exists** — confirmed absent on web by full file read | — | — | — | n/a | — | — |
| 5 | Live vs. "logs" relabel once no longer running | `:21,87,96` | indirect — `card.taskId` persistence-after-completion covered by `stackRun.test.ts` | none | `LiveOutputView.swift` | indirect (`StatsParityTests` is unrelated dashboard scope, not this) | none |
| 6 | Reads `stores/transcript.ts`'s per-`task_id` block feed | — | `transcript.test.ts` exists and covers block parsing (component's *consumption* of it untested) | none | — | — | — |

macOS-only: `TranscriptView.swift` (markdown/diff rendering, collapsible thinking block, tool-call disclosure, auto-scroll, blinking caret) is mounted only via the orphaned `AgentPaneView` — **not reachable from any live nav path today**, so it has no coverage and shouldn't be planned for until it's wired in.

### 2.12 Goal popover (`GoalPopover.svelte` / `GoalPopoverView.swift`) — stack scope only

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Pursue-goal toggle | `:34` | **yes** — `stackGoalActive`/`stackPursuesGoal` (`stack.test.ts`) | none | `:24` | **yes** `testStackPredicatesAndGoalFacet` | none |
| 2 | Inert-goal hint (pursue on, no real acceptance evals) | `:37-39` | **yes** (same test) | none | `:27-30` | **yes** (same) | none |
| 3 | No-progress-limit stepper (0 = off) | `:40-48` | **yes**, thoroughly — `stackGoal.test.ts`'s full decision core (`decideAfterMiss` 7 cases, `foldGain`, `precede`, `stackStopLabel`) | none | `:42-57` | **yes** `StackGoalTests.testDecideAfterMiss` (incl. limit=0), `StackRunTests.testGoalHaltsOnNoProgress` | none |

### 2.13 Config drawer / config popover / repo picker / other shared primitives

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Model/effort/repo/branch/autonomy dropdowns | `ConfigDrawer.svelte:55-107`, `StackConfigPopover.svelte:40-57` | branch resolution tested (`stackDefaults.test.ts`) | none | `StackConfigViews.swift` `StackDropdown`, `ConfigDrawerView`, `StackConfigPopoverView` | **yes** (`StackBranchTests`, `testDefaultResolutionPrecedence`) | none |
| 2 | Repo picker: search-to-filter, grouped/sectioned, escape-clears-then-closes | web reuses inline autocomplete (no separate picker component); macOS has a dedicated `RepoPickerView.swift` | `groupedMenu`/`repoOptions` tested (`repoMenu.test.ts`) | none | `RepoPickerView.swift:36-124` | **yes**, extensively — `RepoMenuTests` (filtering, grouping, golden-fixture parity with web via `crates/lopi-ui/tests/fixtures/repo_menu_golden.json`) | none |
| 3 | Branch auto-resolve + write-back on repo change | `stackDefaults.ts::resolveBranch` | **yes** — `stackDefaults.test.ts` (9 cases) | none | `StackConfigViews.swift:141-144,183-186` `resolveBranch` | **yes** — `StackBranchTests` (6 methods incl. idempotence), golden-fixture-parity confirmed with web | none |
| 4 | Shared primitives (toggle pill, segmented control, ± combo, iteration pill, cardbar icon button, provenance chips, summary row) | `Toggle.svelte`, `Combo.svelte`, plus inline styling in `StackCard`/`StackControlDock` | trivial/none at component level (consumer state changes are tested at store level) | none | `StackPrimitives.swift` (`StackToggle`, `StackSegmented`, `StackCombo`, `IterationPill`, `CardbarButton`, `ProvenanceChips`, `SummaryRow`, drag-payload types) | n/a (dumb views); label-producing functions they call ARE tested | `CardbarButton` partially exercised via a11y ids `"add to stack"`/`"Schedule the entire stack"` in `StackChainScheduleUITests` |

---

## 3. `/loop` — Loop Engineering

Web: `web/src/routes/loop/+page.svelte`. macOS: `LoopView.swift`. Read-mostly surface — writes are limited to self-prompt strategy, escalation toggle, and per-schedule trust level.

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Initial load (parallel config/health/runs fetch) | `:47-62` | none client-side | `nav-parity-smoke` load-only | `LoopView.swift` on-appear fetch | none | none |
| 2 | Loop Health stat cards (success rate/verifier pass/runs/spend/tokens) | `:240-263` | none | none | `:69-133` | none | none |
| 3 | Sparklines (score/attempt, context pressure, diff size, cost burn) | `:265-311` | none | none | `:69-133` | none | none |
| 4 | Outcome distribution bar | `:314-336` | none | none | included in health tiles | none | none |
| 5 | Recent Runs — click to expand/collapse attempt trace | `:64-81,346-442` | none | none | `:165-217` `model.selectRun` | none | none |
| 6 | Per-attempt trace (stage row, metrics, verifier verdict/gaps, errors) | `:383-436` | none | none | same | none | none |
| 7 | Effective config panel (validity/issues) | `:446-493` | none | none | `:269-297` | none | none |
| 8 | Autonomy ladder display (L1–L4) | `:496-506` | none | none | `:301-322` | none | none |
| 9 | Self-prompt strategy picker (writes `.lopi/loop.toml`) | `:161-171,509-516` `setLoopStrategy` | none client-side | none | `:326-373,416-440` `setLoopStrategy` | none | none |
| 10 | Strategy card click → preview focus (no save) | `:524-541,151-159` | none | none | preview mechanism in `LoopView` | none | none |
| 11 | Adaptive escalation toggle | `:189-198,564-578` `setLoopEscalation` | none | none | `:376-414` `setLoopEscalation` | none | none |
| 12 | Scheduled Loops — per-schedule trust-level dropdown | `:140-149,599-629` `setScheduleAutonomy` | none | none | `:461-498` `setScheduleAutonomy` | none | none |
| 13 | Skills / Rules / Quality Gates read-only panels | `:633-681` | none | none | `:502-560` | none | none |

**Backend** — see §4.4 (Loop Engineering). Summary: `get_loop`/`set_strategy`/`set_escalation` are well-tested end-to-end (5 tests). `GET /api/loop-engineering/health` and `GET /api/loop-engineering/runs` (list) have **no HTTP-level test at all** — only pure formatting helpers are unit-tested; the 404 path on `GET /api/loop-engineering/runs/:id` is untested. The autonomy-ladder/self-prompt/escalation *domain logic* in `lopi-core` (`loop_config.rs`, `self_prompt.rs`, `earned_trust.rs`) is the single best-tested area of the entire backend (34 tests across the three files).

---

## 4. `/budget` — Budget

Web: `web/src/routes/budget/+page.svelte`. macOS: `BudgetView.swift`.

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | "Stop all running" button | `:28-32,51-58` | none | none | `:63-69,190-194` `model.cancelTask` per agent | none | none |
| 2 | Stat cards (spent/burn-rate/cap/time-to-cap) | `:62-69`; `stores/budget.ts::fleetBudget` | **none — `budget.ts` has no test file at all** | none | `:47-50,95-113` | none | none |
| 3 | Burn-vs-cap meter, 75% warn marker, color by state | `:71-89` `budgetColor` | none | none | `:95-113` | none | none |
| 4 | Hourly cap numeric input + presets ($1/5/10/25/50) | `:90-114` | none | none | `:115-123,196-199` (persists to `UserDefaults`) | none | none |
| 5 | Top spenders list (top 8, live pulse dot) + per-agent stop | `:21-25,117-145` | none | none | `:131-173` | none | none |
| 6 | Breach history panel | `:148-162`; `events.ts::budgetAlerts` | `describe()` tier/summary logic unit-tested (`events.test.ts`); this render untested | none | `:175-186` | none | none |
| 7 | `startBudgetSampler` periodic sampling | `:17` | none | none | sampled every 2s (`:47-50`) | none | none |

**Backend note:** there is **no dedicated `/api/budget` route on either platform** — confirmed by grepping every `.route(` call in `crates/lopi-ui/src/web/mod.rs`; this page is entirely client-derived from live `agents`/task-cost data plus a client-local (unpersisted-server-side) hourly cap. The one real server concept in this area is account-wide rate-limit quota, `GET /api/quota` (backs MAXX's quota bars, not this page) — see §4.3 and §2.10. `AgentEvent::BudgetExceeded` (the wire event that drives the breach toast) has **zero test coverage repo-wide**.

---

## 5. `/schedules` — Scheduling (standalone cron CRUD, distinct from stack-chain scheduling)

Web: `web/src/routes/schedules/+page.svelte`. macOS: `CronView.swift` + `CronPresets.swift`.

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Schedule list render | `:142-210` | none | none | `CronView.swift` list | none | none |
| 2 | "+ new" → blank create form | `:45-50,122-128` | none | none | `:28-39` new-schedule sheet | none | none |
| 3 | "edit" → pre-filled form | `:52-64,194-198` | none | none | `:40-44,96` edit sheet | none | none |
| 4 | Create/update submit (validates name/cron/goal) | `:66-82,227-289` | none | none | `:83-89` via `model.saveSchedule` | none | none |
| 5 | Close form without saving | `:214-225` | none | none | sheet dismiss | none | none |
| 6 | "run now" | `:84-94,173-180` `runScheduleNow` | none | none | `:92-95` `model.runScheduleNow` | none | none |
| 7 | "pause"/"enable" toggle | `:181-191` | none | none | `:83-89` `model.toggleSchedule` | none | none |
| 8 | "delete" | `:199-205` `deleteSchedule` | none | none | `:100-103` `model.deleteSchedule` | none | none |
| 9 | Priority select | `:264-274` | none | none | `:165-169` | none | none |
| 10 | Frequency picker → cron builder | web reuses `SchedulePopover`-style inline fields directly in the form | see §2.7 for the shared cron logic (this page's own form fields are separate markup, untested) | none | `:145-220`, `CronPresets.swift::CronSpec` (`.cron`/`.summary`/`.parse`) | **none — `CronSpec` is a completely separate implementation from `LopiStacksKit/StackCron.swift` and has no test file** | none |
| 11 | Live cron preview (human + raw) | inline in form | none | none | `:242-254` `CronSpec.summary`/`.cron` | **none** | none |
| 12 | Auto-refresh every 15s | `:102-106` | none | none | n/a (no auto-refresh noted) | — | — |

**Backend** — `/api/schedules` full CRUD+lifecycle: **yes**, 14 tests in `schedules_tests.rs` (create/list/get/update/enable/disable/delete/run-now, incl. 404 and validation-rejection paths). **Gap**: `POST /api/schedules/:id/autonomy` (`set_autonomy`) — confirmed **zero test coverage at any level**, including the documented "normalize unknown values to draft_pr" fallback. See §4.2.

---

## 6. `/overview` — Overview

Web: `web/src/routes/overview/+page.svelte`. **macOS has no equivalent view** — confirmed in `PARITY_AUDIT_2026-07-16.md` §4 as a deliberate, already-logged cut (`macOS-Parity-Cut-1`), not a bug.

| # | Feature | Web impl | Web UT | Web E2E |
|---|---|---|---|---|
| 1 | Row derivation from live `agents` + `permissionWaiting` | `:28`; `stores/overview.ts::overviewRows` | **yes** — `overview.test.ts` (goal/repo/phase carried through, failed→hardStop, completed→kryptonite) | none |
| 2 | Lifecycle filter chips (all/running/queued/done/dead-letter) with live counts | `:34-40,63-75` `filterCounts`/`filterRows` | **yes** — `overview.test.ts` ("running filter", "dead-letter folds failed+cancelled") | none |
| 3 | Row click/Enter/Space → focus agent, navigate to `/stacks` | `:42-45,105-109` | none (navigation side-effect) | none |
| 4 | Offline/connecting banner | `:31,77-80` | none | none |
| 5 | Idle (connected, zero rows) banner | `:32,81` | none | none |
| 6 | No-rows-for-filter banner | `:83-84` | none | none |
| 7 | Table columns + FLIP row animation | `:86-137` | `formatElapsed` unit-tested (0ms→0s, sub-minute, minute+seconds, negative-clamp) | none |
| 8 | Score color thresholds | `:47-51` | none | none |
| 9 | Orb-color row dot + motion vocabulary | `:104,111,226-257` | not this table (orb color mapping is exercised by `forge/orbState.test.ts`/`cardOrb.test.ts` elsewhere) | none |

**Backend:** the underlying `/api/tasks` list/get/cancel and the `/ws`+`/ws/tasks` live-update sockets are what actually feed this page. `list_tasks`/`get_stats` are well-tested; `get_task`/`cancel_task` only have their 404 paths tested (no successful-get/cancel test found); **`/ws` and `/ws/tasks` have zero test coverage of any kind** — confirmed by grep, zero hits for `ws_handler`/`WebSocket` in any test file — and per the web `api.ts` comments this socket is the actual live-data path Overview depends on, making it a high-priority backend gap. See §4.5.

macOS-exclusive, no web equivalent: `DashboardView.swift` — live stat tiles, budget radial gauge + sparklines, activity ticker, agent cognition grid. Flagged as a new (previously undocumented) one-way parity gap in `PARITY_AUDIT_2026-07-16.md` §4.

---

## 7. `/config` — Configuration

Web: `web/src/routes/config/+page.svelte`. macOS: `ConfigView.swift`.

| # | Feature | Web impl | Web UT | Web E2E | macOS impl | macOS UT | macOS XC |
|---|---|---|---|---|---|---|---|
| 1 | Server identity panel (service/version/uptime) | `:58-78` | none | none | `:66-92` | none | none |
| 2 | Appearance theme picker | `:82-102`; `stores/theme.ts::setTheme` | **none — `theme.ts` has no test file** | none | n/a (no theme picker found in `ConfigView`) | — | — |
| 3 | Tree/raw view toggle | `:110-131` | none | none | n/a | — | — |
| 4 | Config tree view (flattened rows, redaction highlight, bool/number coloring) | `:36-49,150-174` `flatten()` | none (local fn, unexported) | none | `:66-92` pretty-print (read-only) | none | none |
| 5 | Config raw JSON view | `:143-149` | none | none | n/a | — | — |
| 6 | Clear result cache | n/a (no web equivalent found) | — | — | `:38-46` `model.clearCache` | none | none |
| 7 | Refresh button | n/a | — | — | `:21-27` | none | none |

**Backend** — `GET /api/config` / `GET /api/version`: **yes, thorough, at 3 levels** — unit (`redact_blanks_present_secrets_only`, `redact_is_noop_when_fields_absent`), HTTP (`config_endpoint_reflects_loaded_config`, `config_endpoint_reports_none_without_config`, `config_endpoint_returns_200`, `version_endpoint_reports_service_and_version`), plus 26 tests on `LopiConfig` itself in `lopi-core/config_tests.rs`. Gap: `MaxxEntry`'s quiet-hours/headroom fields and `LimitWindow::parse`/`as_str` have no dedicated test. `/api/cache/*` (backs the macOS "clear cache" button): only empty-store paths tested, no test seeds entries to verify non-zero counts get cleared.

---

## 8. `/onboard` — web-only, no macOS equivalent, not in the nav

`web/src/routes/onboard/+page.svelte`. Never a visible tab (per `nav.ts`'s own doc comment); reachable only by direct URL.

| # | Feature | Web impl | Web UT | Web E2E |
|---|---|---|---|---|
| 1 | "Install GitHub App" CTA → redirects to `:3002/app/install` | `:7-10,51-53` | none | none |
| 2 | Static onboarding steps + pricing display | `:26-94` | none | none |

---

## 9. Backend (Rust) logic coverage, by feature area

This is the cross-cutting layer the UI tables above reference. Route table lives in `crates/lopi-ui/src/web/mod.rs:195-345` (`build_app`), all under `/api/*`, gated by rate-limit + auth middleware.

### 9.1 Stack-chain scheduling
Routes: `schedule_chain_handlers.rs` — create/list `/api/schedule-chains`, get/update/delete `/api/schedule-chains/:id`, enable/disable, run-now.
**Coverage: yes, full CRUD+lifecycle.** `schedule_chains_tests.rs` (11 tests): `create_chain_returns_201_with_next_runs_and_steps_in_order`, `create_chain_rejects_invalid_cron`/`_empty_steps`/`_empty_step_goal`, `list_chains_includes_created`, `get_chain_unknown_returns_404`, `get_chain_includes_run_history_field`, `update_chain_replaces_steps`, `update_unknown_chain_returns_404`, `chain_enable_and_disable_toggle_flag`, `delete_chain_then_get_returns_404`, `run_now_returns_202_and_submits_only_step_zero`, `chain_run_now_unknown_returns_404`.
Engine: `crates/lopi-orchestrator/src/chain_schedule_manager.rs` — `chain_schedule_manager_tests.rs` (10 tests, confirmed via direct read): `start_is_idempotent`, `register_and_unregister_roundtrip`, `run_now_submits_only_step_zero`, `run_now_on_chain_with_no_steps_returns_none`, `task_completion_advances_to_next_step_and_finishes_chain`, `on_fail_stop_ends_run_without_submitting_next_step`, `on_fail_continue_advances_past_a_failed_step`, `resume_orphaned_advances_when_step_already_finished_before_restart`, `resume_orphaned_resubmits_the_in_flight_step_when_task_was_lost`. Plus a real-SQLite process-restart integration test (`tests/chain_schedule_resume.rs`).
**Confirmed gap:** `OnFail::Backoff` branch untested at the chain-manager level. **No pause/resume/drain of an in-flight chain run exists on the backend at all** — independently verified: `grep -n "pause\|resume\|drain" chain_schedule_manager.rs` only matches boot-time crash-recovery (`resume_orphaned`), never a user-initiated pause of a live run. The frontend's `pauseStack`/`resumeStack`/`drainStack` (`stackRun.ts`) have no server-side counterpart — **highest-value backend gap for stack-run E2E planning**, since it means "pause a running scheduled chain" can only ever be a client-local (non-persisted) UX today.

### 9.2 Single cron schedules
Routes: `schedule_handlers.rs` — CRUD/lifecycle on `/api/schedules[/:id]`, plus `POST /api/schedules/:id/autonomy`.
**Coverage: yes for CRUD+lifecycle (14 tests in `schedules_tests.rs`), NO for autonomy.** `POST /api/schedules/:id/autonomy` — confirmed via grep, zero hits for "autonomy" in any test file in the module; neither the happy path nor the documented "normalize unknown values to draft_pr" fallback is exercised.
Engine: `schedule_manager.rs` (7 tests) — register/unregister/run_now/history/idempotent-start all covered.

### 9.3 Budget / cost tracking
No dedicated per-task-budget route. `GET /api/quota` (`quota_handlers.rs`) is account-wide rate-limit quota, not per-task budget — **yes**, 2 tests (`quota_reports_null_for_unobserved_windows`, `quota_reflects_observed_windows_independently`).
`budget_tokens` (per-task token ceiling on `POST /api/tasks`, wired in `apply_loop_fields`): **no test sends a `budget_tokens` value and checks it lands on the task**, confirmed by direct grep.
Engine: `quota_tracker.rs` (4 tests). lopi-core: `budget.rs::BudgetScope` (2 tests, serde-only), `stop_reason.rs::StopReason::Budget` (5 tests, precedence `GoalMet > Budget > NoProgress > MaxIterations` pinned).
**Confirmed gap:** `AgentEvent::BudgetExceeded` (`lopi-core/src/event.rs`) has **zero test coverage repo-wide** — this is the actual wire event the web toast/Telegram/webhook integrations would consume.

### 9.4 Loop engineering config
Routes: `loop_handlers.rs` (`get_loop`/`set_strategy`/`set_escalation`) — **yes**, 5 tests, thorough (`loop_tests.rs`: snapshot carries self-prompt catalog, strategy persists to `.lopi/loop.toml` and round-trips, unknown-tag rejection confirmed to not write the file, escalation ladder/flag round-trip).
`loop_health_handlers.rs::get_loop_health` (`GET /api/loop-engineering/health`): **no HTTP-level test** — only pure helpers unit-tested in-file (ratio-zero-denominator guard, headline counts, time-ordering, outcome-series mapping).
`loop_runs_handlers.rs::list_runs` (`GET /api/loop-engineering/runs`): **zero coverage, any level.**
`loop_runs_handlers.rs::get_run_trace` (`GET /api/loop-engineering/runs/:id`): no HTTP-level test; pure-function helpers tested; the 404 path is untested at any level.
lopi-core domain (**best-tested area in the whole backend**): `AutonomyLevel` (`loop_config.rs`, 9 tests — rank-monotonicity, capability-gates, promote/demote-saturation), `SelfPromptStrategy`/escalation (`self_prompt.rs`, 13 tests), `EarnedTrust` promotion/demotion (`earned_trust.rs`, 8 tests + 4 more on `promote_after`/`trust_ceiling`).

### 9.5 Overview / agent listing
Routes: `handlers.rs` (`list_tasks`/`get_stats`/`get_task`/`cancel_task`/`approve_plan`/`reject_plan`), `metrics_handlers.rs` (`/api/agents/:id/dag`), `repos_handlers.rs` (`/api/repos`,`/api/branches`), `/sse`, `/ws`, `/ws/tasks`, task-stream/logs endpoints.
- `health`/`get_stats`/`list_tasks`: **yes** (4 tests).
- `get_task`/`cancel_task`: only 404 paths tested — no test for a successful get/cancel on an existing task.
- `approve_plan`/`reject_plan` (the plan-approval workflow): **zero coverage** — confirmed via grep across all test files and independently by grepping `handlers.rs` for the function definitions with no matching test-file references; neither the happy path, a 409-conflict, nor a 404 is exercised.
- `GET /api/spec`: zero coverage in this module.
- `GET /api/repos`, `GET /api/branches`: **no HTTP-level test for either route** — only pure helpers (`scan_repos`, `git_branches`/`is_generated_branch`/`current_branch`) are unit-tested; `is_generated_branch` specifically confirms `lopi/*`/`claude/*` prefixes are hidden while a near-miss like `feat/claude-integration` is not filtered.
- `GET /api/agents/:id/dag`: **yes** (`f8_id_scoped_reads_status_codes` + unit tests on `dag_graph_json`).
- `/sse`: yes but shallow (handshake only — 200 + `text/event-stream` content-type — not actual event delivery).
- **`/ws`, `/ws/tasks`: zero test coverage** — confirmed via grep for `ws_handler`/`WebSocket`, zero hits in any test file. Per the web `api.ts` comments, this socket is the actual live-data path backing the Overview page's real-time updates, making this the single highest-priority backend gap surfaced by this audit.
- Task stream/logs: **yes, thorough** — `task_stream_rejects_malformed_id_with_400`, `f8_id_scoped_reads_status_codes`, `task_stream_isolates_concurrent_tasks_with_zero_cross_talk` (2 concurrent SSE subs, 20 interleaved events, zero cross-talk asserted), plus 4 logs tests.
- `GET /api/quality/trend`, `GET /api/routing/q-values`: **no coverage found for either** (these are also the "orphaned backend routes" already flagged in `PARITY_AUDIT_2026-07-16.md` §4 as having zero UI callers on either platform — doubly untested and doubly unused).
- Orchestrator: `AgentPool` submit/queue/topology/rate-limits well tested; **`run()`/`run_one()` dispatch loop and a real in-flight `cancel()` success path are untested anywhere** — only the `cancel_nonexistent_task_returns_false` no-op path is covered.

### 9.6 General config
`GET /api/config` / `GET /api/version`: **yes, thorough** — see §7 above for the full breakdown.

### 9.7 Guardrails (gate/until/on_fail)
Not a standalone route — fields on `POST /api/tasks`. **Partial coverage**: `create_task_with_guardrail_fields_returns_201` posts a full guardrails payload but only asserts `201 CREATED` — no round-trip GET confirms persistence. The field-mapping logic itself IS separately proven by a pure unit test (`apply_loop_fields_threads_gate_until_and_on_fail`) that bypasses HTTP/JSON deserialization entirely — so the full request→response contract for `on_fail:"backoff"` specifically is unverified end-to-end.
lopi-core `loop_config.rs`: `gate`/`until`/`OnFail`/`run_guard_command()` — well covered, 9 tests (incl. exit-code reflection, cwd-correctness, backward-compat deserialization of pre-guardrail configs).
Gap: `Task.gate`/`Task.until`/`Task.on_fail` per-task override fields have no dedicated test of their own; task-vs-repo-config precedence resolution lives in lopi-agent (out of scope of this audit's crate coverage).

### 9.8 Evals
**No backend route exists** — confirmed zero "eval"/"acceptance" hits in any route declaration across `crates/lopi-ui/src/web/*.rs`. The `acceptance` field IS accepted on `POST /api/tasks` (mapped in `apply_loop_fields`) but **no test constructs a request with an `acceptance` payload at unit or HTTP level** — confirmed gap.
lopi-core `acceptance.rs` (`EvalTier`, `Op`, `MetricGate`, `CheckSpec`, `Acceptance`): 13 inline tests (tier-ordering, operator coverage, JSON round-trip). Companion `eval_outcome.rs`: 11 tests, notably a fail-closed precedence pin (`a_required_error_beats_a_fail_and_is_not_passing`). Downstream `gain.rs`: 14 tests incl. several mutation-style pins.
Gap: the tiered *executor* that actually runs `CheckSpec::Shell`/calls a judge model lives in `lopi-agent`, unaudited by this pass.

### 9.9 Goal editing
**No backend route exists.** `mod.rs` only registers GET/DELETE on `/api/tasks/:id` — no PUT/PATCH anywhere. A goal is set once at creation and validated by `handlers.rs::validate_goal`.
Creation-time validation coverage: **yes, thorough** — `create_task_rejects_oversized_goal`, `create_task_accepts_valid_goal`, `create_task_rejects_empty_goal`, `create_task_rejects_whitespace_only_goal`, plus a pure `validate_goal_table` test (empty/whitespace/over-length/NUL/ANSI-escape rejection, unicode/emoji/multiline acceptance).
**Note for the test plan:** "goal editing" on either client (the inline textarea in `StackCard.svelte`/`StackCardView.swift`) is client-local draft-state editing before submit — there is no server concept of editing an existing task's goal post-creation. Don't plan a backend-round-trip E2E test for this; it's client-only by design.

### 9.10 MAXX / model catalog
**MAXX** (`/api/maxx` full CRUD): **the most exhaustively tested CRUD surface in the whole crate** — 12 tests in `maxx_tests.rs` (create+201, 4 distinct validation-rejection cases, list, get+404, update, delete, enable/disable+404). Engine `maxx_loop.rs` (lopi-orchestrator): exhaustively tested (quiet-hours wraparound/degenerate bounds, headroom AND-semantics, cooldown/refire-suppression).
**Model catalog** (`GET /api/models`): **yes, at 2 levels** — unit (mixed bool/object/false effort-tier shape decoding locked in; TTL-cache fresh/stale/expired behavior) and e2e (`models_returns_a_valid_catalog`, deliberately live-or-fallback rather than mocked). lopi-core `models.rs`: 4 tests.
Naming note: `maxx_loop.rs` (orchestrator) is the MAXX dispatch engine, NOT model-catalog logic, despite the name similarity — the real model catalog is `model_handlers.rs` + `lopi-core/models.rs`.

### 9.11 Templates
**No backend route exists** — `resolve()`/`TemplateError` are pure lopi-core (`template.rs`), used internally by `Task::from_template()`; confirmed zero "template" hits in any web route file. **Coverage: yes**, 6 inline tests (named-hole resolution, missing-var error naming, no-holes passthrough, unused-extra-var tolerance, escaped-brace literals, purity) plus 2 `task.rs` integration tests.

---

## 10. Top cross-cutting gaps, ranked for E2E-framework prioritization

1. **`/ws` + `/ws/tasks`** — zero backend test coverage, and per the web client's own code comments this socket is the real live-data path behind Overview/agent-list state. Fix the backend gap before building an E2E test that assumes this path is reliable.
2. **Stack-chain pause/resume/drain** — the frontend has a fully-built, heavily-unit-tested UX for this (`stackRun.ts`/`StackRun.swift`'s pause/resume/drain state machine) but **the backend has zero support for pausing an in-flight scheduled chain run**. Any E2E test that exercises "pause a running stack" will only ever be testing client-local state unless this backend gap is closed first — flag this to whoever scopes the sprint, not just the test-writing effort.
3. **`bumpCard`** ("bump sooner/later") — the single best example in this whole audit of logic that's fully implemented and extensively unit-tested on both platforms but has **zero E2E coverage and, on macOS, no discoverable UI trigger at all** in the files read. Worth a direct follow-up to confirm whether macOS's bump UI exists somewhere unread, or is a genuine parity gap.
4. **Run/pause/resume/drain, dry-run, duplicate/delete (card+stack), drag-to-reorder (card+stack), MAXX, evals suite-apply, guardrails gate/until, goal-pursuit toggle, config-drawer edits, all 3 autocomplete flows (`:alias`/`@repo`/`/command`) at both scopes, templates (apply/save both scopes), and the live-output panel** — all have **zero Playwright/XCUITest coverage today** despite most having strong-to-exhaustive unit coverage of their pure logic on both platforms. This is the bulk of the actual sprint work: proving wiring, not logic.
5. **`approve_plan`/`reject_plan`** (plan-approval workflow) — zero backend coverage, including an untested 409-conflict path. No UI reference to this flow was found in either client's read files in this audit — worth confirming whether a plan-approval UI exists anywhere before writing tests for it.
6. **Evals, goal-editing, and templates have no backend HTTP surface at all** — confirm this is by design (client-local / creation-time-only) before scoping any E2E test that assumes a server round-trip for these three areas.
7. **`templates.ts`/`StackTemplateStore` persistence** (localStorage on web, presumably `UserDefaults`-backed on macOS) has no unit test on either platform — only the pure conversion functions each store calls are tested. A reasonable unit-test gap to close before or alongside E2E work, since it's cheaper to fix than to E2E-cover.
8. **`stores/keyboard.ts` (all 5 shortcuts) and `HelpOverlay.svelte`** have zero coverage at any level on web; no equivalent keyboard-shortcut surface was found on macOS at all.
9. **`window.prompt()` dialogs** (both "save as template" flows) need explicit Playwright `page.on('dialog')` handling in the test-plan design — flag this as a framework-setup concern, not just a coverage gap.
10. **Dead code to exclude from test planning**: web — `Composer.svelte`, `LaunchControls.svelte`, `stores/layout.ts`; macOS — `AgentPaneView.swift`, `TranscriptView.swift` (both orphaned, unreferenced outside their own files, unreachable from any live nav path).
