# Ledger

A running log of load-bearing design decisions — the ones that would be
expensive to silently re-litigate in a later sprint. One entry per sprint,
newest first. Not a changelog (that's `CHANGELOG.md`) — this is *why*, not
*what*.

## Onboarding-Import-1 — `toolchain`, not `stack` (KT-C); KT-A/KT-B left genuinely open

**KT-C — the naming decision, confirmed and logged, not asked interactively.**
The mission brief itself already did the naming analysis and proposed
`toolchain` (table/column, not `toolchain_id`-as-separate-table) with a
concrete collision rationale: `web/src/lib/stores/stack.ts` and the whole
loop-stack/card concept already own the word `stack` in this codebase — a
grep against `stack.ts` before writing the migration confirmed the concept is
load-bearing there (`StackCard`, `buildCard`, `applyStackTemplate`, dozens of
call sites), not a stray usage that could tolerate a second meaning. Given the
brief itself had already reasoned through and proposed the one defensible
name, and given this is a one-way schema/naming decision worth surfacing but
not worth blocking an otherwise self-contained sprint on, the call made here
was: proceed with `toolchain` as a plain nullable `patterns.toolchain` column
(the simpler of the brief's two sanctioned shapes — a full `toolchains` table
would add a join with no present payoff, since Phase 2 only ever derives one
label per project directory), document the rationale here, and surface it
plainly in the session summary so a human can redirect before this actually
ships to production data. Logged as a one-way door regardless of which way it
had gone, per the brief's own instruction.

**KT-A — partially answered from real data, but not the corpus the kill-test
asked for.** This session's sandbox is a single-session ephemeral container,
not Wes's machine: `~/.claude/projects/` here contains exactly one file, this
very session's own in-progress transcript (`2afe0e65-....jsonl`), not 3+ files
across separate projects (lopi/squish/kiban). That one file was real enough to
settle the core structural question with certainty rather than a guess: a
`type: "user"` transcript line is not always a genuine human turn. Diffing two
real entries from the same file — `message.content` as a plain JSON string
(session-transcript line 2, no `toolUseResult` key) versus `message.content`
as a JSON array containing a `{"type":"tool_result",...}` block plus a
top-level `toolUseResult` key (line 13) — pins the distinguishing signal as
content *shape*, not the envelope's own `type` field, mirroring exactly what
`claude_events.rs` had to handle for the live-stream format. What this single
file cannot answer: whether every historical session across a real multi-
project corpus follows this same shape with no exceptions, and whether any
transcript ever carries a `type: "summary"` entry (raised as a possible richer
goal source in the brief) — none appeared in this one file, so
`transcript_import.rs` does not special-case it. Left open for a session with
real `~/.claude` access on Wes's machine; do not treat the one-file finding as
a full corpus validation.

**KT-B — could not be run at all, stated plainly rather than assumed.**
`~/.claude/settings.json` does not exist anywhere in this container (only
`launcher-settings.json`, a different file with a different purpose — SDK
hook wiring, not user retention prefs). There is no `cleanupPeriodDays` to
read here, so onboarding's real-world recovery window (30-day default vs.
whatever a given user has configured) is genuinely unknown from inside this
sandbox. Not assumed to be the 30-day default; not assumed to be anything.
Needs a session with real `~/.claude` access.

**Backfill success-rate semantics deliberately diverge from `mine_patterns`'s
live-run stats, not by oversight.** A live-mined pattern's `success_rate` is a
real test-pass-rate average across `attempts` rows; a historical transcript
has no `attempts` rows at all. `backfill_onboarding_pattern` uses a binary
proxy instead — `1.0` when Phase 4's completion heuristic passed, `0.0`
(no signal either way) otherwise — rather than inventing a fractional
pass-rate the data can't actually support. The shared `upsert_pattern_row`
blend-on-collision path (`f64::midpoint`) then treats that binary proxy the
same as a real average when folding it into an existing live-mined row, which
is an accepted approximation for this sprint, not a hidden precision loss —
worth revisiting if a future sprint finds backfilled evidence measurably
skewing blended success rates.

**How to apply:** any future migration touching the toolchain/language
dimension (the continual-recognition follow-on this sprint explicitly sets up
for) must keep the `toolchain` name — this is the point a one-way door was
meant to close. Any future kill-test gated on real `~/.claude` access should
assume a fresh Claude Code on the web / remote-environment session starts
with zero pre-existing transcript history, by design — that is not a bug to
work around, it is the reason this sprint's onboarding-import mission exists
in the first place.

## macOS-Web-Parity-4 — Config and Cron get the page header every other screen already has

**The candidate `NEXT_SESSION_PROMPT.md` flagged as lower-priority turned out to be worth doing anyway, for a reason specific to macOS, not just "match web."** `0cdd3a0` (2026-07-22) was filed as "mostly cosmetic/design-system alignment" when Parity-3 wrote its handoff — web giving Config/Schedules a page header instead of leading straight into a panel. Checking macOS's `ConfigView`/`CronView` before deciding whether to port it found the identical gap already existed natively: both screens go straight from the window chrome into their first panel/list, with zero page-level title, while `BudgetView`/`OverviewView`/`DashboardView` all open with a `Text(title).sans(22, semibold)` + mono-uppercase-subtitle header. That's not "macOS doesn't match web" — it's "macOS doesn't match *itself*," an inconsistency this sprint would have found even without web's own alignment sprint as a prompt. Confirmed by grep (zero `.navigationTitle` calls anywhere in the app — `RootView` draws its own black top bar with no reserved system-toolbar band, per its own doc comment, so every screen is entirely responsible for its own in-content header; there's no native window-title fallback quietly covering for Config/Cron).

**Two items from the same web commit deliberately left out, both platform-structural, not oversights.** The "Onboard" page has no macOS nav equivalent at all (`NavSection` has never had a case for it) — first-run setup on macOS goes through its own native config/server-settings surface, the same kind of one-way platform asymmetry `Dashboard` already represents in the other direction (macOS-exclusive, no web route). And `a2ce843`'s `:focus-visible` CSS ring recolor has no macOS analogue to port to — that's a web-specific hand-rolled focus-ring override; macOS gets its focus ring from AppKit for free, per-control, with no equivalent "make my accessibility ring match my border color" seam to touch.

**No new tests — a real, considered call, not an omission.** Both headers are static text (one title, one subtitle interpolating an already-computed `model.schedules.count`) with no branching, no new computed property, no data dependency beyond what the view already reads. Every other page header in this codebase (`BudgetView.header`, `OverviewView.header`) carries the same no-dedicated-test precedent for the same reason: there's no logic here to test independently of "does the text I wrote match the text I meant to write," which a build-and-look-at-it verification catches, not a unit test.

**Live-verify owed — same standing constraint as every macOS round.** Written on the Linux host that authors every macOS change in this repo; never compiled.

## macOS-Web-Parity-3 — Budget catches up to web's cost-breakdown sprint

**Found by diffing web's git history against macOS state, same method as Parity-2.** With Overview closed, the next question was "what did web ship since macOS's last Budget port that macOS never picked up?" `feat(budget): add budget store, API handlers, and web UI` (2026-07-22) turned out to be a real, server-backed addition — a brand-new `web/src/lib/stores/budget.ts` (77 lines, wholly new file, not a refactor of something that existed before) plus a genuinely new backend surface (`crates/lopi-memory/src/store/budget.rs`'s `cost_by_model_today`/`daily_cost_trend`, `GET /api/budget/breakdown`). macOS's `BudgetView` — built in an earlier sprint whose commit message just says "budget history" — has the live burn-rate/cap/top-spenders machinery this new commit's client-side `fleetBudget` store re-homes, but nothing from the two genuinely new, server-backed panels.

**Scope call: port the two backend-driven panels + the two free stat cards, skip `byRepo`.** Web's redesign added four things: a by-model breakdown, a 7-day trend, an alert-threshold slider, and a by-repo breakdown. The first three are either server-backed (by-model, trend) or a cheap, self-contained client addition (alert threshold — same persistence pattern as the existing hourly-cap setter). `byRepo` is different in kind: it groups cost by `AgentState.repo`, a field web's live wire events carry that macOS's `LiveAgent` doesn't have at all. `Store/Overview.swift` already documents this exact gap for its own goal/repo column (hardcoded `"—"` — "repo is unwired end-to-end, the same real gap web's own Overview has"). Building `byRepo` on macOS would mean threading a new field through the live event model first — a separate, larger change than "port a breakdown panel," so it's cited and deferred, not silently dropped.

**Where the pure trend logic lives, and why: same precedent as `Store/Overview.swift`/`Store/StackOverview.swift`, not `LopiStacksKit`.** `weekdayAbbrev`/`trendBars`/`trendDelta` (new `Store/BudgetTrend.swift`) compute UI-ready values (bar heights, "today" labels, a delta arrow direction) from live-ish server data, the same shape as the two prior Overview ports — app-target logic that's pure enough to unit-test but isn't the reusable cross-platform domain layer `LopiStacksKit` exists for. `trendDelta`'s `nil`-pct-when-prior-average-is-zero branch is a direct, deliberate port of web's own `budget.ts` logic (`if (priorAvg === 0) return today > 0 ? { pct: null, up: true } : null;`) — not a simplification, since "new spend" genuinely can't be expressed as a percentage of zero.

**No color-scheme reconciliation attempted, on purpose.** Web's Phase 10 Budget redesign switched several elements (the burn-vs-cap meter fill, several stat cards) from state-reactive coloring (jade/flame/rose based on burn fraction) to fixed literal colors (`#00ffd4` always, regardless of state) — verified by reading the shipped Svelte, not assumed. macOS's existing burn meter and matching stat cards were deliberately left as they are (state-reactive), rather than recolored to match web's fixed scheme. Reasoning: this sprint's job is closing genuine *feature* gaps (a missing breakdown panel is a capability macOS didn't have at all); recoloring an already-shipped, already-working, arguably-more-informative meter to match what may be an unintentional loss of state signaling on web's side is a separate design question, and folding it into a feature-parity sprint would blur "we ported a missing capability" with "we made a stylistic judgment call about which platform's designer was right." If a future session determines web's fixed-color choice was deliberate (not a stray regression), reconciling macOS to match is a clean, separately-scoped follow-up.

**Live-verify owed — same standing constraint as every macOS round since the first one.** Written on the Linux host that authors every macOS change in this repo; never compiled. `xcodegen generate && xcodebuild -scheme Lopi build`, then `xcodebuild -scheme Lopi test` (acceptance bar: `BudgetBreakdownTests`'s decode + pure-function coverage, alongside the existing suite) are the next session's first move.

## macOS-Web-Parity-2 — Overview becomes the kanban board; a real blocked-status bug surfaces along the way

**Why this sprint, now.** `docs/ops/PARITY_AUDIT_2026-07-16.md` closed macOS's Overview gap (`ef2bd20`, 2026-07-17 — a native rollup table shipped as scoped follow-up work from `macOS-Parity-Cut-1`). Four days later web redesigned the *same* route entirely (`2dee147`, 2026-07-21): a flat per-agent table became a 4-column lifecycle kanban board, because "users think in stacks, not individual loop runs." macOS's port was current when it shipped and stale within the week — the kind of divergence that accumulates silently unless someone actually diffs the two platforms' git history for the surface in question, which is how this sprint found it (not from a stale audit doc, which still described the *pre-port* macOS state).

**The color/status data flows through the pure logic, not the View — same architectural split as `Store/Overview.swift`, not `LopiStacksKit`.** `stackOverview.ts`'s `loopDotColor`/`metaFor` bake actual color decisions into the projection (a running loop's dot uses the *stack's own* accent color, not a fixed one) — this is genuine board-shaping logic, not View styling, so it had to live somewhere pure-ish. The choice was `macos/Lopi/Store/StackOverview.swift` (app target, imports SwiftUI + `LopiStacksKit`) rather than adding it to the portable `LopiStacksKit` package alongside `StackTypes.swift`/`StackRun.swift`. Reasoning: `Store/Overview.swift` (the existing Swift port of `stores/overview.ts`) already established this exact precedent — it imports SwiftUI and returns `Color` directly, because it's a *board projection* over live agent state, not the reusable domain layer Verify-4 proved portable for iOS. `stackOverview.ts` is architecturally identical to `overview.ts` in web's own module graph (both build on `stack.ts`'s domain types plus the live agent map), so its Swift port belongs beside `Overview.swift`, not inside the package. Kept colors as literal `Konjo.*` constants rather than round-tripping through hex strings the way web's `LIFECYCLE_COLOR` does — `Konjo.ice`/`.violet`/`.jade`/`.rose` are the exact same hex values (`0x00D4FF`/`0x7C3AED`/`0x00FF9D`/`0xFF0066`) web's board uses for the same four lifecycle meanings, confirmed by direct comparison rather than assumed.

**A real bug fell out of writing the port, not a hypothetical one.** Building `classify`/`loopDotColor` required a `.blocked` `CardStatus` case and a `blockReason` field — web added both in its "round 2, item 3" sprint, but the Swift port of `stack.ts` never picked them up. Tracing why led straight to `StackRun.swift`'s `launchNextCard` (and its bare-pane sibling in `StackRunControls.swift`): both call `seams.updateCard(...) { $0.status = .done }` unconditionally immediately after `waitForTerminal` resolves, *before* the very next line (`applyCardOutcome`) even branches on whether the terminal status was `.completed` vs. `.failed`/`.cancelled`. Every failed card in a macOS Loop Stack chain has been silently mislabeled `done` — not an Overview-only cosmetic gap, a run-state-correctness bug that would have stayed invisible until someone looked at a real failed chain's card list. Fixed to branch exactly like web's `advance`: completed → `.done`, otherwise → `.blocked` + a `blockReason` (the generic `"<goal>" ended <terminal>` fallback string — web's richer `blockReasonFor` also prefers a live verifier-gap/task-status detail when available, but `StackRunSeams.waitForTerminal` only returns a bare `TerminalStatus` enum with no richer payload; extending that seam to carry verifier detail is a larger, separate change than this sprint's Overview-board scope, so the fallback-only string is what shipped, honestly short of web's fuller message).

**Clone paths needed the same fix web already has.** `duplicateCard` (`StackOps.swift`), `duplicateStack` + `loadStackCardsInto` (`StackPaneOps.swift`) all reset `status = .idle` on clone but, before this sprint, left a stale `blockReason` behind — a cloned card from a previously-failed original would silently carry the old failure message into a fresh, never-run copy. Now cleared alongside the status reset, matching web's `duplicateCard`/`cloneStack` exactly.

**Dead code deleted rather than left the way web left its own equivalent.** Web's `stores/overview.ts` still exports `overviewRows`/`OverviewFilter`/`filterRows`/`filterCounts`/`OverviewRow` with zero callers anywhere in the web app post-redesign — only `formatElapsed` survives as a live import. Rather than mirror that leftover verbatim, macOS's `Store/Overview.swift` had the now-dead `overviewRows`/`OverviewFilter`/`rowMatchesFilter`/`filterRows`/`filterCounts`/`overviewScoreColor`/`OverviewRow` removed outright (confirmed zero remaining callers by grep across the whole macOS + iOS source tree first — `LopiIOS`'s target also compiles `Lopi/Store` wholesale, so this was checked against both native targets, not just macOS). `formatElapsed` stays, for the identical reason web kept it. This is a deliberate deviation from "mirror the reference exactly" — the reference's own leftover isn't a design decision worth replicating, just an unrelated session's unfinished cleanup, and leaving obviously dead code in place conflicts with this repo's stated zero-dead-code posture even though no Swift-side CI gate enforces it today.

**Click-to-focus reuses the grid's existing "everything renders side-by-side" property instead of building navigation that doesn't exist.** Web's `focusStack.ts` exists *because* `/stacks` has no per-stack detail route — every pane already renders at once, so "open a stack" from the board can only mean "scroll to and flash the one that's already visible," never a real navigation. The macOS Forge grid has the identical property (`ForgeView`'s grid is `store.panes` rendered 1:1, no per-stack push destination), so the same non-navigation affordance was the right port target rather than inventing a modal/detail view web doesn't have either. New `AppModel.focusedStackKey` (set by the board, read by `ForgeView`) + a `.task`-scoped 1.4s fading ice ring on the matching pane, functionally mirroring `StackPane.svelte`'s `focusflash` keyframe (`box-shadow` ring, 0.9→0 opacity) with SwiftUI's nearest equivalent (`.stroke` + `.animation`) rather than a pixel-identical port of a CSS keyframe that has no direct SwiftUI analogue.

**Live-verify owed — same standing constraint as every macOS round since the very first one.** Written on the Linux host that authors every macOS change in this repo; never compiled. `xcodegen generate && xcodebuild -scheme Lopi build`, then `xcodebuild -scheme Lopi test` (acceptance bar: `StackOverviewTests`'s ported assertions, `StackRunTests.testFailingCardHalts`'s new blocked-status assertion, and the existing suite staying green with the two new `CardStatus`/`StackCard` fields in play) are the next session's first move, per the standing "build on the M3" discipline this repo has never once skipped.

## iOS-Web-Parity-Plan-1 Phase 0 — composer grammar unification (`/` → `;`)

**Ported web's Composer-Grammar-1 rename into `LopiStacksKit`, closing the divergence `NEXT_SESSION_PROMPT.md`'s Composer-Grammar-1 entry carried forward.** That sprint scoped every touched file to web and explicitly left "port the identical `/` → `;` rename to the Swift side" as a follow-up, naming `stack.test.ts`'s kill-test-1 table (`;model/sonnet`, `;effort/high`, `;branch/main`, `;autonomy/L2`, `;eval/kcqf`) as the literal acceptance bar. This sprint is that follow-up — Phase 0 of `docs/ops/IOS_WEB_PARITY_PLAN_2026-07-23.md`'s plan, chosen to run first because the plan doc flagged it as fixing both native platforms in one shared-package change, before either platform's missing surfaces get built against a grammar already scheduled to change.

**One change point, not two.** `packages/LopiStacksKit/Sources/LopiStacksKit/StackOps.swift`'s `commandAutocomplete`/`detectPendingCommand`/`commandValueAutocomplete` are the only place the trigger character is decided — `macos/Lopi/Views/Forge/StackControlDockView.swift`'s command-bar suggestions and `macos/LopiIOS/Views/StackCommandBar.swift`'s stack dock both read their suggestion tokens from these same three functions, so macOS and iOS pick up the new `;` prefix from a single edit. What isn't shared: each platform's own text-field completion logic (finding the trigger character's position in the typed string to splice in a chosen suggestion) and iOS's literal `GrammarChip` hint labels — those live in per-platform SwiftUI views (`StackCardView.swift` on macOS; `StackCommandBar.swift`/`StackDetailScreen.swift` on iOS) and needed their own mechanical `/` → `;` edits, confirmed by grep to be the complete set (no macOS view renders a literal grammar-hint string the way iOS's `GrammarChip` does — macOS's facet summaries use SF Symbol icon rows instead, unaffected by this rename).

**`/loop/N` killed outright on the Swift side too, mirroring web's own decision — not renamed to `;loop/N`.** `xN`/`×N` was already the sole loop-count grammar on both native platforms (same as web before its own rename); the STACK_COMMANDS `loop` command was a second, redundant path to the identical `pane.config.loopCount` field. Removed from `STACK_COMMANDS` and every downstream switch that handled it — `StackControlDockView.swift`'s `commandOptionsFor`/`applyCommandValue` (macOS) and `StackCommandBar.swift`'s `valueOptions`/`applyCommand` plus the now-unused `loopCountOptions` catalog (iOS) — rather than leaving unreachable `case "loop"` branches behind.

**Test acceptance bar adapted to what the Swift layer actually exposes, not force-fit to web's literal table.** Web's kill-test-1 table lives in `stack.test.ts`'s `tokenizeGoalChips` tests — a chip-*rendering* tokenizer with no Swift equivalent (each native platform renders chips its own way; it was never extracted into the shared package). The Swift port (`StackStoreTests.testComposerGrammarRenameAcceptance`) instead exercises the same five literal tokens through `detectPendingCommand`, which only depends on the command *name* matching the regex, not a catalog's contents — the safe apples-to-apples check. A literal round-trip assertion through `commandValueAutocomplete` against the real `MODEL_OPTIONS`/`AUTONOMY_OPTIONS` catalogs was checked by hand first and rejected for the model case: `;model/sonnet` legitimately resolves to a *different* token (`;model/claude-sonnet-5`) than the query text, since `optionMatches` filters on `label` (`"Sonnet 5"`) not `value` — asserting literal equality there would have been testing a coincidence (it happens to hold for `;effort/high` and `;autonomy/L2`, where the value and a label substring coincide) rather than a real invariant.

**Written, not built — the same standing constraint every Swift round in this repo has carried since `IOS_RESEARCH_1_SPIKE.md`.** This host has no Xcode. `xcodegen generate && xcodebuild -scheme Lopi build` (macOS) and `-scheme LopiIOS build` (iOS), plus `cd packages/LopiStacksKit && swift test` (the acceptance bar: `testInlineCommandAutocomplete`/`testDetectPendingCommand`/`testComposerGrammarRenameAcceptance` passing, alongside the existing 60+ ported assertions untouched by this change), are the real bar and remain owed to a session with real hardware.

## MCPB-App-2 — the stack-status widget's first write path: click-to-cancel

**Phase 0 pre-flight kill-tests, run before any widget code, per this sprint's own gate.**

- **KT-1 (tool-call symmetry) — confirmed symmetric.** Read `crates/lopi-mcp/src/server.rs`'s `handle_request`/`handle_call` end to end: `tools/call` is routed to `handler.call(name, arguments)` with no inspection of *where* the JSON-RPC line came from — there is no session/origin field on `Request` at all, model-initiated and widget-initiated calls are structurally indistinguishable to this server. A widget's `callServerTool({name:"lopi_cancel_task",...})` and the model calling the same tool go through the identical `handle_call` path. Nothing to build here; this was a read, not a fix.
- **KT-2 (response-delivery mechanism) — confirmed distinct from `ontoolresult`.** Extracted the vendored SDK's actual `callServerTool` (`sed -n '241p' stack_status.html | grep -o ...`, since the bundle is one 300KB+ line): `async callServerTool(r,i){...return await this.request({method:"tools/call",params:r},hn,{onprogress:...,...i})}` — a plain awaited JSON-RPC request/response, resolved directly to the caller. `ontoolresult` is a separate assigned handler (`app.ontoolresult = fn`) that fires on `ui/notifications/tool-result` — a *notification*, not a response — and per this widget's own existing comment, only ever for `lopi_get_stack_status` re-invocations (the tool this widget is bound to). Wired the cancel result through the `callServerTool()` promise's resolved value (`doCancel()`'s `await app.callServerTool(...)`), never through `ontoolresult`. **This is exactly the distinction the sprint brief warned a future click-action sprint would get wrong if unwritten — write it down again here:** `ontoolresult` is for "my own bound tool got re-invoked and pushed new data at me"; `callServerTool()`'s return value is for "I just asked the server to do something and I'm waiting for that specific answer." A future widget action should always use the latter for its own request's result.
- **KT-3 (host-level approval UX) — still unknown, correctly left unknown.** No real MCP Apps host is reachable from this sandbox (same boundary `KT-B3-Live` already established). The widget's own code does not assume a host-level modal, a one-time per-session grant, or anything else — it only implements its *own* confirm step (see below), independent of whatever the host does or doesn't add on top. Whatever a real host does here will layer on top of, not replace, this widget's own confirmation.
- **KT-4 (autonomy/plan-approval gate on cancel) — confirmed none applies.** Read `AgentPool::cancel` (`crates/lopi-orchestrator/src/pool/mod.rs:128`) and `MemoryStore::delete_task` (`crates/lopi-memory/src/store/mod.rs:251`) directly rather than trusting `cancel_task`'s existing test coverage to imply it: `cancel` only checks for a live `cancel_tx` handle and unconditionally signals it; `delete_task` unconditionally cascades the delete across `attempts`/`turn_metrics`/`agent_checkpoints`/`task_logs`/`verifier_verdicts`/`eval_outcomes` plus the `tasks` row itself. Neither consults `Task::require_plan_approval`, `successor_enabled`, or any `TaskSource`-based check — those gates (`Sprint Successor-1`'s entry above) govern task *creation* from untrusted origins, not cancellation of an existing task. Nothing to build here either; this was a read confirming a negative.

**Decision — confirm-before-destructive-action tries `window.confirm()` first, falls back to a two-click affordance on a caught exception, and this split is untested against a real host (ties to KT-3).** `lopi_cancel_task` deletes the row outright — no undo. Widget iframes served under MCP Apps are commonly sandboxed without `allow-modals`, which makes `window.confirm()` *throw* rather than quietly return `false` — so the code branches on catching that exception (`confirmed = null`) versus getting an explicit `true`/`false` back, rather than trying to detect the sandbox some other way. This could not be verified against a real host in this session (Phase 3 is gated on KT-B3, per the brief), so both paths are implemented rather than picking one and hoping — a future live-verification session should confirm which path actually fires and can delete the other once observed.

**Decision — `.row` changed from `<button>` to a `role="button"` div; this was forced, not a style choice.** Adding the Cancel action as a real nested `<button class="cancel-btn">` inside the existing `<button class="row">` is invalid HTML: per the HTML parsing algorithm, a `<button>` start tag encountered while already inside an open `<button>` auto-closes the outer one (the same "not on the implied-end-tag list, but the parser still fixes it" behavior as nested `<p>`/`<a>`), which would have silently truncated every row's markup the instant this shipped, not thrown a build error. Fixed by making `.row` a `div` with `role="button" tabindex="0"`, and adding a `root.onkeydown` (Enter/Space) alongside the existing `root.onclick`, since a div doesn't get keyboard activation for free the way a real button does. `toggleDetail()` was factored out of `render()`'s inline `onclick` body so both handlers share it instead of duplicating the expand/collapse logic.

**Decision — the `crates/lopi-mcp/src/server/tests.rs` location the brief named for Phase 2 doesn't have access to `lopi_cancel_task` at all, so the test lives in `src/mcp_commands/server_wire_tests.rs` instead.** `crates/lopi-mcp` is deliberately a pure protocol engine (its own module doc: tested "over in-memory pipes with a mock handler" — the real handler is "wired in at the binary layer"); `lopi_cancel_task`'s actual dispatch logic and the private `LopiToolHandler` struct that implements `ToolHandler` for it both live in the root binary crate's `src/mcp_commands/mod.rs`, which `crates/lopi-mcp` has no dependency on and could not reach even with a different file path. The brief's intent — drive a real `tools/call` for `lopi_cancel_task` through the actual JSON-RPC server loop, not just `dispatch()` in-process — is still met: the new tests wrap the real (not mock) `LopiToolHandler` around a fresh `test_state()` `AppState` and drive it through `lopi_mcp::serve()` (the same `pub fn serve` the mcp-serve binary itself calls) over an in-memory `tokio::io::duplex` pipe with a real `McpClient`, exactly mirroring `crates/lopi-mcp/src/server/tests.rs`'s own `client_drives_served_handler_end_to_end` pattern. `mod_tests.rs`'s `test_state()` helper was changed to `pub(super)` (one keyword) so both test modules share it rather than duplicating a 10-line helper.

**Verified, not assumed:** `cargo build --workspace` and `cargo test --workspace` both green (1576 tests, including the 2 new `server_wire_tests`), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt` applied. Widget: extracted the `<script type="module">` body and ran `node --check` — clean; confirmed exactly one `<script>`/`</script>` pair and zero stray literal `</script` substrings, same checks `Stack-Status-Kanban-1` ran.

**Phase 3 (live verification) was not attempted.** Per the sprint's own explicit gate, KT-B3 (the widget render handshake) has not been confirmed as of the most recent `KT-B3-Live` entries below — this session did not fabricate a pass. See `NEXT_SESSION_PROMPT.md` for exactly what a session with real Claude Desktop access needs to check once KT-B3 clears: the confirm-dialog-vs-two-click question (KT-3), the mid-flight task-completes-before-click race, and the rapid-double-click disable check.

**How to apply.** Any future click-driven widget action: (1) fire it through `callServerTool()`'s own resolved promise, never `ontoolresult` — the latter is for a *different* re-invocation of the widget's own bound tool, not this request's answer (KT-2, restated because this is the second time it's been written down); (2) never nest a real `<button>` inside `.row` or any other clickable `<button>` wrapper — check what the outer clickable element actually is before assuming a nested interactive element is safe HTML; (3) a destructive action needs its own confirm step regardless of what a host might add — do not wait for KT-3 to resolve before shipping the widget's own guard; (4) if a future tool's write path needs regression coverage at the real JSON-RPC surface and that tool's handler lives in the binary crate (as every lopi-specific tool does), the test belongs beside that handler (`src/mcp_commands/`), driven via `lopi_mcp::serve()` — not inside `crates/lopi-mcp` itself, which has no access to lopi's actual tool implementations by design.

## Stack-Status-Kanban-1 — `stack_status.html`'s `render()`, table → 5-column kanban

**The brief described `bucketOf()`/`orbColor()`/`isPulsing()` as already-existing helpers in this file ("keep that function, it's already correct") — they were not there.** Read the file in full before writing anything: `src/mcp_ui/stack_status.html`'s `render()` was still the original plain `<table>` from `MCPB-App-1` (`git log` confirms only two commits ever touched this file: `ad3a95b` created it, `ddcd2b7` rebuilt it onto the real MCP Apps SDK — neither added a kanban board or those three functions). The "1a"/"1b" design directions the brief named turned out to be real, just living somewhere else: `feat(web)`'s `replace /overview with a kanban-style Loop Stacks board` commit, in `web/src/lib/stores/stackOverview.ts` and `web/src/lib/components/stacks/StackOverviewCard.svelte`. Confirmed this rather than guessing from the brief's description alone, since building the wrong color/spacing values from a paraphrase would have been a second design pass to unwind later.

**Decision — this is a translation, not a shared-code refactor, and the two implementations are allowed to drift.** The web board and this widget solve the same design problem (kanban board, same "1a"/"1b" visual language) over two structurally different data models: the web board projects a client-only `panes` store keyed by lifecycle (`queued`/`running`/`testing`/`done`, 4 columns, `testing` *is* a column there) against a live in-memory `agents` map with `elapsedMs`/`cost`; this widget renders a server-pushed `lopi_get_stack_status` JSON payload (`{id, goal, status, branch, stage, created_at, completed_at}`) with no live-agent join at all. The brief's own 5-bucket spec (`Queued`/`Running`/`Conflict`/`Dead-letter`/`Done`, `Testing` explicitly *not* a column) confirms these were meant to diverge, not converge — so `bucketOf`/`orbColor`/`isPulsing` here are fresh, self-contained functions written against this widget's actual payload shape, not a port of `stackOverview.ts`'s functions of the same intent. A future session unifying the two into one shared TS/JS module would be a real, separate refactor — not something this session should quietly attempt as a side effect of "translate the design."

**Decision — `orbColor(status, stage)`'s `test`-stage override is keyed on the literal DAG node kind `"test"`, not a `TaskStatus` variant.** `crates/lopi-memory/src/store/dag.rs`'s `current_stage()` returns one of `RECORDED_PIPELINE = ["plan", "implement", "test", "score"]` (or `"queued"`) — a DAG-node kind, never a `TaskStatus::db_status()` string. The brief's "a conflicted task mid-test-stage still lives in the Conflict column, just with whatever stage color applies" only makes sense once `stage` and `status` are recognized as two independent fields from two independent tables (`tasks.status` vs. `agent_dag_nodes.kind`) that the tool joins — conflating them (e.g. trying to derive the testing accent from `status` alone) would have been silently wrong for exactly the scenario the brief called out.

**Verified, not assumed:** `node --check` on the extracted `<script type="module">` body, a single `<script>`/`</script>` pair, zero literal `</script` inside the vendored SDK bundle line, `cargo build --workspace` green, and the 8 existing `mcp_commands::stack_status` tests still green (they assert tool/resource wiring and the `get_stack_status` JSON join — none of them assert on `WIDGET_HTML`'s contents, so a render-only change was never at risk of breaking them, and passing them is not evidence the new render is correct). **Actual rendering in a live MCP Apps host is still unverified** — same sandbox boundary `KT-B3-Live` and `MCPB-App-1` already documented; nothing in this session changes that.

**How to apply:** before implementing a brief that says "keep/reuse the existing X function," grep for X first — a brief describing prior design work can be accurate about *where a design came from* while wrong about *whether it already landed in the file you're about to edit*; this session's brief was both at once. Any future change to this widget's bucket/color/stage logic should stay grounded in the real payload shape (`src/mcp_commands/stack_status.rs::get_stack_status`) and the real DAG-stage vocabulary (`RECORDED_PIPELINE` in `dag.rs`), not in the web app's `stackOverview.ts` — read that file for design inspiration only, never as a source of truth for this widget's actual data.

## Sprint Successor-1 — Task Lineage and Containment (`crates/lopi-core/src/{successor.rs,task.rs,task_source.rs}`)

**One-way-door decisions.** Once real tasks start persisting with these three shapes, changing any of them means a migration across every already-derived successor task, not just a code edit — recorded here per the sprint brief's own instruction.

**Decision 1 — `TaskSource::SelfAuthored { parent: TaskId }` is a new variant, not a reuse of `SelfModify`.** `SelfModify` already existed for "approved self-modification task targeting lopi's own codebase" and carries `approved_by: String` — a human/mechanism identity. Conflating the two would have meant either overloading `approved_by` to sometimes hold a `TaskId` as a string (untyped, lossy, and exactly the kind of stringly-typed drift this codebase's `ReportChannel`/`AutonomyLevel` parse-with-named-errors precedent exists to avoid), or adding an `Option<TaskId>` field to `SelfModify` that's meaningless for its original case. `SelfAuthored` answers a different question than `SelfModify` — *who created this task* (the agent that ran `parent`, vs. a human/webhook/API caller) vs. *what this task targets* (lopi's own codebase) — and a task could in principle be both someday (a successor that happens to target lopi's own repo). Once tasks are persisted with `source` values across a `TaskSource` enum, adding a new variant is additive (old code's exhaustive `match`es break loudly at compile time, which is the point — no `Webhook`/`Telegram` catch-all silently swallowed the new case, as `pool/run_loop.rs::task_source_label` and `is_untrusted_source` both had to be updated by hand); *removing or renaming* a variant already in a durable `tasks.source` JSON column is the expensive direction, so the naming (`SelfAuthored`, not e.g. `Derived` or `AgentSpawned`) was chosen to still read correctly next to `SelfModify` if a future sprint's variant list grows.

**Decision 2 — the autonomy ceiling is `min(parent, requested)` by rank, computed fresh at derivation time, not inherited-then-optionally-overridden.** `clamp_autonomy_to_parent(parent_level, requested_level) -> AutonomyLevel::from_rank(parent_level.rank().min(requested_level.rank()))` means a successor's trust level is *recomputed* from its parent every time, never copy-forward-then-trust. This matters once chains run more than one hop deep: rank is `1..=4` and strictly ordered (`ReportOnly < DraftPr < VerifiedPr < AutoMerge`), so a chain can only ever monotonically narrow or hold steady, never regain trust a shallower ancestor gave up. The one-way-door part: this sprint's only caller (`AgentRunner::derive_and_stash_successor`) always passes a freshly-defaulted child's own `autonomy_level` (`AutonomyLevel::default()`, i.e. `DraftPr`/L2) as `requested_level`, since neither `Successor` (the Phase 1 struct) nor this sprint's fixture-only enqueue path lets anything ask for a specific level yet. A future sprint that lets an agent's own output request a level (Sprint Successor-2's parsing work) *must* route that request through this same clamp, never around it — the gate is the ceiling, not the ceiling's caller.

**Decision 3 — the untrusted-source gate is a one-way ratchet: `require_plan_approval = true` and `successor_enabled = false` are forced, never merely defaulted, and there is no override.** A `Webhook`- or `Telegram`-sourced parent (an external system or an inbound message, as opposed to a human at the CLI/API or an already-`SelfModify`-approved task) produces a child that (a) cannot proceed to implementation without a human approving its plan, full stop, regardless of what autonomy level gate 2 computed, and (b) cannot itself spawn a further successor — the chain dead-ends at exactly one hop from untrusted input. This was chosen over a softer "narrow autonomy to `ReportOnly`" response because autonomy and plan-approval are already-established *orthogonal* axes in this codebase (`Task::require_plan_approval`'s own doc comment: "a genuinely different axis from... `autonomy_level`") — narrowing only the autonomy axis would still let an `L1`/report-only successor run unattended to completion and write a report, which is not "a human looks at this before anything happens." Once a chain has been allowed to self-extend past webhook/Telegram input under a *weaker* version of this gate, retrofitting the stronger one is a behavior change for every already-running or already-persisted successor task from that origin — hence recording it now, before any of this sprint's plumbing is live.

**How to apply:** any future variant added to `TaskSource` must be checked against both `is_untrusted_source` (does this origin need the lockdown?) and every exhaustive `match` the compiler flags (there is no `_ =>` wildcard on this enum in `lopi-orchestrator`'s `task_source_label`, deliberately). Any future path that lets an agent (not a human/config/test-fixture) supply an `AutonomyLevel` for a derived task must call `clamp_autonomy_to_parent`, not assign the requested level directly. Any future relaxation of gate 4 (e.g., letting an operator explicitly re-enable `successor_enabled` on a webhook-derived child) should be an explicit, named opt-in on the *child* task, not a change to `derive_successor_task`'s default behavior — the gate's value is that it is unconditional today.

## KT-B3-Live (cont'd) — third first-real-run bug: widget resource advertised the wrong MIME type, never spec-conformant

**With the two packaging bugs from the entry below fixed and the server actually spawning, the widget still never rendered — Claude Desktop showed the resource's raw HTML in a warning toast instead of an inline dashboard, `"Unsupported UI resource content format"`.** Verified the failure was real (a screenshot from the user's own Claude Desktop, not a tool-result annotation — a `structuredContent`/resource-read success annotation only confirms the tool declared UI capability, not that the host actually rendered it) before diagnosing anything. Checked the two most likely culprits first and ruled them both out: `server.rs:120`'s `resources/read` response already wraps contents as the spec-correct `json!({ "contents": [contents] })`, and the resource genuinely was discovered and fetched (the HTML reached the client intact) — this was not a repeat of Findings 1–2's spawn failure.

**Root cause: `src/mcp_commands/stack_status.rs:47` and `:57` advertised `mime_type: "text/html"`, but MCP Apps (SEP-1865, the January 2026 extension co-authored by Anthropic and OpenAI) requires `text/html;profile=mcp-app`.** Confirmed against the authoritative spec, not assumed from the bug report alone: Claude Desktop's own `initialize` capability negotiation advertises `"extensions":{"io.modelcontextprotocol/ui":{"mimeTypes":["text/html;profile=mcp-app"]}}`, and the `@modelcontextprotocol/ext-apps` package's `RESOURCE_MIME_TYPE` constant is defined as that exact string — bare `text/html` was never a valid value for this extension, even though it reads as the obvious choice for an HTML payload. Fixed in both spots (`ui_resources()`'s advertised `mime_type` and `ui_resource_contents()`'s served `mime_type`), plus the two matching test assertions in `stack_status_tests.rs:142`/`:149` that had encoded the same wrong expectation. `crates/lopi-mcp/src/server/tests.rs`'s bare-`"text/html"` mock fixtures were left alone — they test the generic `resources/list`/`resources/read` wrapping mechanism, not this widget's actual content type, so changing them would prove nothing about this bug.

**How to apply:** any future `McpResourceContents`/`McpResource` for a `ui://` MCP Apps widget must use `text/html;profile=mcp-app`, never bare `text/html` — the profile suffix is what makes a host's UI-capable extension actually claim the resource, and its absence fails silently as a content-format rejection rather than a wiring/spawn error, so it's easy to mistake for the KT-B3 render-handshake question itself (it isn't; the handshake question is still open). More broadly: this is the **third** consecutive bug in this same first-real-run track (`${platform}` templating, `timeout` on macOS runners, now this) where code that was internally consistent, passed every existing test, and looked correct on paper was still wrong the moment it met a real host. None of the three would have been caught by more unit tests of the existing kind — each needed the actual external contract (a real manifest loader, a real macOS runner, a real MCP Apps host) in the loop. Treat "builds and unit-tests green" as necessary, not sufficient, for anything that talks to a real host/runner/client outside this repo's own control — schedule a real-device/real-host check before, not after, calling a packaging or protocol-surface change done.

## KT-B3-Live — first real attended install attempt: server failed to spawn, two independent packaging bugs found and fixed

**KT-B3 (the widget render handshake) still has not been observed — but this is the first time the attended runbook actually ran, and it surfaced a real failure before ever reaching the render question.** Repo-gap fixed first: `LOPI_KTB3_ATTENDED_RUNBOOK.md` was referenced by `CHANGELOG.md`, `LEDGER.md`, and `NEXT_SESSION_PROMPT.md` but never committed (same drift class as `LOPI_DISTRIBUTION_PLAN.md`) — committed as-is, nothing in it was stale.

**Finding 1 — `mcpb/manifest.json` used a substitution token that doesn't exist.** Installing `lopi-bfe4d7bb...-darwin-arm64.mcpb` (the real `MCPB-App-1` artifact, correct SHA, green build) into a real Claude Desktop produced this in its MCP log:

```
Using MCP server command: .../server/${platform}/lopi
Failed to spawn process: No such file or directory
```

`${platform}` never got substituted — `${__dirname}` in the same string resolved fine. Checked against the authoritative spec ([`modelcontextprotocol/mcpb` `MANIFEST.md`](https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md#variable-substitution)): the only supported tokens are `${__dirname}`, `${HOME}`, `${DESKTOP}`, `${DOCUMENTS}`, `${DOWNLOADS}`, `${pathSeparator}`/`${/}`, and `${user_config.*}`. Platform variance is meant to go through a sibling `platform_overrides` key, not a template token in the path itself — `${platform}` was never real. Since `compatibility.platforms` is already `["darwin"]`-only, no override mechanism was even needed: fixed by hardcoding the literal path the release workflow actually bundles, `server/darwin-arm64/lopi`, in both `entry_point` and `mcp_config.command`. This means **every previously-built `.mcpb` artifact, including the one this sprint verified with `mcpb pack`/`unpack` mechanics, was never actually installable** — the packaging-mechanics check exercised `unpack` + direct binary invocation, never the manifest's own command-resolution path a real host uses.

**Finding 2 — independent of Finding 1: this branch's copy of `mcpb-release.yml` had regressed to `timeout 10`, which doesn't exist on macOS runners.** The branch's `origin/main` merge predated `bfe4d7bb` ("Fix timeout handling in mcpb-release workflow") landing on main, so re-triggering the workflow after Finding 1's fix hit `timeout: command not found` in the smoke-test step (run `29770546202`) — nothing to do with the manifest fix, pure branch/main drift on a file that had already been fixed once. Re-applied `perl -e 'alarm 10; exec @ARGV'` directly rather than merging main wholesale.

**Both fixes verified together in one real run, not assumed:** `29770853385` (headSha `467abb8`) went green end to end, including the smoke-test's real `initialize` → `serverInfo` round trip. Fresh artifact: `lopi-467abb86e6e3408e73fefc7367db9e72d428587c-darwin-arm64.mcpb`.

**What's still open — the actual KT-B3 question.** None of this touched the widget-render check itself; the runbook's steps 2-5 (tool list, task submission, panel-renders-or-doesn't) have not run against a build that can even spawn yet. The `.mcpb` dropped in the repo root from the failed attempt (`lopi-bfe4d7bb...`) is stale — the new artifact from `29770853385` needs to replace it before the next attended attempt.

**How to apply:** any future MCPB manifest change should be smoke-tested through the manifest's own `mcp_config.command` resolution (i.e., actually installed and spawned by a real host), not just `mcpb pack`/`unpack` + direct binary invocation — the latter is necessary but was not sufficient here and gave false confidence. Also: a stale-workflow-file-on-a-branch check (`git merge-base --is-ancestor <known-fix-commit> HEAD`) before trusting a CI file on a long-lived feature branch would have caught Finding 2 before spending a run on it.

## Browser-Pane-1 — Live Dashboard via Claude Code Desktop's Browser Pane (no new code; `CLAUDE.md`)

**Finding: the Browser pane does NOT auto-detect a `lopi sail` process it didn't start itself, but Claude navigates to it autonomously anyway — even without any written instruction telling it to.** Verified against a real, already-running instance (`--repo /Users/wscholl/kohaku`, port `3000` per `lopi.toml`'s default, running for hours before this session touched it): `preview_list` returned `[]` for it. The pane's "auto-detect a dev server" behavior is scoped to processes *it* launches via `preview_start({name})`/`.claude/launch.json` (the standard `npm run dev` pattern) — a Rust binary spawned independently outside that flow is invisible to it until pointed at explicitly. Calling `preview_start({url: "http://localhost:3000"})` showed the real dashboard immediately: real stack cards, real running/queued task counts, zero console errors.

**Why this matters more than expected:** the sprint's actual bar (can Claude navigate there autonomously from a natural, mechanism-blind prompt like "what's lopi running right now, show me the stacks") passed twice, independently — **before any `CLAUDE.md` note existed to explain the mechanism**. Once directly in this session, and once via a freshly spawned `general-purpose` subagent with no hint that a Browser pane was the intended path: it worked its way there through `ps`/`lsof`/`curl` against the real REST routes, then reasoned on its own that `preview_start` was the tool to actually display it. A capable session is already good enough at this unprompted; the "does this need a written rule to be discoverable" premise going in undersold what ordinary tool exploration already gets you.

**Caveat, stated plainly: this sprint could NOT genuinely validate whether the `CLAUDE.md` addition (its new "Live Dashboard (Browser Pane)" section) is itself what makes this work in a cold session.** The `Agent`-tool subagent used to test this inherited a `CLAUDE.md` context snapshot from this conversation's start — **before** the note was added — confirmed directly: asked to recap its reasoning, it reported never having seen that section, and that it arrived at the same procedure independently. Its success is evidence the underlying capability doesn't strictly need the note, not evidence the note causes anything. A true cold-start test requires quitting and relaunching the actual Claude Code Desktop process (a fresh top-level session, not a subagent spawned mid-conversation) — not something this session can do to itself. Carried forward in `docs/ops/NEXT_SESSION_PROMPT.md`.

**How to apply:** for any future "does the Browser pane see X automatically" question, verify with `preview_list` first — never assume yes for a process not launched via `preview_start`/`launch.json`. For "does Claude need an explicit written rule to use a tool it already has," don't assume yes before testing with a blind, naturally-phrased prompt — this sprint found a fully capable agent already gets there via ordinary tool exploration, without the rule. The MCPB widget track remains a separate, non-obsoleted concern (it targets claude.ai/Cowork reach, which this Desktop-only mechanism structurally cannot provide) — but for the narrower ask of "let Claude Code itself check on live lopi state," this path already works today with zero new Rust/MCP code, and should be the default answer over building a new widget for that specific use case.

## MCPB-App-1

**KT-B1 — branch-persistence shape: a new `tasks.branch` column, written by
a dedicated `set_task_branch` store call fired from `TaskStarted`.** Read
`crates/lopi-core/src/event.rs`'s `AgentEvent::TaskStarted` and
`crates/lopi-agent/src/runner/run_loop.rs:186-197` (where the event fires)
before deciding, per the brief's own instruction not to assume the plan
doc's phrasing. Found a clean synchronous path already in place: `AgentRunner`
(`crates/lopi-agent/src/runner/mod.rs:60`) carries `pub store: Option<MemoryStore>`,
and `lifecycle.rs`'s existing `record_dag_transition` (called from every
`self.status()`) already establishes the exact shape needed — clone the
store, `tokio::spawn` a fire-and-forget write, `tracing::warn!` on error,
never block the run loop. `persist_branch` (`lifecycle.rs`) copies that
shape exactly and is called immediately after `TaskStarted` fires in
`run_loop.rs`, alongside the existing `self.bus.send(AgentEvent::TaskStarted
{ .. })`. **Chosen over a dedicated non-`tasks`-table store call** (the
brief's other option) because `client_ref`'s prior `ALTER TABLE tasks ADD
COLUMN client_ref TEXT;` (`crates/lopi-memory/src/schema.sql:71`, Backend-1)
is the exact precedent: a plain nullable column, applied via the same
idempotent `ALTER TABLE` migration guard `apply_schema()` already tolerates
duplicate-column errors on. A dedicated table would need its own join for
every roster read `lopi_get_stack_status` does; a column doesn't. `TaskRow`,
`get_task`, and `load_history` all now carry/select `branch`. The store
method itself lives in a new `crates/lopi-memory/src/store/branch.rs` (not
inline in `store/mod.rs`) purely because `store/mod.rs` was already at 493
lines against the repo's 500-line hard gate before this sprint touched it —
same file-splitting precedent `dag.rs`/`task_logs.rs`/etc. already set.

**KT-B2 — `lopi_get_stack_status`'s join verified against a real two-task,
two-stage fixture, real field values asserted.** Per the brief's own
mutation-testing-precedent bar (`MCP-Serve-1`'s G3 gate), not just "the
query runs." `src/mcp_commands/stack_status_tests.rs` seeds one task with a
`DagNodeRow` in `running` state at `plan` (a `Planning`-shaped attempt) and
a concurrent second task with `plan`/`implement` `done` and `test` `running`
(a `Testing`-shaped attempt), each on its own `set_task_branch`-set branch.
`get_stack_status_joins_roster_branch_and_stage_for_concurrent_tasks`
asserts each task's `branch`, `stage`, `status`, and `goal` independently —
confirms the join doesn't cross-contaminate between concurrently-running
tasks, not just that both rows exist. `current_stage` (new pure fn,
`crates/lopi-memory/src/store/dag.rs`) derives the roster's `stage` field:
the currently-`running` node's kind, else the most advanced `done` node
(ranked by a small fixed `RECORDED_PIPELINE` array — `plan`/`implement`/
`test`/`score`, deliberately excluding `verify`/`diff`/`pr` from
`lopi_agent::dag::NodeKind::PIPELINE` since `record_dag_transition`'s match
arms never actually write those three), else `"queued"` when no DAG node
exists yet. Neither existing tool was rebound — per `MCP-App-1`'s KT-D3
finding below, `lopi_get_agent_dag` is one-task-scoped with no branch, and
`tasks.status` alone can't carry stage granularity.

**A new MCP protocol surface, not scoped by the original plan doc's
`_meta.ui.resourceUri`-only framing: `resources/list`/`resources/read` plus
`structuredContent`.** `_meta.ui.resourceUri` on a tool only tells a host
*which* `ui://` URI to fetch — the host still needs a standard MCP way to
actually fetch it. `crates/lopi-mcp` had zero resource scaffolding before
this sprint (confirmed: `grep -rn "ui://|resources/read"` across the whole
repo returned nothing). Added: `McpResource`/`McpResourceContents` types
(`protocol.rs`), `ToolHandler::resources()`/`read_resource()` with
default-empty/default-error bodies (RPITIT default methods — Rust
1.94/stable supports this; the trait is used generically, `H: ToolHandler`,
never as `dyn`, so this doesn't hit RPITIT's dyn-compatibility gap), new
`resources/list`/`resources/read` dispatch arms in `server.rs`, and
`initialize`'s capabilities now advertise `resources: {}` alongside
`tools: {}`. Also added: `tools/call`'s response now includes
`structuredContent` whenever the tool's text output parses as JSON (every
lopi tool's output does) — this is what an MCP Apps host is specified to
hand into a bound widget's `ui/initialize` response; without it there'd be
a `ui://` resource and a binding but no actual data path into the iframe.
Both are backward-compatible additions (existing `content`-only consumers
unaffected) verified by `crates/lopi-mcp/src/server/tests.rs`'s new cases,
and by directly driving the packed-then-unpacked binary's real stdio
protocol (see the packaging finding below) — `resources/list`,
`resources/read`, and `tools/call` for `lopi_get_stack_status` all round-
tripped correctly, including a byte-exact widget HTML fetch.

**The widget (`src/mcp_ui/stack_status.html`) implements exactly the three
lifecycle methods the brief specified — `ui/initialize`,
`ui/notifications/initialized`, `ui/notifications/tool-result` — and
nothing beyond that.** Plain HTML/JS, no framework, `include_str!`'d into
the binary (not a loose file the `.mcpb` needs to carry separately — the
plan's bundle-layout diagram showing `server/ui/*.html` as a bundle member
turned out to be one workable option, not the only one; embedding avoids a
second thing that has to stay in sync with the binary). Deliberately
**not** implemented: any widget-initiated `tools/call` for interval
polling — the plan's "the widget polls on an interval" freshness note
describes the *store's* checkpoint-fresh write behavior, not a specified
widget-side polling API, and SEP-1865 doesn't define one lopi could target
with confidence from a doc read alone. Building an unspecified polling
mechanism now would be exactly the "simulate the happy path" failure mode
KT-B3 exists to catch — deferred to whatever the real handshake in KT-B3
actually looks like. User-controlled text (`goal`) is HTML-escaped before
insertion (`escapeHtml`) — the roster renders free-text task goals, and a
prior task's goal is attacker-adjacent input the same way any other stored
user content is (see `.claude/rules/security.md`).

**A new, concretely-checked kill-test the original brief didn't anticipate:
this sandbox cannot produce a real macOS arm64 binary at all, cross-
compilation or otherwise — checked two ways, not assumed.** The brief's
Deliverable 4 assumed "local or cloud both work... nothing here needs
nested-spawn access or a GUI host," reasonably extrapolating from KT-B1/B2
being sandbox-safe. That assumption doesn't extend to producing the actual
target binary:

1. Plain `cargo build --target aarch64-apple-darwin`: fails immediately —
   this sandbox's `cc` is Linux GCC/Clang, which rejects `ring`'s
   macOS-targeted build flags (`-arch arm64`, `-mmacosx-version-min=11.0`,
   `-gfull`) outright.
2. `cargo-zigbuild` (the standard cross-compilation workaround, installed
   live via `pip install ziglang` + `cargo install cargo-zigbuild`): gets
   substantially further — `zig cc` accepts the Apple-targeted flags `ring`
   needs, and even `openssl-sys` cross-builds cleanly once `git2`'s
   `vendored-openssl`/`vendored-libgit2` features are enabled. It still
   hits a hard wall on `libgit2-sys`'s own `build.rs`
   (`~/.cargo/registry/.../libgit2-sys-*/build.rs:166-213`), which
   **unconditionally** selects `GIT_SECURE_TRANSPORT` + `GIT_SHA256_COMMON_
   CRYPTO` and links `framework=Security`/`framework=CoreFoundation` for
   any `target.contains("apple")` — there is no feature flag or env var in
   the upstream crate to force OpenSSL on a Darwin target instead. Apple's
   Security/CoreFoundation frameworks are proprietary and not present in
   zig's bundled SDK subset (nor legitimately obtainable in this sandbox).
   The `git2/vendored-openssl,vendored-libgit2` feature experiment used to
   reach this finding was reverted afterward (`crates/lopi-git/Cargo.toml`,
   confirmed clean via `git diff`/`git status`) — it doesn't fully solve
   the problem anyway, and enabling "vendor and build OpenSSL from source
   on every build" isn't a decision to make silently as a side effect of a
   kill-test.

**This is a structural toolchain gap, not a code defect** — disabling
`git2`'s `https` feature would dodge it by silently removing HTTPS git
support from the shipped binary, which is exactly the "quietly redefine
success downward" failure mode the brief warned against; not done. Real-
world Rust projects hit this identical wall and solve it by building
natively on a macOS runner rather than cross-compiling from Linux, which is
what `.github/workflows/mcpb-release.yml` (new, `workflow_dispatch`-only,
not yet run for real) now does.

**What was verified instead, for real, since the actual target binary
couldn't be:** `mcpb validate` against `mcpb/manifest.json` (this caught
two real schema errors the plan doc's own example JSON had — `repository`
must be an object not a string, and every `user_config` entry needs a
`description` — fixed, then passed clean). `mcpb pack`/`unpack` round-
tripped the real manifest + directory layout using the host's own
(x86_64 Linux) `lopi mcp-serve` binary as a packaging-mechanics stand-in —
**not a substitute for the real macOS arm64 build**, but it did confirm the
manifest schema, `entry_point` path convention, and bundle layout are all
correct, and that the unpacked binary — invoked exactly as `mcp_config`
specifies (`command` + `args: ["mcp-serve"]`) — correctly answers
`initialize`, `tools/list` (all eight tools, `lopi_get_stack_status`
carrying the right `_meta.ui.resourceUri`), `resources/list`,
`resources/read` (byte-exact widget HTML), and `tools/call` for
`lopi_get_stack_status` (`structuredContent: {"tasks":[]}` against an empty
fixture). Every piece of this sprint's own code is now real-protocol
verified; only "does this literal binary exist for arm64 macOS" remains
open, and that's a toolchain question, not a lopi-code question.

**KT-B3 (the widget render handshake) was not attempted — out of scope for
this sprint by its own brief, not a gap.** See
`LOPI_KTB3_ATTENDED_RUNBOOK.md` for the attended checklist; nothing in this
sprint tries to simulate or approximate that check.

**`LOPI_DISTRIBUTION_PLAN.md`'s repo copy is still stale — flagged again,
not fixed, per the brief's own instruction not to silently trust either
copy.** Confirmed live: the repo's Track B section (`## TRACK B — MCPB
Desktop Extension`, no "+ Inline Dashboard" suffix) is still the
pre-Track-D-merge draft — no Deliverables 1–2 (branch persistence, the
aggregating tool), no KT-B1/KT-B2/KT-B3, no widget mention at all. This
sprint worked from the session prompt's pasted `LOPI_DISTRIBUTION_PLAN.md`
(the merged version), exactly as `NEXT_SESSION_PROMPT.md`'s prior entry
warned would be necessary. Third time this exact drift has been logged
(`MCP-App-1`'s entry below, and that entry's own note about the two
`NEXT_SESSION_PROMPT.md` files) — still not this sprint's job to fix, but
now clearly overdue for a sync pass.

## MCP-App-1

**KT-D2 attempted and confirmed blocked in this environment — the sprint's
hard gate did its job.** The brief ordered KT-D2 first specifically because
everything downstream (Deliverables 2–4, Phase D1–D4) is wasted effort if it
fails, and named the exact honest-stop condition: "If this sandboxed
environment has no real Claude Desktop install or real claude.ai account to
test against: stop here. Do not simulate, do not assume the spec's happy
path, do not mark this passed." Checked concretely rather than assumed:

- `uname -a` / `$DISPLAY` / `/Applications` confirm a headless Linux
  container (`Linux vm 6.18.5`, no `DISPLAY` set, no `/Applications`) — Claude
  Desktop is a macOS/Windows GUI app with no possible rendering surface here,
  structural, not a permissions issue to work around.
- No saved claude.ai browser profile/cookies/credentials exist anywhere on
  disk (checked `~/.config`, `~/Library` — neither present or populated with
  auth state). Chromium/Playwright is installed but there is no real
  authenticated claude.ai account to log a widget render into, and obtaining
  one isn't this session's to do.
- The only `claude` binary present (`/opt/node22/bin/claude`) is this very
  session's own harness process (`ps aux` shows it running
  `--output-format=stream-json` as the driver of this conversation), not a
  separate interactive session available for nested testing — the same
  classifier-blocked shape MCP-Serve-1's KT2 and Composer-Grammar-2's kill
  test hit (see that entry below), not a new failure mode.

**Consequence, per the brief's own instructions, followed exactly:** no
widget code, no `ui://` resource, no new tool implementation this sprint.
KT-D1 (Claude Code's text fallback staying clean with a resource attached)
depends on both a built resource *and* live interactive Claude Code
verification — blocked for the identical root cause, not attempted.
Deliverables 2–4 (the resource, the tool binding wired to real
`structuredContent`, the status view) are Phase D1–D3 work, explicitly
gated behind KT-D2 by the brief's own "Phased build (only past this point
if KT-D2 cleared)" section — not started, correctly.

**KT-D3 (tool-binding decision) does not depend on live hosts, so it was
answered — the brief calls this out as a real decision "logged either way."**
Read the actual source chain before deciding, not the plan doc's assumption:

- `lopi_get_agent_dag` (`src/mcp_commands.rs:311-328`) reads
  `state.store.load_dag_nodes(&id)` → `lopi_memory::dag_graph_json`
  (`crates/lopi-memory/src/store/dag.rs:36-56`). This is scoped to **one**
  task's pipeline-stage nodes (`plan`/`implement`/`test`/`score`/…) and
  carries no branch field at all.
- `lopi_list_tasks`/`lopi_get_task` read `TaskRow` (`crates/lopi-memory/src/
  store/mod.rs:433-448`), sourced from the `tasks` table's `status` column.
  That column is coarser than it looks: `save_task` writes `"queued"` at
  submission, `mark_running` (`store/mod.rs:192-198`) flips it to the
  **literal string `"running"` exactly once**, and nothing updates it again
  until a terminal `mark_completed` call. Every `Planning → Implementing →
  Testing → Scoring` transition happens *without* touching this column — so
  `tasks.status` cannot answer "what stage is this task in right now," only
  "queued / running / done."
- Stage-level `TaskStatus` detail only ever lands durably in
  `agent_dag_nodes`, via `record_dag_transition`
  (`crates/lopi-agent/src/runner/lifecycle.rs:52-58`), called from
  `self.status()` on every transition — the same call that also broadcasts
  the in-memory (pool-local, not cross-process) `AgentEvent::StatusChanged`.

**Decision: the widget needs a new aggregating tool** (not yet built —
gated behind KT-D2), not a rebind of `lopi_get_agent_dag` as-is. It would
need to join a task roster (`load_history`-shaped, like `lopi_list_tasks`)
with a per-task `load_dag_nodes` read for stage-level status, since neither
existing tool alone covers "which tasks are running" (a roster) plus
"current `TaskStatus`" (stage granularity `tasks.status` doesn't carry) in
one call. This is *more* specific than the plan doc's "one task's DAG vs.
a new tool" framing assumed — it's not just about multi-pane aggregation,
`tasks.status`'s coarseness is an independent reason `lopi_get_agent_dag`
alone can't be the whole answer either, since the DAG alone doesn't give a
roster and `list_tasks` alone doesn't give live stage detail.

**A second, unplanned finding: "branch" (Deliverable 4's second required
field) has no clean structured source anywhere in the store.** Branch names
are deterministic (`format!("lopi/{}-attempt-{}", task_id, attempt+1)`,
`crates/lopi-agent/src/runner/run_loop.rs:186`) but only ever materialize
as: an in-memory `AgentEvent::TaskStarted { branch, .. }` (pool-local, not
shared cross-process — confirmed dead-end per MCP-Serve-1's KT4, the same
constraint that ruled out reading pool state for anything else); a freeform
`"● branch: {branch}"` line inside `task_logs` (durable, reachable via
`lopi_get_logs`, but string-embedded, not a field — parsing it is fragile,
not a real API contract); or `TaskStatus::Success{branch}` (only present
once a task has already finished, useless for "which branch is this
*running* task on"). None of these is a queryable structured column today.
**This means the new aggregating tool from KT-D3 isn't just new
aggregation logic — it needs a small store-side prerequisite first**
(persisting branch as a real column, or a dedicated store call, when
`TaskStarted` fires) that neither the plan doc nor the original KT-D3
framing anticipated. Carried forward to `NEXT_SESSION_PROMPT.md` rather
than built speculatively this sprint, since building it without KT-D2
resolved would be shipping widget-adjacent surface area with no proof the
render path it's for will ever complete a handshake.

**Freshness (the other half of the narrowed KT-D3): store-backed DAG reads
are checkpoint-fresh, not continuously live.** `record_dag_transition`
writes synchronously on every stage transition, so a store poll reflects
the true current stage within moments of it changing — accurate at each
`Planning`/`Implementing`/`Testing`/`Scoring` boundary — but there is no
push/stream from the store between transitions. A widget built on this
needs to poll on an interval (a few seconds is plausible given transitions
happen on the order of tens of seconds to minutes per stage, per the run
loop's own pacing), not assume any continuous live feed.

**Also flagged, not fixed this sprint:** the repo's `LOPI_DISTRIBUTION_PLAN.md`
is stale — it's the pre-`MCP-Serve-1` draft (no "Track A shipped" update, no
Track D section at all). The session prompt that kicked this sprint off
pasted the up-to-date version (with Track D, and Track A marked shipped)
as an attachment rather than relying on the repo's own copy — which is how
this sprint could be scoped at all despite the repo file's drift. This is
the same class of "small, real inconsistency… not this sprint's job to fix"
already called out for the two `NEXT_SESSION_PROMPT.md` files; worth a sync
pass before another session gets tripped up trusting the repo's copy over
a pasted one.

## MCP-Serve-1

**Plugin `name` slug: `lopi` — one-way door.** `plugin/.claude-plugin/plugin.json`'s
`name` field is `"lopi"`. Once anything installs against this slug from any
marketplace (self-hosted or `anthropics/claude-plugins-community`), it is pinned —
changing it later is a new plugin, not a rename. Chosen over `lopi-orchestrator`
or a `konjo-` prefix because it's the name every other surface (crate, binary,
CLI verb, repo) already uses; a mismatched plugin slug would be the one thing
that *doesn't* match. Matches the marketplace entry name (`lopi@lopi-marketplace`)
and the MCP server key (`"lopi"` in `.mcp.json`'s `mcpServers`) — all three are
independently renameable later without breaking installs, `name` in `plugin.json`
is the only one that can't be.

**Plugin content lives in `plugin/`, not the repo root — a real constraint
discovered live, not a style choice.** `claude plugin validate --strict` on a
`plugin.json` at repo root fails: it flags the repo's own `CLAUDE.md` sitting at
"plugin root" as invalid plugin context (`CLAUDE.md at the plugin root is not
loaded as project context`). This repo's `CLAUDE.md` is real, load-bearing
content for human/agent contributors — not something to delete or move to
satisfy a plugin validator. `.claude-plugin/marketplace.json` stays at the repo
root (Claude Code's marketplace discovery is a fixed path — `/plugin marketplace
add konjoai/lopi` only looks there) but its one plugin entry's `source` points at
`./plugin`, a subdirectory with no `CLAUDE.md` sibling. Verified live: installing
via this layout resolves `${CLAUDE_PLUGIN_ROOT}` to the `plugin/` subtree's cache
copy, not the repo root — `plugin/bin/lopi`, `plugin/.mcp.json`, and
`plugin/skills/lopi-cli/SKILL.md` all land where `.mcp.json`'s
`${CLAUDE_PLUGIN_ROOT}/bin/lopi` expects them.

**KT4 — `lopi mcp-serve`'s `ToolHandler` state-sharing design.** Decision: build
a standalone, in-process `AgentPool` + `TaskQueue` + dispatch loop inside
`mcp-serve` itself (mirroring `sail_commands::run`'s wiring, minus the HTTP
listener/browser-open/Telegram/cron-quota-warmup — those are dashboard-only
convenience, out of scope for the curated tool set), reusing `lopi_ui::web::AppState`
as the literal state type rather than inventing a second one. The one piece
that's genuinely shared across any concurrently-running `lopi sail` process is
the `MemoryStore` — both open the same SQLite file at the same `db_path()` (or
`--config`'s `lopi.db_path`), so every read-only tool (`lopi_list_tasks`/
`lopi_get_task`/`lopi_get_logs`/`lopi_get_agent_dag`/`lopi_get_stats`) reflects
true durable history no matter which process a task was submitted through. Live
dispatch (the pool that actually runs `AgentRunner`, i.e. `claude -p`) is *not*
shared and structurally can't be — `TaskQueue`/`AgentPool` are pure in-memory
`Arc`/`DashMap`/`Mutex<BinaryHeap>` state, not backed by the DB, confirmed by
reading `crates/lopi-orchestrator/src/queue.rs` and `pool/mod.rs` before writing
a line of `mcp_commands.rs`. A task submitted via `lopi_submit_task` in one
`mcp-serve` invocation is executed only by that invocation's own pool.

**Why this and not an HTTP-client `ToolHandler` calling an already-running
`sail`'s REST API:** that alternative would make `lopi mcp-serve` depend on a
separately-started `lopi sail` as a hidden prerequisite — contradicting the
sprint's own goal ("something a stranger can install and watch run"), since a
freshly-installed plugin user has no `sail` running yet. The standalone-pool
design makes `submit_task → get_task` genuinely round-trip end-to-end inside one
`mcp-serve` process's lifetime, with no setup step beyond installing the plugin.
The cost — a task submitted via MCP isn't visible as "running" in a *different*,
already-running `sail` dashboard's live view, and `lopi_cancel_task`'s
`pool.cancel()` only succeeds against tasks that process itself dispatched — is
real but bounded: `get_task`/`list_tasks`/`get_stats`/`get_logs`/`get_agent_dag`
all still resolve correctly cross-process because they read `s.store`, not
`s.pool`'s live handles. Verified live against the actual packaged binary, not
just the dev build: `lopi_submit_task` in one `mcp-serve` process, `lopi_get_task`
in a fresh second process pointed at the same `--config` DB, correctly returns
`"status":"queued"` — the durable read succeeded; the second process's pool
never ran it, exactly as designed, not a bug.

**How to apply:** Track B (MCPB) reuses this exact same `ToolHandler` and the
same state-sharing design — a `.mcpb`-bundled binary invoked as `lopi mcp-serve`
is architecturally identical to the plugin's `.mcp.json` invocation, just a
different wrapper (per `LOPI_DISTRIBUTION_PLAN.md` §2.1: "No new tool logic").
Track C (remote connector) is a different animal — a Streamable HTTP transport
serving *multiple concurrent clients* against one long-lived process changes
the calculus entirely (that process's pool *would* need to be the one true
dispatcher, since there's no "the user's own separate `sail`" to defer to) —
don't assume this sprint's answer carries over uncritically; re-derive it when
Track C is actually scoped.

## Permission-Modes-1

**Four-mode subset (`bypassPermissions`/`auto`/`acceptEdits`/`dontAsk`),
`plan`/`manual` deliberately excluded — logged as a one-way door on the
selectable set, not a permanent ceiling.** `claude --permission-mode` accepts
six values on the installed CLI (`2.1.211`); only four are exposed as
`PermissionMode` variants / web dropdown entries.

**Why:** `plan` and `manual` both need every tool call to round-trip through
a live human decision, which headless `claude -p` has no channel for today.
`plan_gate.rs` proves lopi *can* build this kind of relay (it does exactly
this for one specific point — the first attempt's plan), but generalizing it
to every tool call is a distinct, larger feature, not a dropdown addition.
Live kill-test evidence for the four that *are* exposed:

- **KT1 (`auto`/`dontAsk` don't stall headless) — PASS.** Ran both live
  against a throwaway clone with a Bash write outside the read-only set
  (`mkdir` + file write, not pre-approved). `auto` self-approved the command
  as low-risk and completed in 10s; `dontAsk` cleanly denied it (no matching
  allow-list entry) and reported back in 14s. Neither stalled.
- **KT2 (`acceptEdits` + `permission_allow` avoids stalling) — PASS.** Ran a
  real `cargo test -p lopi-toon --lib` under `acceptEdits`. With
  `--allowedTools "Bash(cargo test:*)"` (what `LoopConfig::permission_allow`
  forwards as): completed in 8s, 33/33 passed, no prompt. Negative control —
  same command, `acceptEdits`, no allow entry — was denied cleanly in 16s
  ("requires your explicit approval... isn't going through"), confirming the
  allow-list is what prevents the stall, not the mode alone.
- **KT3 (`bypassPermissions` is a true drop-in for
  `--dangerously-skip-permissions`) — PASS**, on the installed CLI. Both
  flags produced the byte-identical root-refusal error string
  (`"--dangerously-skip-permissions cannot be used with root/sudo privileges
  for security reasons"`) — even the `--permission-mode bypassPermissions`
  path's error names the other flag, confirming a shared refusal code path.
  The non-root success path wasn't independently re-verified (no working
  non-root `claude` auth in the sandbox this sprint ran in); the shared
  refusal path is strong evidence of true equivalence regardless. Note: the
  repo pins no `claude` CLI version anywhere (the Dockerfile builds only the
  `lopi` binary, never installs `claude` at all) — there is no "pinned
  version" to diff a changelog against; `2.1.211` is simply what was
  installed in the sandbox that ran this kill-test.
- **KT4 (`auto` mode account eligibility) — NOT VERIFIED, open item.** The
  account this sprint's sandbox authenticated as is not the account lopi's
  production deployment authenticates as — this session had no visibility
  into that deployment's real credentials, so eligibility (model/provider/
  plan, Team/Enterprise Owner toggle) could not be confirmed for the account
  that will actually run this. Decision made anyway, per the spec's "pick
  one, don't leave it implicit": `auto` is **shown, not hidden or
  disabled** — an ineligible account fails at spawn time with a surfaced
  CLI error, the same failure-visibility default `select_model`/`with_effort`
  already use elsewhere in this codebase for a malformed value. Re-verify
  against the real deployment account before trusting this silently.
- **KT5 (container root check) — NOT VERIFIED, open item.** Static audit
  only: `Dockerfile:74` sets `USER lopi`; `fly.toml` carries no process-level
  user override. No `fly` CLI or attended access to the live deployed
  container was available this sprint to confirm at runtime, per the kickoff
  prompt's own anticipated gap. Do not treat the Dockerfile as proof; a
  compose override or fly.toml directive could still change the runtime user
  without touching it.

**Enum wire-value strings match the CLI's own literal flag values verbatim,
not a snake_case translation.** `PermissionMode` serializes to
`"bypassPermissions"`/`"auto"`/`"acceptEdits"`/`"dontAsk"` via per-variant
`#[serde(rename = ...)]`, and `PermissionMode::parse` matches those same
literals case-sensitively (no lowercasing, unlike `normalize_effort` — these
come from a controlled dropdown, not free-form text). Rejected: a
snake_case Rust-side representation with a translation table at the CLI
spawn site — that's exactly the indirection `--model`/`--effort` already
avoid by storing the CLI-ready string directly, and it's an extra place a
`bypass_permissions` ↔ `bypassPermissions` typo could silently drift.

**Default variant: `BypassPermissions`.** An absent `Task.permission_mode`
(and an absent `CreateTaskRequest.permission_mode`) must reproduce the
pre-existing unconditional `--dangerously-skip-permissions` behavior
exactly — this sprint is an opt-in loosening of autonomy, never a silent
behavior change for a task that doesn't touch the new field.

**`--permission-mode` folded into `apply_cli_caps`, reversing that
function's own prior doc comment.** The doc comment at
`claude_support.rs:93-100` explicitly said `--dangerously-skip-permissions`
was kept per-site because "their positions/doc comments differ enough not to
share." This sprint revisited that call and inverted it: permission mode is
now emitted unconditionally inside `apply_cli_caps`, the one shared
injection point already used for `--model`/`--effort`/`--max-turns`/
`--max-budget-usd`/`--allowedTools`/`--disallowedTools`.

**Why:** every other cap in `apply_cli_caps` is genuinely optional —
`None`/empty means "add nothing, let the CLI default stand." Permission mode
is categorically different: there is no "add nothing" state for it anymore.
Every one of the three spawn sites must emit *some* `--permission-mode`
value on every call, always (falling back to `PermissionMode::default()`
when the task hasn't set one). That "always emits, never optional" shape is
precisely the pattern a shared cap-injection point is for; keeping it
per-site after this sprint would mean three near-identical
`cmd.arg("--permission-mode").arg(...)` blocks instead of one, the exact
copy-paste risk `apply_cli_caps` was built to close for the other caps.

**How to apply:** any future flag that becomes "always emitted, resolved
from a typed default" rather than "optional, `None` = omit" should fold into
`apply_cli_caps` the same way, not stay per-site by default. A cap that's
still genuinely optional (can validly be entirely absent from the argv)
should stay following the existing `Option<T>` + per-site-comment pattern
until it, too, gains an unconditional fallback.

## Composer-Grammar-2

**Kill-test 1 (does `claude -p` expand a `/name` token embedded mid-prompt,
or only standalone?) was attempted, not assumed unanswerable, and is
genuinely blocked in this environment.** The sprint brief called this
"BLOCKING, live proof only — M3 + real auth." A `claude` CLI binary
(`/opt/node22/bin/claude`, authenticated) is actually present in this
session's environment — unlike prior sprints' Xcode/quota kill-tests, which
were blocked by a missing toolchain or missing hardware entirely. A fixture
repo with a real `.claude/commands/foo.md` was built and the kill-test's own
two-scenario protocol (bare `-p "/foo"` vs. embedded mid-prose) was attempted
verbatim — both invocations were refused by this session's own permission
classifier ("Blocked by classifier" — a nested/recursive `claude` CLI
invocation from within an active Claude Code session, distinct from every
other kill-test's missing-hardware blocker). This was proven by attempting
it, exactly as the pre-flight kill-test itself instructs, not skipped on
assumption.

**Why this matters for what shipped:** Phase 3 (the actual `claude -p`
pass-through) is explicitly gated on kill-test 1's result by the brief's own
phased-build section — "if kill-test 1 failed: add a pre-submission bypass
route... if it passed: no change needed." Building either branch on a guess
would mean shipping unverified core-loop behavior (`claude.rs`'s
`build_plan_prompt` wrapping) with a 50/50 chance of being backwards. Phase
1 (backend discovery) and Phase 2 (frontend autocomplete/chip wiring) do not
depend on kill-test 1's answer at all — a `/name` token reaching the goal
field is real, correct behavior regardless of how it later gets wrapped —
so those shipped. Phase 3 did not.

**How to apply:** the next session with an unblocked `claude` CLI (the
user's own machine, or wherever "M3 + real auth" resolves to for this repo)
should re-run the exact fixture-repo protocol this entry describes — it is
already built out, not something to re-derive — read the
`--output-format stream-json` system-init event's `slash_commands` field
and confirm the fixture command's actual body executes (not just literal
text echoed back) in both the bare and embedded-in-TOON-wrapped-prose cases.
Whichever branch fires, Phase 3's implementation is small (either "no
change" or one bypass function) — the live proof, not the code, was always
the hard part.

**The `/name` chip color (`chip-claude`, rose) breaks from the sprint
brief's suggested reuse — because the brief's premise didn't survive how
Composer-Grammar-1 actually landed.** The brief assumed "the generic violet
freed up by the `;` sprint's per-field split is the natural reuse, since
nothing else claims it anymore." That was true of the brief's own mental
model of Composer-Grammar-1, but not of what actually shipped:
Composer-Grammar-1's `chip-command` bucket was *renamed* to `chip-autonomy`
(same violet value, still actively used by `;autonomy` plus five
non-value-picker commands), not freed. Reusing it here would have made a
real Claude Code command visually indistinguishable from `;autonomy`/`;eval`/
`;guard`/`;schedule`/`;maxx`/`;goal` chips — the opposite of the stated goal
("own chip color" so it never reads as one of lopi's own verbs). `--konjo-rose`
(`#ff0066`) was picked from the app's existing named palette (`app.css`) —
the one color token no stack chip had claimed yet — rather than inventing a
new hex value from nothing.

**`lopi-skill` becomes a real production dependency of `lopi-ui`, where
`lopi-agent` deliberately stayed dev-only.** `lopi-ui/Cargo.toml` already
carries a comment on its `lopi-agent` dev-dependency: "Test-only... without
adding a real production dependency on lopi-agent." That boundary was
respected, not routed around: the new discovery module was built in
`lopi-skill` (already a dependency of `lopi-agent`, so no new crate enters
the build graph — just a direct edge for visibility) rather than beside
`claude.rs` in `lopi-agent` as the brief's "New module (lopi-agent or
lopi-core)" line suggested. `lopi-skill` carries none of `lopi-agent`'s
process-spawning/`reqwest` weight, so taking it as a real (not dev-only)
dependency doesn't reintroduce the coupling the earlier comment was written
to avoid. `lopi-core` was ruled out outright: `lopi-skill` depends on
`lopi-core`, so the reverse edge would be a cycle.

## Composer-Grammar-1 (web)

**`/` → `;` prefix swap for lopi's own composer verbs — logged as a one-way
door.** `CARD_COMMANDS`/`STACK_COMMANDS` (`model`/`effort`/`branch`/
`autonomy`/`eval`/`guard`/`schedule`/`maxx`) moved from the `/` prefix to a
new `;` catch-all prefix. `:alias`, `@repo`, and `×N`/`xN` keep their own
prefixes, untouched.

**Why:** `/` is what real Claude Code slash commands use. Lopi's own
composer grammar squatting on that character blocks ever wiring up real
Claude Code `/` commands in the same goal field without a collision — two
different command vocabularies can't safely share one trigger character in
the same autocomplete surface. `;` is free, unambiguous, and gives lopi's
verbs one consistent home instead of borrowing a character it doesn't own.

**Hard cutover, no backward-compat shim.** An old `/model/...`-style token
already sitting in a saved card/stack goal string (composer text, templates,
`localStorage`) stops parsing as a chip after this sprint — it renders as
plain text instead. This was a deliberate default, not an oversight: the
underlying text is unaffected (nothing is deleted or silently rewritten),
only the chip-rendering/autocomplete behavior stops recognizing it. Adding a
read-compat shim (accept both `/` and `;` as trigger prefixes) was considered
and rejected — it would have kept `/` semantically occupied by lopi's own
grammar exactly as long as any old saved text existed, defeating the entire
point of vacating `/` for the next sprint's real Claude Code hookup.

**`/loop/N` killed outright, not renamed to `;loop/N`.** `xN` was already the
sole primary loop-count grammar; `/loop/N` was a second, redundant path to
the identical `pane.config.loopCount` field. Rather than carry that
redundancy forward under the new prefix, it was deleted. The stack dock's
`×N` grammar-chip button (previously wired through the value-picker command
path) now inserts a literal `x3` token directly, the same way
`StackCard.svelte`'s own `chipLoop` always has.

**Chip colors reuse `ConfigDrawer.svelte`'s palette verbatim, not new
values.** `ChipInput.svelte`'s generic violet `chip-command` bucket split
into `chip-model` (cyan) and `chip-branch` (green) as distinct
`GoalSegment['chipKind']` variants, and was renamed (not recolored) to
`chip-autonomy` — the exact same violet RGB triple it already had, since that
color happened to already match `ConfigDrawer`'s real autonomy swatch. No new
colors were invented for `eval`/`guard`/`schedule`/`maxx`/`goal` — those stay
on the renamed `chip-autonomy` bucket as the generic fallback, since
`ConfigDrawer` has no per-field swatch for any of them to reuse.

**macOS (`StackCardView.swift`/`StackControlDockView.swift`) was not
touched.** The sprint brief scoped every file reference to web
(`stack.ts`/`ChipInput.svelte`/`ConfigDrawer.svelte`) and never mentioned
macOS; this session also has no Xcode toolchain to compile-verify a Swift
change against (a standing constraint noted in prior `NEXT_SESSION_PROMPT.md`
entries). macOS still parses the old `/`-prefixed grammar — a real
composer-grammar divergence between platforms, but not a functional
regression: each platform only ever parses its own locally-typed text into
the same wire fields (`card.config.model`/`.effort`/`.branch`/`.autonomy`),
so a card's *behavior* is identical either way, only its *composer shortcut
text* differs. Flagged as a concrete follow-up, not silently dropped.

**How to apply:** any future addition to lopi's own composer grammar
(another `;command`) is a pure catalog append to `CARD_COMMANDS`/
`STACK_COMMANDS` — the four matching functions and the tokenizer are already
generic over `InlineCommandDef[]`, proven by this sprint's own rename being
mechanical rather than requiring new parsing logic.

## Stack-Chain-1 / Popover-Fix-1 / Parity-Audit-1

**New tables, not an overload of `schedules`.** `schedule_chains` /
`schedule_chain_steps` / `schedule_chain_runs`
(`crates/lopi-memory/src/schema.sql`) are new, sibling to `schedules` rather
than an extension of it.

**Why:** confirmed by two pre-flight kill-tests before any schema was
written. KT1 read `crates/lopi-agent/src/dag.rs` in full: it's a fixed
7-node linear pipeline of *stages within one agent attempt*
(`Plan→Implement→Test→Score→Verify→Diff→PR`), not a sequence of independent
goals — reusing it would have force-fit a structure that doesn't model the
problem. KT4 confirmed `schedules`' `ScheduleSpec`/`ScheduleRow` have exactly
one `goal: String` field each, with no chain/step concept anywhere, and that
`AgentPool::submit()` is the only task-injection entrypoint — extending the
existing row shape in place would have meant either cramming a
serialized-list hack into `goal` or breaking every existing single-schedule
caller.

**How to apply:** any future "sequence of N independent things, each its own
full unit of work" primitive in this codebase should follow the same
shape — a header table + an ordered child table + a per-fire run-state
table — rather than trying to generalize an existing single-item table.

**Restart-resume is real, not best-effort-and-hope.** `ChainScheduleManager`
(`crates/lopi-orchestrator/src/chain_schedule_manager.rs`) scans
`schedule_chain_runs` still `running` on boot and either advances (task
actually finished before the restart, per its durable `tasks.status` row) or
resubmits the same step (orphaned).

**Why:** KT4's research established that `AgentPool`'s `TaskQueue` is purely
in-memory — nothing about a queued or running task survives a process
restart today, anywhere in this codebase. A chain scheduler that assumed
`TaskCompleted` events would eventually arrive post-restart would have
silently hung forever on exactly the incident scenario (backend offline
overnight) that motivated this sprint. This was proven, not assumed: a
genuine integration test (`crates/lopi-orchestrator/tests/chain_schedule_resume.rs`)
opens a real on-disk SQLite file, drops every in-process object, and reopens
a fresh set against the same file — the actual boundary a process restart
crosses.

**How to apply:** any future server-side scheduler that spans more than one
fire-and-forget task submission must assume zero in-memory state survives a
restart and re-derive "what was I doing" from the durable store on boot, the
same pattern `ChainScheduleManager::start()`/`resume_orphaned` establishes.

**Popover fix is a bug fix, not a `preferAbove` policy default.** The sprint
brief proposed adding a `preferAbove` prop to `Popover.svelte` and defaulting
it `true` at every stack-context call site. That was not implemented.

**Why:** KT2 reproduced the bug with hard numbers before writing any fix
code — `popEl.getBoundingClientRect()` before and after toggling "run on a
schedule" on. The popover correctly flipped above the anchor for the small
pre-toggle content (`computePosition()`'s existing flip logic already
worked); it only failed to reposition *after* the content grew, because
nothing re-triggered `computePosition()` on a content-size change — only on
`open` and `window resize`. A `preferAbove` default would have been treating
a stale-measurement bug as if it were a "never enough room below" design
question, and would not have actually fixed anything: the popover would
still fail to reposition on content growth, just from a different starting
side. The real fix (a `ResizeObserver` on the popover element) was
live-verified: pre-fix the popover overflowed the 700px window by 57.4px
after the toggle; post-fix the identical interaction repositions with
133.6px of clearance.

**How to apply:** any future "popover/dropdown clips off-screen" report
should be kill-tested with real before/after `getBoundingClientRect()`
numbers before reaching for a positioning-policy prop — the fix is usually
"the reposition trigger is missing," not "the default side is wrong."

**macOS needed no popover-positioning fix — confirmed live, not inferred.**
`request_access` for the `Lopi` app was denied earlier in the session; the
user re-granted it later in the same session, which let KT3 actually run:
build the app, add a card, open the dock's schedule popover from its
bottom-pinned anchor, toggle "run on a schedule" on (mounting the full
frequency-picker/cron-field/next-runs content — the same growth trigger that
broke web), and screenshot. Result: the popover renders fully above the
anchor with zero clipping. `StackCardView.swift` uses `arrowEdge: .bottom`,
`StackControlDockView.swift`/`StackTemplatesMenuView.swift` use `.top` — an
inconsistency, but cosmetic-only, since native `NSPopover` re-flips either
preference to whichever side actually has room. Left as-is.

**Why this belongs in the ledger despite being a non-fix:** it's the
resolution of the previous entry's open question, not a new decision — the
previous entry explicitly warned against inferring an `arrowEdge` fix from
the web bug without live evidence, and that caution paid off: the naive
inference (`.top` looks backwards for a bottom-pinned anchor, "fix" it to
`.bottom`) would have been wrong. `NSPopover`'s native repositioning made the
web-style bug structurally impossible on macOS.

**Also fixed live, same verification session:** the stack dock's split "run
stack ▾" button had a mismatched chevron-segment height relative to web (spotted
by the user from a live screenshot, not part of the original sprint scope).
First fix attempt (`.frame(maxHeight: .infinity)` on the chevron) overcorrected
into a much worse regression — a chevron bar stretching the full window
height — because SwiftUI's `HStack` doesn't stretch children to a sibling's
height the way CSS flex `align-items: stretch` does; `maxHeight: .infinity`
instead fills whatever *unbounded* space an ancestor offers. Caught immediately
via live screenshot before being reported as done, then corrected with a
measure-then-match `PreferenceKey` that reads `.runmain`'s actual rendered
height and applies it as a fixed `.frame(height:)` on the chevron — the
general-purpose SwiftUI technique for matching a sibling's height when the
parent stack won't do it automatically.

**How to apply:** when a SwiftUI layout needs "match my sibling's height"
(the CSS `align-items: stretch` behavior), reach for a `GeometryReader` +
`PreferenceKey` pair, not `frame(maxHeight: .infinity)` — the latter answers
a different question ("fill available space") and will visibly misbehave
the moment the parent has more room to give than the sibling used.

**Playwright added as a new web devDependency** (`@playwright/test` in
`web/package.json`, config at `web/playwright.config.ts`, specs under
`web/e2e/`) — the first browser-automation test tooling in this repo.

**Why:** the sprint's Phase 6 explicitly required e2e coverage for the
chain-scheduling flow and the popover-viewport regression, and
`web/src/lib/**/*.test.ts` (the `tsx`-run unit suite) has no browser — it
cannot drive real DOM layout/`ResizeObserver` behavior, which is exactly
what the popover fix needed proving. 8 specs were written and actually run
(not just written) against a live dev server: all 8 pass.

**How to apply:** future browser-level regressions (real layout, real
`ResizeObserver`/`IntersectionObserver` behavior, real cross-tab timing)
belong in `web/e2e/`, not forced into the `tsx` unit-test harness. Don't add
a second e2e framework — extend this one.

**XCUITest added as a new macOS test target** (`LopiUITests` in
`macos/project.yml`, sources under `macos/LopiUITests/`) — the first UI-level
test target in this repo (`LopiTests` is unit-only).

**Why:** same Phase 6 requirement, macOS side. Unlike computer-use (which
drives the *user's* screen interactively and was denied this session),
XCUITest drives the app's own accessibility tree via a test-runner process —
a different, already-implicitly-authorized mechanism (the same one
`xcodebuild test` uses for `LopiTests`). `build-for-testing` succeeds
cleanly; actually *running* `LopiUITests` hit a local code-signing/Team-ID
mismatch in this environment's DerivedData, unrelated to the test code —
documented rather than silently worked around or claimed as passing.
Element identifiers (`stack.dockExpand`, `stack.scheduleToggle`,
`stack.goalField`, plus `CardbarButton`'s `.accessibilityIdentifier(help)`)
were added alongside the tests rather than guessing at implicit AppKit
labels for icon-only buttons, which would have made the suite fragile from
day one.

**How to apply:** the next macOS session should resolve the DerivedData
signing mismatch (likely a stale/inconsistent local signing identity, not a
project.yml issue) and actually run `LopiUITests` before trusting it as a
real gate — see `NEXT_SESSION_PROMPT.md`.

## iOS-Research-1 spike + kill-test harness prep + eval-enforcement decision brief

Three phases, one real feature (the first). Per the sprint's own scoping: the
other two are tooling/docs, noted here plainly rather than written up as if
they closed something.

**Phase 1 (shipped): the package boundary is 15 files, not "the whole
directory."** Verify-4 established the *test* layer was framework-free;
re-verifying the *source* layer file-by-file (not trusting the rounder claim)
found two exceptions. `StackTheme.swift` imports SwiftUI directly (a `Color`
extension) and is UI theming, not domain — it was never a mechanical fit.
`CardOrbState.swift` is the sharper finding: it imports only `Foundation`, so
a directory-level import scan calls it clean, but `CardOrb.state(for:in:)`
reads `LiveAgent`/`ForgeOrbState` from `Store/`, both of which import
SwiftUI — a transitive dependency an import-statement grep can't see.
Moving it as-is would have quietly broken the entire point of the
extraction. Left in the app target; a real fix (a package-local protocol
`LiveAgent`/`ForgeOrbState` conform to from the app side) is future work, not
a mechanical port.

**The access-control work is the part "a move, not a rewrite" undersells.**
Every symbol in the moved files defaulted to `internal`, invisible outside
the file only because Views/Store shared its module. A separate package
makes that boundary real, and Swift's sharp edge is that it **never**
synthesizes a `public` memberwise initializer, even for a fully-`public`
struct — every struct without a hand-written `init` needed one added,
mirroring the implicit one's parameters/defaults exactly. Applied uniformly
by rule (default to `public` when unsure — over-exposing is harmless and
tightenable later; under-exposing is a compile error at the one point this
can actually be checked, and there is no compiler on this host). Spot-checked
against real call sites (`StackRunSeams`'s 7 closure properties against
`AppModel+Stacks.swift::makeStackSeams()`) rather than assumed correct.

**Prep, not execution, for the other two:**

- **MAXX kill-test instrumentation** (`crates/lopi-agent/src/quota_kill_log.rs`)
  — real, compiled, unit-tested Rust (unlike Phase 1, this crate builds on
  this host), but off by default (`LOPI_QUOTA_KILL_TEST_LOG` unset = zero
  behavior change) and never run against a live session. Extended
  `StreamEvent::RateLimit` with `surpassed_threshold`/`is_using_overage` —
  present in the real capture (`artifacts/STREAM_CAPTURE.jsonl`) but
  previously decoded nowhere, which would have silently defeated kill test
  1's actual question (is the event threshold-gated). Scoped as a
  process-wide `OnceLock`, not threaded through `AgentRunner`: a single
  `lopi run` CLI invocation is one process, matching the kill-test
  protocol's intended single-task usage; running it against concurrent
  `lopi sail` tasks would interleave their events into one cadence count — a
  named caveat, not a silent one. `.konjo/scripts/quota-kill-test-log.sh` is
  the one command the next session runs on real hardware.
- **Eval-enforcement decision brief** (`docs/ops/EVAL_ENFORCEMENT_DECISION.md`)
  — re-reading `LEDGER.md`'s own A1/macOS-Loop-Stacks-1 entries (per the
  sprint brief) surfaced a bigger finding than expected: **the claim that
  `acceptance`/`budget_tokens` are "not wired to the live body" is only true
  for macOS, and even there it's a bug, not a scope decision.** The server
  has applied both since A1/A3 (`handlers.rs:290-297`); web has sent both
  since A1 (`stack.ts::cardToTaskPayload` → `api.ts::createTask`'s options
  spread). Only macOS's `launchStackTask` silently drops them when mapping
  the pure payload onto the real wire struct — its own code comment claiming
  this was deliberate is what every later doc (this ledger included, twice)
  trusted instead of re-checking against `stack.ts`. Not fixed here (the
  sprint's own instruction); flagged as a follow-up task, not wired even
  partially.

**Housekeeping:** none of the three "not fixed here" items above are silent —
Phase 1's compile-risk flags live in `IOS_RESEARCH_1_SPIKE.md`, Phase 2's
"run this on real hardware" lives in the script + `NEXT_SESSION_PROMPT.md`,
and the macOS acceptance/budget_tokens bug is flagged as a standalone
follow-up task, not folded into this sprint's diff.

## Loop Stack connect & test — auto model, branch round-trip fix, bumpCard UI

**The audit this sprint was scoped against was already stale, and re-verifying
against the live repo (not the prompt's specifics) is what found the real
bug.** The prompt's Phase 3 assumed the branch picker had "zero prior
callers" — untrue since `repo + branch pickers` shipped it into
`ConfigDrawer.svelte`/`StackConfigPopover.svelte`. But verifying that claim
(rather than trusting either the stale prompt or the shipped feature) surfaced
a real gap the audit never described: `card.config.branch` reached the wire
via `paneSubmitPayload` (bare-pane launch) but not `cardToTaskPayload` (the
run-stack sequencer's actual call site) or `evaluateStackAcceptance` (the
stack-eval task). A branch chosen in the UI silently did nothing once a
multi-card stack ran. **The lesson, stated for future sprints: re-verifying a
"this is already done" claim is not optional busywork — this sprint would
have shipped nothing real on Phase 3 without it.**

**`PaneDefaults.branch` made optional rather than adding a second, richer
defaults type.** `cardToTaskPayload`/`cardToTaskPayloadForRunOnce`/
`dryRunStack` are typed against the narrower `PaneDefaults` (`model`/
`effort`/`repo`), but every real call site actually passes the richer
`StackDefaults` (`+branch`/`autonomy`) — TS structural typing already made
this safe at every call site; the type just hadn't caught up. Adding
`branch?: string` to `PaneDefaults` (optional, so the one bare `{model,
effort, repo}` test literal in `stackRun.test.ts` still satisfies it) closes
that gap with a one-line type change instead of threading a second type
through four function signatures.

**`auto` (`MODEL_OPTIONS`) is a client-only sentinel, never a wire value —
the same pattern `branch` already established for a config field with no
`CreateTaskRequest` column of its own, reused rather than reinvented.**
Selecting it means "omit `model`," not "send the string `auto`" — verified
against `select_model` (`claude.rs:45-59`): `task.model.is_some()` short-
circuits the heuristic and would pass `"auto"` straight to the CLI as
`--model auto`, a guaranteed failure. Appended last in `MODEL_OPTIONS` (not
first) specifically so it doesn't silently become `DEFAULT_STACK_DEFAULTS
.model` / `controls.ts`'s launch-control seed via the codebase's existing
`MODEL_OPTIONS[0]` convention — a real behavior change (every new stack's
default model silently switching to heuristic-selected) that this sprint
was not scoped to make and did not make.

**Backend needed zero changes for `auto` to work.** `apply_loop_fields`
(`crates/lopi-ui/src/web/handlers.rs`) already leaves `task.model: None` when
the wire `model` key is absent (`#[serde(default)]`), and `select_model`
already runs its heuristic on `None`. The gap was 100% client-side (the UI
never had a way to *not* send a concrete model). Proven end-to-end — request
mapping through to the heuristic's actual model choice, not just the pure
`select_model` unit tests in isolation — by a new `lopi-ui` test that adds
`lopi-agent` as a **dev-dependency only**, so the production dependency graph
(`lopi-ui` → `lopi-orchestrator` → `lopi-agent`, never `lopi-ui` → `lopi-agent`
directly) is unchanged.

**Phase 1 (wiring `acceptance`/`budget_tokens` onto the live `CreateTaskBody`)
was scoped as conditionally in-play, pending whether A1's `VerifierAgent`
reuse counted as "the evaluator landing server-side." It doesn't — confirmed
by re-reading this ledger's own Eval-Execution-1 (A1) and macOS-Loop-Stacks-1
entries, not by assumption.** A1 promoted `VerifierAgent` into the tiered eval
*judge*, real and load-bearing for a task's own pass/fail — but
macOS-Loop-Stacks-1's entry is explicit and post-dates A1/B1: `acceptance`/
`budget_tokens` are carried in the pure payload and unit-tested, "intentionally
not wired to the live body... acceptance/goal-execution is A1–B1's evaluator
track ('no backend changes')." Nothing this sprint touched changes that.
Skipped rather than forced, per the sprint's own instruction not to fake it.

**Phase 3 (branch) and Phase 4 (pane creation), as literally scoped, needed
no new code.** The topbar's `+` (`Add pane`) already dispatches
`window.dispatchEvent(new CustomEvent('lopi:add-pane'))`, handled in
`routes/stacks/+page.svelte` since before this sprint; `deleteStack`'s
last-pane refusal is unchanged and still flagged in `NEXT_SESSION_PROMPT` as
"worth revisiting together," per `NEXT.md`'s own standing note — not
unilaterally decided here.

**Version:** `0.10.0` → `0.11.0`, straight increment on top of MAXX's own
`0.7.0` → `0.10.0` catch-up (merged to `main` first). No drift to reconcile
this time — `CHANGELOG.md` and `Cargo.toml` now agree.

## MAXX — opportunistic backlog dispatch, gated on quota headroom

**One-way doors this sprint opened:**

- **`AgentEvent::ApiRetry` gained `resets_at: Option<i64>`.** `#[serde(default)]`
  so the wire format stays backward-compatible and the three-language golden
  fixture didn't need a matching update — but any future consumer of `ApiRetry`
  (TS `parser.ts`, the Swift decoder) that starts asserting on exhaustive field
  sets will need to learn about this field. Chosen over a separate `resets_at`
  event because it's the same underlying `rate_limit_event` payload; splitting
  it into two events would have meant correlating them by `task_id` + a race
  window for no benefit.
- **New persisted `quota_observations` table, one row per `limit_type`.**
  Deliberately keyed by `limit_type` (not a single "last event wins" row) —
  `five_hour` and `seven_day` arrive through the identical `ApiRetry` variant,
  so a scalar-overwrite design would silently lose one window's state every
  time the other updates. `QuotaTracker::snapshot` returning `None` for an
  unobserved window (rather than defaulting to `0.0`/favorable) is load-bearing
  for Phase 1: it's what keeps `maxx_loop` from ever treating "we don't know"
  as "it's fine to dispatch."
- **New `MaxxEntry` type + `/api/maxx` routes**, deliberately shaped to mirror
  `ScheduleEntry`/`/api/schedules` rather than inventing a new convention.
  Anyone touching one CRUD surface without touching the other should notice
  the asymmetry immediately — that was the point of mirroring it exactly.
- **`headroom_favorable` requires every configured window to be favorable
  (`AND`), not any one of them (`OR`).** A real dispatch spends quota against
  every window simultaneously — a `five_hour` window with no headroom left
  makes a dispatch unsafe even if `seven_day` looks comfortable. Getting this
  backwards (`OR`) would look correct in testing (the happy path where both
  windows agree) and only misbehave once a real account has one window under
  pressure and the other not — exactly the situation MAXX exists to be careful
  around. Locked by `headroom_favorable_requires_every_configured_window`.
- **A 1-hour per-entry refire cooldown, not in the sprint's locked spec.**
  The sprint's Phase 1 design is a straight favorable/not-favorable check per
  tick with no mention of a cooldown; without one, an entry with an 8-hour
  quiet-hours window would resubmit its identical goal on every 5-minute tick
  all night — ~96 duplicate runs, burning exactly the quota headroom this
  feature exists to protect. Added deliberately as a safety property of the
  tick itself rather than left for a future sprint to discover the hard way.
  If a real use case needs faster re-dispatch of the *same* entry, that's a
  config knob to add later, not a reason to remove the default.
- **Kill tests 1–3 (firing cadence of `rate_limit_event`, `resetsAt`
  reliability, canary-probe cost) were not run.** They require instrumenting
  a live `lopi run` session with real Claude Code auth across low/mid/high
  utilization, which this sandboxed session cannot do. The gating numbers in
  `maxx_loop.rs` (`HEADROOM_UTILIZATION_MAX = 0.5`, `HEADROOM_RESET_WITHIN_SECS
  = 2h`) are therefore reasoned defaults, not empirically validated ones. The
  design was kept conservative specifically so this gap is safe to carry
  forward: a missing/stale observation is always "don't dispatch," never
  "assume favorable," and no canary probe was built (kill test 3's premise —
  that the event might be threshold-gated — was never confirmed, so spending
  real quota on an unvalidated probe mechanism would have been the wrong kind
  of decisive). **This needs to be closed out on real hardware before MAXX
  ships to anyone who isn't explicitly opting into an unverified feature** —
  see `docs/ops/NEXT_SESSION_PROMPT.md`.
- **MAXX's popover only exposes one interactive control (the enable
  toggle).** The locked design's "run" list (quiet hours / headroom gate) is
  descriptive text, not per-field editors — `MaxxConfig.quietHours` and
  `.headroomGate` exist on the client type and are sent to `/api/maxx` on
  create, but nothing in this sprint lets a user change them from the
  defaults (`11PM–7AM`, both windows). This is a real gap, not an oversight:
  building the editing UI wasn't in the locked Phase 2 spec, which showed
  static text only.
- **Version:** `0.7.0` → `0.10.0`. Catches up a two-version drift where
  `CHANGELOG.md` had already reached `[0.9.0]` (Stack-Templates-1, both
  platforms) without a matching `Cargo.toml` bump in either of the last two
  sprints — this sprint's version now matches `CHANGELOG.md`'s actual
  sequence again.

## Creation-Flow-1 (macOS) — the draft card, ported to SwiftUI

**The model is the web model, verbatim.** `CardStatus.draft`, `StackCard.tpl`/
`tplKind`, `PromptTemplate`/`StackTemplate`/`TemplateLoop`, and the pure
functions (`applyPreset`/`applyPromptTemplate`/`applyStackTemplate`/
`stackTemplate(from:)`/`finalizeDraft`/`makeDraft`/`draftIsHot`) are 1:1 ports
with the same names, ordering, and semantics as the web sprint (`[0.6.0]`). Same
reasoning as every macOS-parity sprint: divergence between the two surfaces is a
bug, not a platform idiom, so the models are literally the same shape and the
tests are literal ports.

**Draft-as-`CardStatus` earns its keep in Swift specifically.** Making the draft
a `.draft` case (not a `DraftCardView` fork) means the compiler's exhaustive
`switch` requirement *forced* every `CardStatus` consumer to handle it — the
draft can't silently fall through to a run path, which is exactly the §1.1 rule,
enforced by the type system rather than by review. The draft lives on
`StackPaneState.draft` via a defaulted custom init, so every existing pane
construction site stayed unchanged.

**Chip colors + provenance semantics** match the web exactly (sun replaces the
alias chip for a prompt template; violet + the loop's own teal alias chip for a
stack template; teal alias chip for no template). Every SF Symbol size is
constrained — an unconstrained glyph blows the chip apart, same failure mode as
the web's missing `svg{width;height}`.

**Persistence is `UserDefaults`, honestly per-machine and NOT synced with web.**
Same key (`lopi.templates.v1`) and JSON shape as the web's localStorage so the
two are conceptually identical, but they are two physical stores that never talk.
This is a **real limitation, stated plainly**: a template saved on the web is not
visible in the macOS app and vice-versa. Fixing that needs a backend (see
`NEXT_SESSION_PROMPT`), which is out of scope.

**Bottom-first serialization** is the same load-bearing invariant as the web:
`addCard` prepends (bottom runs first), so `stackTemplate(from:)` serializes
bottom-first and `applyStackTemplate` prepends in reverse. Pinned by a ported
round-trip test — the two platforms must agree, and now provably do.

**Deliberate native deviation:** the templates control is a SwiftUI `.popover`
(the app's existing popover mechanism) with a hand-colored sectioned list, not a
native `Menu`. A native macOS `Menu` can't tint per-section text, and the web's
color-coding is load-bearing (the colors are how the card says where it came
from), so the popover wins on fidelity. Name prompts use native alerts (the macOS
analogue of the web's `window.prompt`).

## Creation-Flow-1 (web) — the draft card replaces the composer

**Draft-as-`CardStatus`, not a separate component.** The pre-commit draft is a
`StackCard` with `status: 'draft'`, rendered by the *same* `StackCard.svelte`
(a draft branch), never a `DraftCard.svelte`. Rationale: a forked draft
component is exactly what let the two surfaces drift in the mockups — one card
component means one place for the cardbar, popovers, and chips to change. The
draft lives on `StackPaneState.draft`, never in `pane.cards`, so it is excluded
from run/reorder/loop-count *by construction*; `executionOrder` also filters
`'draft'` so no run path can ever schedule one.

**Template provenance survives edits — it records origin, not drift.** `tpl`/
`tplKind` are stamped when a template fills a card and are never cleared by later
edits to `goal`/`preset`. A card says *where it came from*, not *whether it still
matches*. Picking a bare preset (not a template) clears provenance, because a
preset is not a template origin.

**Chip color semantics are load-bearing, not decorative.** prompt template → a
sun chip that *replaces* the alias chip (the template is the prompt's identity);
stack template → a violet chip *plus* the loop's own teal alias chip (each loop
in a chain keeps its distinct preset); no template → the teal alias chip. The
colors match the dropdown sections so the card says where it came from at a
glance. Every chip gets an explicit `svg` size (a missing one renders full-size
and blows the card apart — a real mockup bug).

**Persistence is localStorage-only and honestly labelled client-only.** Templates
live under `lopi.templates.v1` in one browser profile. No backend, no sync — the
store comment, the CHANGELOG, and this ledger all say so rather than implying a
durability we don't have. Every access is try/catch'd; a private-mode / quota /
corrupt-JSON failure degrades to empty and never throws into a click handler.

**Bottom-first template serialization — the easiest thing to get backwards.**
`addCard` prepends, so the bottom card is oldest and runs first.
`stackTemplateFromCards` serializes bottom-first and `applyStackTemplate`
prepends the loops in reverse, so a saved chain round-trips into the identical
run order (first loop at the bottom). Pinned down by an explicit round-trip unit
test, not left to inspection.

## macOS-Parity-Cut-1 — remove what web already cut (front + back + tests + docs)

**The reversal, stated plainly.** `macOS-Loop-Stacks-1`'s README framed the Tools/
Health/Patterns/Audit/Tasks admin panels as *deliberately native-exclusive* — web
folded or cut them, macOS kept them. This sprint reverses that: macOS should not
carry UI for features web no longer has. Twelve `NavSection` cases → six (`forge,
dashboard, budget, cron, loop, config`); the six removed views and their orphaned
backends are gone.

**Backend fate was decided per-endpoint against *verified* callers, not the
assumption "macOS no longer uses it" = "nothing uses it."** Pre-flight grepped web,
macOS, CLI, TUI, and tests for every candidate. The results split three ways:

- **Removed — zero callers after the panel went (Tools, Health, Patterns, Audit):**
  `/api/patterns`, `/api/audit`, the agent-health HTTP surface (`/api/agents/:id/health`,
  `/api/agents/health/summary`, `/api/agents/:id/heartbeat`), and `/api/tools*`. Each
  was macOS-panel-only — web's clients were already deleted in Unify-2, and no agent
  code consumes them (the `HealthRegistry` and `ToolRegistry` in `AppState` were read
  *only* by their own HTTP handlers; the health "sweeper" the struct comment
  mentioned was never actually spawned in lopi-ui). Removing them cascaded cleanly
  into `AppState.health`/`tools`/`patterns_cache` + the `TtlCache` helper + the
  `lopi-tools` dep. The library types (`lopi_orchestrator::HealthRegistry`,
  `lopi_tools::ToolRegistry` — still used by `lopi-mcp`) stay.
- **Kept — generic, not the removed feature:** `GET /api/health`. The doc listed it,
  but verification showed it is a static liveness probe (`{"status":"ok"}`) unrelated
  to the agent-**Health** panel (which used `/api/agents/health/summary`). Removing it
  would be scope creep that could break external monitoring. Kept.
- **Initially kept, then removed outright — the dead-letter queue.** The first pass
  kept `/api/tasks/dead-letter*` because web's `api.ts` still exported and unit-tested
  `listDlq`/`retryDlq`/`deleteDlq` (Overview's `dead-letter` chip is a **client-side
  filter** over the live agents store — it imports `$lib/stores/agents`, never
  `$lib/api` — so the "Overview depends on it" clause never fired; the only stakeholder
  was that retained web client). A follow-up call reversed this: **remove the DLQ
  completely, web included.** Gone across every layer — `dlq_handlers.rs` + routes,
  the `MemoryStore` dead-letter methods + `dead_letter.rs` + the `dead_letter_queue`
  table, the orchestrator `push_dlq` write path, and the web client + its tests. The
  write path was verified purely additive before deletion: `push_dlq` only wrote a
  `dead_letters` row + a `task.dead_letter` audit entry; task failure status is marked
  independently by `run_one`/`mark_completed` and the pool `failed` counter, both
  untouched. So exhausted tasks are still marked `failed` and counted — they are just
  no longer separately dead-lettered or retryable. This retires the DLQ feature rather
  than deferring it.

**The Tasks removal is a deliberate capability gap, not a mechanical parity cut —
recorded here so it is not re-litigated as a bug.** Web folded task history into
Overview; macOS has no Overview yet. Removing `TasksView` therefore removes the native
app's *only* way to view task history — a new gap specific to macOS, not a loss web
already absorbed. The call (confirmed with the owner before the phase ran): remove it
anyway to hit the full-parity goal, and defer the capability to a future macOS
Overview. Dead-letter *management* is a separate matter: the DLQ was retired entirely
(above), so it is not a deferred-until-Overview gap — it is a removed feature. A future
Overview that wants dead-letter recovery would rebuild the subsystem, not re-expose a
retained backend.

**Next session — this sprint's direct follow-up.** Build a macOS Overview equivalent
(the read-only app-wide rollup web has at `/overview`) to close the task-history gap
this sprint knowingly opened. It is scoped follow-up work, not an indefinite deferral.
It does **not** restore dead-letter management — that subsystem is gone by decision.

## macOS-Loop-Stacks-1 — bring Loop Stacks to the native app

**Sequencer fork: functional port, taken (not visual-first).** The prompt flagged
the same fork macOS-Parity-1 raised — port `stackRun.ts`'s sequencer to Swift, or
ship a visual-first shell that defers goal-directed sequencing. Pre-flight
confirmed the port lifts cleanly: `stackRun.ts` is already written against injected
seams (it takes `statusSource` as a *parameter* rather than importing `./agents`,
precisely so its unit tests can substitute a plain `writable(new Map())`). So its
pure decision core — `advance`/`pursueGoal`/`decideAfterMiss`/`foldGain`/
`bumpInOrder` — ports to a Swift `StackRunEngine` with `StackRunSeams` (createTask
/ waitForTerminal / score / createSchedule / reorderPaneCards) injected the same
way; production wires them to `LopiClient`/`liveAgents` in `AppModel+Stacks`, tests
wire a deterministic mock mirroring the web `mockBackend`. A native app should run
stacks the way web does, not defer to a server that has no stack concept either.

**This supersedes macOS-Parity-1's two-target framing.** That doc predated
Unify-1/Unify-2, when Forge and Stacks were two things to port. Web unified them —
`forge/+page.svelte` is gone, `/stacks` is the only route, a bare pane *is* a
one-card stack. So macOS extends its existing 965-line Forge into stacks rather
than adding a parallel Stacks screen: **one `.forge` nav item, not two.** A
single-card pane is the regression bar — visually + functionally the old Forge
pane; the connectors + purple dock appear only on a second card.

**Pure-Swift domain types (zero SwiftUI/AppKit), by decision.** `StackStore`/
`StackGoal`/`StackRun` and the whole `macos/Lopi/Stacks/` layer import only
Foundation (+ Observation for the two store wrappers — the svelte-`writable`
analogue, not a UI framework). This costs nothing today and directly de-risks
`iOS-Research-1`'s still-open shared-package-boundary question: the core is already
portable, so R-1 evaluates a *move*, not a rewrite. The pure ops are Foundation-
only; only the observable wrappers touch Observation.

**Live-verify owed, stated plainly.** Swift does not build on the authoring host
(Linux) — the same constraint every macOS round has carried ("build on the M3").
The ported Swift tests mirror web's `.test.ts` 1:1 (same fixtures/assertions) and
are the acceptance bar, but they were not *run* this session; the single-card
regression screenshot and the live dual-scenario run (bare pane + multi-card stack)
are the immediate next step, same discipline as every round since Ops-2.

**WIRED-fields honesty gap, made explicit.** `CreateTaskBody` gained the additive
optional `max_iterations`/`on_fail`/`gate`/`until`/`client_ref` fields the backend
already honors, so guardrails + max-iter round-trip live. `budget_tokens` and
`acceptance` are intentionally *not* wired to the live body — `budget_tokens` has
no request field yet, and `acceptance`/goal-execution is A1–B1's evaluator track
("no backend changes"). The pure payload still carries both and is proven by test;
the live wire carries only what the backend accepts today. A future sprint that
lands the eval backend wires acceptance through the same seam.

## Fix-3 — macOS stats/cost parity (F9 + F10 + the F6 port)

**Phase 1 (F10 counts) chose "macOS counts from its own live session map" over
"make the WS `pool_stats` event carry DB `status_counts`."** The prompt offered
both. The deciding factor was fidelity to the reference: Fix-2 did *not* change
the `pool_stats` event on web — it made the topbar count from the local `agents`
map and left the pool event supplying only uptime (see the Fix-2 entry below).
Mirroring that exactly means the macOS `.poolStats` handler drops its running/
queued/succeeded/failed assignments and the tiles count `liveAgents` through a
new `FleetBucket` mapping (the Swift mirror of web's `dbStatusToUiStatus`). This
also (a) needs **zero server change**, so it can't regress the web path or any
other `pool_stats` consumer; (b) reuses the exact source the cognition grid's "N
active" already counts correctly, so the two can never disagree; and (c) is
strictly *more* live than a DB round-trip — the session map updates on every
event, seeded from the DB-backed snapshot on connect. The rejected option would
have coupled a client tile fix to a wire-event schema change for no gain the
session-map count doesn't already deliver. **Invariant for future stats
consumers on macOS: count the local `liveAgents` map (or read `/api/stats`),
never the per-pool `.poolStats` event — it is uptime-only by contract now.**

**F9 (cost today) is a poll, not a push.** `stats.totalCostUsdToday` comes from
`/api/stats` (DB `daily_token_totals`, already cross-pool-correct after Fix-2),
and the WS stream carries no cost — so the fix is simply to keep re-reading it (a
5 s background `Task`), not to thread cost through the event spine. Adding cost
to the WS payload was the heavier alternative and buys nothing the poll doesn't:
the number is a whole-day DB aggregate, not a per-event delta, so event-rate
freshness is wasted on it. The one coupled correctness fix: `applySnapshot` must
*not* overwrite the polled cost with the snapshot's stats (which carry counters +
uptime but never the daily totals) — otherwise COST TODAY flashes `$0` on every
reconnect.

**F6 (Budget SPENT) was a decode gap, not a missing event.** The Swift client
already decoded and handled the `.cost` / `turn_metrics` live events (per-agent
`costUsd` + `recomputeAggregates`), so *running* tasks were fine. The break was
that `applySnapshot` seeded only id/goal/phase and ignored the per-task `cost`
Fix-2 added to the snapshot wire — so already-finished tasks hydrated at `$0`,
and the `liveAgents`-sum that `/budget` "spent" reads stayed `$0`. The macOS
analog of web's "the defensive parser dropped the field" — same lesson, mirrored:
a new snapshot field is invisible to the client until the seeding path is taught
to read it. Fix hydrates cost only for freshly-seeded ids, matching web's upsert
that skips ids it already holds, so a live task's incrementally-updated cost is
never clobbered by a staler snapshot on reconnect.

## Fix-2 — wire the bare-pane launch, close the Verify-1 fast-follows

**F2's root cause: the single-prompt launch was built pure-and-tested but never
given a click target.** Unify-1 collapsed Forge's `postTask` into the unified
`createTask` path and left `paneSubmitPayload` — a deliberately loop-semantics-
free payload builder for the "one prompt, no stack chrome" case — behind, proven
by `stack.test.ts`. But Unify-2 then made a 0–1-card pane *bare* (`paneIsBare`),
and the only host of the run action (`StackControlDock` → `runStack`) renders
only for non-bare panes. So the launch *logic* existed and the launch *button*
existed, but never in the same pane: a bare pane could not launch at all. The
fix keeps that separation intentional — a bare pane gets its own `runBarePane`
(a single-card, no-chain sibling of `advance` that submits through
`paneSubmitPayload`, so a bare prompt stays a bare prompt), not the stack dock.
The invariant to preserve: **the bare path never acquires stack-loop semantics**
(`max_iterations`/`on_fail`/`gate`/`acceptance`) — that's the whole reason
`paneSubmitPayload` exists apart from `cardToTaskPayload`.

**F3/F4's real mechanism: `/api/stats` and the WS snapshot counted from a
*per-pool* in-memory counter, and multi-repo mode runs one pool per repo.**
`sail --repos` spawns a separate `AgentPool` per extra repo; `s.pool` is only the
primary. Its `stats()` atomics therefore see only primary-repo tasks — the
undercount Verify-1 measured ("1 live" while 2 ran; `succeeded` 3 vs 7). The
load-bearing choice: **the DB is the one cross-pool source of truth**, so counts
come from `MemoryStore::status_counts` (a `GROUP BY status`), not any pool
counter — mirroring how per-task cost was already derived from `turn_metrics`
rather than a pool tally (Polish-1). On the client, the topbar likewise stops
preferring the WS `poolStats` (same per-pool origin) and counts from the local
`agents` map, which the shared event bus already makes complete across repos —
the exact source the Overview buckets used and got right. Future stats consumers
should read the DB or the local agents map, never a single pool's counters.

**F6's real mechanism: cost was dropped three times on the way to the client.**
The WS snapshot didn't carry per-task cost; adding it wasn't enough because the
*defensive* wire parser (`parseWireMessage`) reconstructs each snapshot task from
a known-field whitelist and silently dropped the new field; only then does the
reducer read it. All three had to carry `cost` for `/budget` + Overview to
hydrate real spend. Lesson for future wire fields: the defensive parser is a
whitelist — a new field on the server is invisible to the client until the
parser is taught to keep it.

## Polish-1 — close bug #3, purge cut-feature remnants, resolve the two open decisions

**Cost/token accrual: persist on the CLI path, and the invariant is "one turn,
one writer."** Bug #3 (`/api/stats` and per-task cost read `$0`) was not a
display bug — the whole read side (`daily_token_totals`, `run_turn_aggregates`)
correctly sums `turn_metrics`, but the **billed CLI path never wrote a row**.
The load-bearing choice was to persist from `runner/stream.rs` after each
streamed call completes, accruing token deltas + the terminal `result`'s
authoritative billed `total_cost_usd`, **not** to re-estimate cost at the read
layer. The correctness invariant to preserve in later sprints: a given turn is
recorded by *exactly one* path — the direct-API planning path (`api_plan.rs`)
records its own planning turn, the CLI path records the implement turn (and the
plan turn when direct-API isn't configured), and the two never overlap for the
same turn. Per-task `cost` is *derived* from `turn_metrics` (`task_costs()`),
deliberately not a new `tasks.cost` column — single source of truth, no
write-path to keep in sync.

**The cut is web-only; the macOS admin panels are a platform-exclusive surface,
not remnants to purge.** This is the boundary a future cleanup must not cross.
Unify-2 collapsed the *web* nav; the same feature names (Tasks, Tools, Health,
Patterns, Audit, Dashboard) survive on macOS as first-class native panels that
Ops-2 verified live (12 of 13 wired). Removing them from macOS would be *opening
a new decision*, not finishing an existing one — explicitly out of scope. So the
Phase-1 sweep deleted only genuinely-orphaned web client code (components with
no importers, `api.ts` wrappers with no callers) and corrected docs, while
leaving every backend route those panels depend on intact.

**Dashboard: kept, decided against current reality.** The original theory was
"Overview absorbs Dashboard." But Dashboard is macOS-only and Overview is
web-only — they never shared a platform, so Overview cannot absorb Dashboard's
job for a native user. Now that Overview's bucketing is fixed (Fix-1) it covers
the *web's* need; macOS keeps Dashboard as its richer at-a-glance cognition grid
(correct buckets off `/api/stats`, cost tiles fixed by Phase 0). Cutting it would
leave the native app with no rollup at all. The original plan predated knowing
Overview would ship web-only.

**Orb-parity: standardize on the compact per-pane orb — resolved, not deferred a
third time.** Web already replaced its hero orb with a compact per-card `OrbDot`
(a status dot); macOS still rendered a 120–300pt Metal orb per live pane, which
does not scale once several panes are visible — the exact multipane case Unify-2
built the grid for. Chose the compact treatment (orb-as-status-indicator
everywhere, Unify-2's actual intent) over the single-hero Metal orb: the macOS
live-pane orb is now a small status indicator; the idle launcher keeps a larger
orb because it's a single-pane launch affordance, not the crowded grid. macOS is
authored on Linux and built on the M3 per this repo's standing convention, so
the visual sizing is pending an on-device confirmation — but the *direction* is
decided, not deferred.

## Unify-2 — one pane primitive, one status vocabulary, one rollup, a four-item nav

**The orb is the single status vocabulary — the `.runtag` badge is retired, not
kept as a fallback.** A `StackCard` no longer renders its own `card.status` text
badge; it looks up its live agent by `card.taskId` in the shared `agents` store
and renders `computeOrbState()`. The load-bearing choice was to route the card
through the *exact same pure function* the Forge orb uses (via a leaf module,
`lib/forge/cardOrb.ts`, with no store/`$app` imports) so parity is provable, not
asserted — a card and a pane cannot drift because they share the mapping and the
key. `card.status` survives only as the coarse client run-lifecycle marker the
sequencer sets (drives the running/output-flash coordination); it is no longer a
*second* status vocabulary living beside the orb.

**One pane primitive: a bare `StackPane` covers the old Forge box, so the
parallel tree is retired.** `paneIsBare` (≤1 card) gates the collapse: a
one-card pane shows composer + card + orb and hides the connector + purple
control dock, so it reads like a pre-Unify Forge pane; a second loop earns the
full stack chrome. Coverage was confirmed *before* deletion (grep-confirmed no
importers), then `AgentGrid`/`AgentPane`/`SessionSidebar` and the `/forge` route
were retired outright. **Deliberately preserved, not deleted:** the WebGL orb
renderer (`ForgeStage`/`Forge.svelte`) — the brief named only the three
components, and `OrbDot` is a compact form of the same orb, so the full renderer
is kept for reuse and flagged for a later "delete or re-home" call rather than
cut speculatively.

**Overview absorbs the *information* of three surfaces, and explicitly not the
fourth.** `/overview` is the sole replacement for Fleet + Dashboard + Pulse
(per-agent metrics, whole-fleet glance, live status) as one read-only rollup
over the `agents` store — which is already the app-wide source of truth for
every launch. Constellation's 3D orbital rendering was **not** folded in: it is
cut in full, because it's a visualization, not information, and keeping it would
re-introduce the surface sprawl the sprint exists to remove. Tasks folded in too
— its dead-letter view is now a filter on Overview, not its own page.

**Patterns: the web panel is removed; the mining store and A2 feed are not
touched.** The decision boundary is display-vs-data: the Debug sub-panel that
*showed* learned patterns is gone, but the pattern-mining store and its A2
reflection feed are load-bearing for A2 and stay. (macOS's first-class
`PatternsView` is a separate surface — flagged for macOS-Parity-1, not reached
into from this web sprint.)

**Router is fully removed, not nav-hidden — and its disconnection was
re-verified before deletion, not taken on faith.** The prior audit's finding
(that `create_task` routes via `pool.submit()` with zero `ConstellationRouter`
reference) was re-confirmed directly against current `web/handlers.rs` before
anything was deleted. Because the router is genuinely dead code, removal was
total: the `/router` page, the three `/api/constellation*` endpoints +
`constellation_handlers.rs`, the app-state field, and the entire
`lopi-orchestrator/src/constellation/` module (types/selector/tests/re-exports).
Non-code mentions (doc comments, a tier feature string) were left alone.

**The sandboxed-CI live-verification constraint is now a standing fact, recorded
once.** Live `sail`-spawned `claude` cannot authenticate in this CI sandbox —
`scrub_inherited_anthropic_env` strips `ANTHROPIC_BASE_URL` and there is no
interactive `~/.claude` subscription login. This is confirmed, not theoretical
(Unify-1 Phase 1 hit the same wall). The split is therefore explicit and
permanent for this environment: **structural proof in-sprint (tests / check /
build / cargo), live proof post-merge by the operator.** Future sprints should
treat this as settled and not re-attempt the live gate here.

## Goal-directed stacks (B1) — binary run-until-goal, because there's no whole-chain rollback to gain-gate against

**The load-bearing decision: ship the binary "re-run the chain until the stack
acceptance passes or a stop reason fires" model, and defer stack-level
gain-gating — because the rollback it would require does not exist.** The §0 fork
was binary run-until-goal vs. gain-gated chain re-runs (keep a re-run only if it
*gained* on the stack metric, rolling back worse chain-runs). Gain-gating at
stack scope needs **whole-chain rollback**: a snapshot of the aggregate repo
state before a chain-run, restored if the run regresses. Pre-flight found none —
each card is its own task doing its *own* per-loop rollback (A1/A3), committing/
PR-ing independently; there is no backend that snapshots or restores "the whole
client-side stack." Per the brief's rule ("don't fake a rollback that doesn't
exist"), gain-gating is deferred to NEXT with that reason, and the binary model —
which is the entire roadmap payoff — ships. If a real whole-chain snapshot/restore
ever lands, gain-gating becomes a clean follow-up reusing A3's `GainRule`.

**The stack-scope eval seam (B1's main unknown): a dedicated eval task, because
stacks are 100% client-only.** There is no server-side "stack" concept — confirmed
against `crates/lopi-ui/src/web/` (the only acceptance surface is task-creation
ingest; `grep stack` in the handlers is empty). Of the three candidate seams the
brief listed — launch a dedicated eval, read the final loop's `EvalOutcome`, or
have the backend expose a stack outcome — **launch a dedicated eval** is the only
one that fits a client-only stack with zero backend change. After each chain-run
the sequencer submits one task carrying the compiled stack `Acceptance`
(`evalsToAcceptance(config.evals)`); A1 already makes a task's terminal status
*iff*-equal to its acceptance verdict (`runner/eval_runner.rs`), so `completed` =
`goal_met` and non-completion = a miss. Reading the final loop's outcome was
rejected: the final *card* carries its own card-evals, not the stack's, and the
client can't read a task's persisted `EvalOutcome` anyway (it observes `status` +
`score` off the event stream, nothing more). A backend stack-outcome endpoint was
deferred as the *honest refinement* (below), not the minimum.

**The honest caveat, recorded not hidden: the stack eval is a real single-attempt
task, not a side-effect-free eval.** lopi has no standalone eval primitive — a
task always runs an agent. So the stack-acceptance "check" is a `max_iterations:
1` task: it makes at most one verification attempt, and the *iterative* progress
comes from re-running the chain across chain-runs, not from the eval doing the
work. The clean fix is a pure `POST /api/evaluate` endpoint that runs A1's
`TieredEvaluator` against a repo with the same `EvalContext` A1 builds at finalize
but **no agent work** — recorded in NEXT. It was not built here because it is
backend scope (Rust + the full Konjo gate battery) for a refinement, where the
client-only path proves the whole run-until-goal loop today with zero backend risk.

**Stack `StopReason` precedence mirrors A3 verbatim, one scope up.**
`stackGoal.ts`'s `StackStopReason` is `lopi_core::StopReason` with the loop-scope
`max_iterations` re-cast as chain-scope `max_chain_loops`, same wire strings, same
rank (`goal_met` 3 > `budget` 2 > `no_progress` 1 > `max_chain_loops` 0), same
`precede`. Two deliberate honesty choices in the client mapping: (1) **`budget`
never trips client-side** — there's no observable stack-level token meter (same
stance as Stack-1's unenforced stack budget), so it stays in the precedence for a
future meter but never fires, and is never rendered as enforced; (2) **`no_progress`
is real, not a second ceiling** — it reads the stack-eval task's observed `score`
across chain-runs and stops when the best hasn't gained by A3's margin for N runs
(`foldGain`), so it's genuinely "stopped improving," not "ran N times." An
unobservable score advances neither best nor streak — don't fake a signal.

**Reuse, not rebuild.** The executor, gain gate, and reflection are untouched;
`evalsToAcceptance` (Stack-1) compiles the stack's evals to the same `Acceptance`
schema A1 scores; the dock's existing loop/schedule/evals controls gained one
toggle, no new popover set. The goal facet is off by default and inert without
acceptance beyond the baseline (`stackPursuesGoal`), so a no-goal stack is
byte-for-byte the old behavior — the additive/backward-compatible rule the rest of
Stack-1 follows.

## Reflection (A2) — durable learnings, and reflection that must *earn* its context

**The load-bearing decision: reflection ships off-by-default, because the
measurement that would justify turning it on could not be run — and even the
mechanism simulation says its marginal value is conditional.** A2's analog of
A1's fail-open and A3's noise-lock is *reflection that doesn't move the needle*:
irrelevant or unbounded injected learnings add tokens and no lift, and can anchor
the worker on a wrong fix. So the whole feature is gated behind
`LoopConfig::reflect_cross_run` (default `false`), and the §2 pre-registration
(`docs/research/loop-intelligence/A2-preregistration.md`, written before any
code) fixed a **15 pp** ship margin against blind retry. The three-arm harness
(`lopi-agent::reflection_harness`) is a **deterministic mechanism simulation**,
not a live LLM benchmark — and it says so in its own doc comment. Its honest
numbers at the baseline (retrieval precision 0.8): blind 45%, within-run 80%,
cross-run 80% pass-rate. Cross-run beats blind by +35 pp — but only because
within-run already does; its **marginal** value over the within-run reflection
lopi already had is **+0 pp** at baseline, **−5 pp** below it, **+10 pp** only at
perfect retrieval. The real baseline win is *speed* (1.44 vs 2.38 iters-to-pass),
not pass-rate. A simulated lift proves the *mechanism* can help when retrieval is
precise; it is **not** proof the live feature beats blind retry. That live
three-arm run needs an API-enabled environment and was not executed here, so the
disciplined default is **off**. This is a first-class documented outcome, not a
failure — the DREX ethos: a measured (here, an honestly *un*measured-live) result
is a real result.

**Extend, don't rebuild — the within-run routing already existed.** A1's
`EvalOutcome.critique` already routes into the next attempt's `constraints`
(`eval_runner.rs`), the verifier already routes `fix_hints`, and adaptive-retry
already frames `last_error` via `SelfPromptStrategy`. A2 *reuses* those seams:
the same critique that routes within a run is distilled into a durable learning
across runs. No new reflection loop was built.

**Capture is rollback-safe by construction.** The learning is written **before**
A3's rollback discards the attempt — at both reject sites (`eval_runner.rs`
before `finalize.rs`'s `hard_rollback`; `run_loop.rs` before
`abort_and_mark_retrying`). It lands in SQLite, which git rollback never touches,
so a gain-gate-rejected attempt still yields its lesson (you learned what does
*not* work). The `learnings` table has **no score gate** — deliberately, because
the silent-0.6 gate on `lessons` (flagged in `A2.md`) would drop exactly the
failure lessons A2 needs to keep, and dropping them silently violates CLAUDE.md's
"no silent failures".

**Retrieval is bounded and relevant, because §2 punishes the alternative.**
`find_relevant_learnings` filters on goal-keyword Jaccard ≥ 0.3 (reusing pattern
mining's fingerprint so "similar" means one thing repo-wide), dedups on critique,
and the runner injects a **hard cap of 3**. Unbounded/irrelevant injection is the
exact failure mode the precision sweep shows turning cross-run's marginal value
negative — so the cap and the relevance filter are load-bearing, not decoration.

**Reflection informs; it does not override the gate.** Capture and injection
touch only the planning prompt and memory — never scoring, never
`lopi-core::gain`. A reflected-but-worse attempt is still rejected by A3, and
every A3 gain-gate test still passes. A2 gives the loop more to *gain* from; it
does not change what counts as a gain.

**How to apply:** turning `reflect_cross_run` on by default is a one-way trust
decision that requires the *live* three-arm numbers to clear the 15 pp margin —
not the simulation's. The harness is the regression guard that makes re-running
that comparison cheap; run it live before flipping the default.

## Progress-Gating (A3) — the gain gate that refuses to lock noise

**The load-bearing decision: the gain rule is objective-primary, and the judge
can only confirm, never create, a gain.** A3's analog of A1's fail-open hole is
a gate that *locks noise* — a single run that edges above "best" on a noisy
signal is not a gain, and ratcheting on it is exactly the rigor failure lopi
exists to avoid. So the rule (`lopi-core::gain::GainRule`) decides on the
**objective** sub-score (the deterministic execution-ok / shell / suite tiers,
via `GainSample::from_outcome`), and treats the **judge** score as confirmatory:
it can veto an objective gain the judge flatly contradicts (`judge_veto_band`
0.20) but a judge-only "improvement" within judge noise never locks. Margins are
pre-registered and written down: objective `margin` 0.01, `judge_margin` 0.10
(wider, judge is noisier). The §2 kill-test feeds four score *sequences*
(monotonic climb, within-noise wiggle, real regression, judge-noise-on-flat) and
proves only the genuine climb locks. This ran *first*, before any wiring.

**Reuse over rebuild (the A1 seams paid off).** A3 reads A1's `EvalOutcome`
score and the finalize rollback verbatim — a non-gaining iteration is rejected
by the *existing* per-attempt rollback path, not a new one. The prior
epsilon-improvement stall detector (`update_no_progress_streak`) is *replaced*
by `ProgressGate` observing a `GainSample` per iteration, so there is exactly
one no-progress mechanism, not two.

**Stop reasons are specific, with a settled precedence.** `StopReason` is
`goal_met` / `budget` / `no_progress` / `max_iterations` — never a generic
"stopped" — and precedence is `goal_met > budget > no_progress > max_iterations`
(a met goal is success however much budget was spent; a hard resource cap
outranks the softer stall heuristic; the iteration cap is the last-resort
backstop). Reasons persist via the structured-string-in-`reason` convention
`TurnLimitExceeded`/`NoProgressStall` already established.

**Budget is real before it's shown.** Token usage is metered at the one point
tokens are observed — the streamed `TokenUsage` events (`runner::stream`) — into
`AgentRunner::tokens_used`, and the loop hard-stops on exceed. Only *after* that
enforcement existed was the UI `budget N` badge un-hidden, and it renders only
for a preset that maps to a real cap (`budgetToTokens('200k') → 200_000`), never
for the inherit/unlimited presets — the exact honesty rule the badge was pulled
for in backend-1. Per-task `Task.budget_tokens` overrides the repo default,
mirroring `max_iterations`.

**The rename:** `:ratchet` → `:gain` (mechanism and preset share the word). The
legacy `:ratchet` alias still resolves to `gain` (`resolvePresetAlias`), so no
saved card or composer string breaks.

## Eval-Execution-1 (A1) — the judge becomes a tiered eval executor

**The keystone decision: A1 was promote + harden, not greenfield.** Research-1
proved the Konjo Verifier already works (24/24 kill-test, 100% adversarial
catch). So A1 did *not* build an evaluator — it reused `VerifierAgent` verbatim
as one tier behind a new interface, and spent its real surface area on the four
cross-cutting seams every later phase depends on. Getting these wrong is how
"evaluator-optimizer loops go circular" — not because the judge can't judge,
but because three subsystems disagree about what the evaluation *was*. So they
are settled once, here:

1. **One `Acceptance` schema** (`lopi-core::acceptance`) at loop *and* stack
   scope. `EvalTier` serializes to the UI's exact `base`/`test`/`judge`/`suite`
   union, so the previously-inert `EvalRef` tags are the authoring surface, not
   a second schema. B1 reuses this at stack scope with zero new code paths.
2. **One `TierEvaluator` interface** (`lopi-agent::eval`) with the judge behind
   a further-pluggable `Judge` trait. That second seam is load-bearing: it is
   what makes the fail-closed test and the 24-fixture regression suite run
   offline (inject an erroring / fixture judge) without a live API call, and it
   is where A3's stochastic re-sampling will wrap any tier uniformly.
3. **One `EvalOutcome` result** (`lopi-core::eval_outcome`) carrying `verdict` +
   scalar `score` + `per_check` + `critique` — designed for all three consumers
   now (A2 reads critique, A3 reads score, A3/B1 read verdict + trajectory) even
   though only PASS/FAIL is acted on this sprint. This is the anti-rework call.
4. **Score-history in SQLite** (`eval_outcomes` + `score_trajectory`). The raw
   score rows already existed but no query surfaced the trajectory; A3's
   ratchet/no-progress and B1's stack termination need a durable, queryable one.

**The fail-closed decision, made explicit and defaulted safe.** A gate that
passes when it errors is the one thing an evaluator can't do. `Verdict::Error`
is a first-class not-passing state, aggregation gives `Error` precedence over
`Fail`, and the verifier's old `Err(e) => return true` (proceed-on-error) is
now `return self.handle_verifier_error(...)` which records an ERROR verdict and
blocks. Fail-closed is the default; `Task.verifier_fail_open` is the deliberate
operator override, not a silent fallback. The decision function
(`verifier_error_proceeds`) is a pure, unit-pinned seam so the guarantee can't
regress unnoticed.

**The objective-to-deterministic routing rule.** The `TieredEvaluator` runs
checks cheapest-tier-first and short-circuits on the first *required* failure —
so anything the execution-ok/shell floor can settle never spends a judge call
(the regression suite asserts `judge_call_count == 0` for every objectively-
visible failure). Objective criteria route to a deterministic tier / `MetricGate`
because they're cheaper *and* un-gameable; the judge is reserved for genuine
judgment. This is also the mitigation for the one thing A1 structurally can't
fix: **input-completeness**. The judge catches only gaming visible in its
inputs. A1 passes the full diff into `EvalContext` (the executor is no longer
the truncation point) and fails a metric gate closed when its reading is
missing, but the honest ceiling remains — so the standing rule for anyone adding
a judge eval is: put the signal in the inputs, or make the criterion objective.

## Stack-1 — stack-level controls + the purple stack control area

**The precedence rule, decided once here rather than re-litigated per
caller: `loop.field ?? stack.default.field ?? DEF.field`.** Reading the
actual code before building anything showed this rule was *already*
structurally true — `cardToTaskPayload`'s `card.config.model ?? defaults.model`
(UI-2) is exactly the `loop ?? stack.default` half of it, and has been since
UI-2 landed. What Stack-1 actually changed is *where the fallback source
lives*: `stores/stackDefaults.ts` was a single app-wide `writable`
(`stackDefaults`) shared by both panes — every pane's cards fell back to the
*same* defaults, which made "each stack carries its own default config"
(the brief's whole premise) impossible even though the resolution function
itself was already correct. Moving `StackDefaults` from a global store into
each `StackPaneState.config.defaults` was the one real change; the
resolution logic in `cardToTaskPayload` didn't need to change at all — a
table-driven test (`stack.test.ts`) proves the three-rung chain explicitly
now, using an actual `DEFAULT_STACK_DEFAULTS` baseline rather than an
arbitrary literal, so a future change to the app-wide default can't
silently invalidate what the test claims to prove.

**The second precedence rule — stack schedule/loop-count GOVERN the chain,
per-loop schedules go inert — is a pure *rendering* rule
(`perLoopScheduleGoverned`), never a mutation.** A card's own `scheduled`/
`cron` fields are untouched when the stack governs it; `StackCard.svelte`
and `StackConnector.svelte` just stop presenting that state as active
("governed by stack — won't fire on its own" instead of the cron's actual
next-run time). This was the only honest option available: mutating or
clearing a card's schedule the moment the stack starts governing would lose
the operator's prior configuration the instant they toggled the stack's own
loop-count back to `×1` — the rendering-only approach makes that reversible
for free, with zero extra state to reconcile.

**Chain guardrails (`StackGuardrails`) are `{ onFail, budget }` — no
`gate`/`until`, deliberately not a reuse of the per-loop `Guardrails` type.**
`gate`/`until` are shell commands executed *server-side*, inside one
task's own retry loop (`crates/lopi-core/src/loop_config.rs`); there is no
server-side "whole client-side stack" for a chain-wide version of either to
run against. Two options existed: (a) reuse `Guardrails` verbatim and hide
the gate/until rows in the popover at stack scope, or (b) give the stack its
own narrower type. Took (b) — a type that can't even express `gate`/`until`
at chain scope is a stronger guarantee than a type that can but is told not
to by a UI-layer conditional, and it costs nothing: `GuardrailsPopover.svelte`
already needed a `scope` prop either way (its footer stepper edits
`maxIterations` at loop scope or the chain `loopCount` at stack scope), so
the type split rides along the same seam for free. `onFail` is the one
field WIRED at chain scope too, into `stores/stackRun.ts`'s new chain-level
on-fail (see below) — a real, observable client behavior, just re-scoped
from "how one task retries" to "what the chain does when a card fails".
`budget` stays exactly as unenforced/hidden as the per-loop decision
(Backend-1's Phase 0 escalation) already established — no new honesty gap
introduced, none closed either.

**Chain loop (`loopCount` ×N/∞) and chain on-fail extend `stackRun.ts`'s
existing `advance()` loop rather than wrapping it in an outer retry.** The
alternative — call `runStack`'s inner logic N times from a new outer
function — would have duplicated the pause/drain-checking-every-iteration
property that already makes an *infinite* per-card wait safe (Backend-1's
own reasoning for why the sequencer never needs a numeric bound on
`max_iterations: 0`). Instead, `state.cursor >= state.order.length` now
branches on "start repetition N+1" vs "finish for good" inside the same
`for (;;)` loop that already re-reads `runs`/`panes` fresh every iteration —
an infinite chain (`loopTarget: 0`) is exactly as pause/drain-safe as an
infinite single loop already was, for free, because it's the same loop.
`onFail`'s three values needed a real interpretation at chain scope since
their per-loop meaning (retry-pacing within one task) doesn't transfer
directly: `stop` keeps the pre-Stack-1 hardcoded "halt everything"
behavior as the explicit default (a one-way compatibility door — nothing
that depended on that hardcoded behavior breaks); `continue` skips the
failed card and presses on within the same pass; `backoff` ends the
current pass early (skips its remaining cards) but still attempts the next
repetition — a failed pass doesn't necessarily kill the whole ×N chain,
only itself. All three still leave `hadFailure: true` on the run state, so
a chain that "pressed on" past a failure still reports `phase: 'error'`
overall rather than a misleadingly clean `'done'`.

**Whole-chain scheduling is STUBBED, not wired, confirmed by reading
`scheduleStack` before deciding.** Backend-1's own `scheduleStack` can only
ever attach one cron to one card server-side — `ScheduleBody.goal: String`
has no multi-goal-pipeline concept, a gap Backend-1's own ledger entry
already flagged as needing a real backend change
(`ScheduleSpec.goal: String` → `Vec<String>`) to close. Building a
chain-wide schedule toggle that silently degraded to "schedule the bottom
card only" (reusing `scheduleStack` under the hood) would have been exactly
the "inert control that looks enforced" the brief rules out — worse, it
would have looked *more* enforced than the honest per-loop "Schedule stack"
run-menu item, which at least reports its `skippedCardIds` back. So the
dock's schedule popover stores `config.scheduled`/`config.cron` and renders
an explicit "not yet enforced — no whole-chain cron exists server-side yet"
hint whenever the toggle is on, and nothing in this sprint calls
`scheduleStack`/`createSchedule` from it.

**`options.ts` is a new module, not a refactor avoided.** Adding
`stores/stackDefaults.ts` as a real (not type-only) import into
`stores/stack.ts` — needed for `defaultStackConfig()`'s factory call, not
just the `StackDefaults` type — surfaced a transitive dependency nobody had
hit before: `stackDefaults.ts` imported `MODEL_OPTIONS` from `controls.ts`,
which imports `$app/environment` for its `launchControls` localStorage
persistence. That import is invisible in the browser (Vite resolves the
virtual module fine) but fatal under this repo's plain-`tsx` test
convention the moment anything in the `stack.ts`/`stack.test.ts` chain
needs it — exactly the failure mode `stackRun.ts`'s own doc comment already
named and designed around (`statusSource` as a parameter instead of an
`./agents` import) for a different edge of the same problem. Splitting the
pure option catalogs (`Option`/`MODEL_OPTIONS`/`EFFORT_OPTIONS`/
`PRIORITY_OPTIONS`/`labelFor`) out of `controls.ts` into `options.ts`, with
`controls.ts` re-exporting them verbatim, fixes it at zero cost to any
existing call site — nothing outside `stores/` even knows the split
happened.

**How to apply:** any future module that `stores/stack.ts` (or anything
`stack.test.ts` transitively imports) needs a *runtime* dependency on —
not just a type — must be checked for its own transitive imports first;
`import type` alone doesn't save you the moment a real value/factory
function is needed. Any future "stack-level facet mirroring a per-loop
one" should default to generalizing the existing popover's props (value +
callback, plus a `scope` prop if the fields genuinely differ) before
reaching for a forked component — `SchedulePopover`/`GuardrailsPopover`/
`EvalsPopover` all took this path this sprint; only `ConfigDrawer.svelte`
didn't, because its whole job (per-loop *override* of something) doesn't
exist at the stack level (the stack *is* the something), so a new
`StackConfigPopover.svelte` reusing `Dropdown.svelte` directly is the
correct amount of reuse, not a gap.

## Shell-1 — Loop Stacks as default view, fully-hidden left sidebar

**Default-route change is a redirect (`+page.ts` `load()` throwing
`redirect(307, '/stacks')`), not moving Stacks' page into the root route —
and Forge, not Stacks, is what physically moved.** The brief framed this
as "redirect vs. move," but either choice requires *some* page to vacate
`/`, since a route can't simultaneously render a component and
unconditionally redirect away from itself. Forge's `+page.svelte` was a
5-line wrapper around `AgentGrid.svelte` — relocating it to `/forge` is a
zero-risk mechanical move (confirmed byte-identical via diff). Moving
Stacks' considerably larger implementation into the root route instead
would have been the higher-blast-radius option for no benefit: `/stacks`
as a URL keeps working either way, and this way `/stacks`'s own route
folder is never touched at all. Reversible: deleting the new root
`+page.ts` restores Forge as the default with a one-line change.

**Pause/drain/bump's client-side precedent from Backend-1 extends
naturally here: the sidebar's open/closed state is a single shared
`writable` (`stores/nav.ts::sidebarOpen`), not local component state
duplicated between the hamburger and the panel.** The hamburger button
lives in `+layout.svelte`'s topbar (existing chrome, existing spacing);
the panel/scrim/focus-trap lives in a new `AppSidebar.svelte`. Splitting
the toggle *control* from the toggle *target* into two components only
works cleanly with a shared store — passing a callback prop back and
forth for a single boolean would be more coupling for no benefit.

**The closed sidebar is `inert`, not just visually off-screen.** A
`transform: translateX(-100%)` alone still leaves the panel's links in
the tab order — a keyboard user tabbing through the page would land on
invisible, off-screen anchors before ever reaching the page's own content.
The `inert` HTML attribute (gated on `!$sidebarOpen`) removes the whole
panel from both tab order and pointer interaction without touching the
CSS transform, so the slide animation is untouched. Moving focus *into*
the panel on open has to `await tick()` first — `inert` is still present
in the DOM for one tick after `sidebarOpen` flips true, and focusing an
inert element is a silent no-op; without the `tick()`, keyboard users
would open the sidebar and land nowhere.

**`SIDEBAR_MODE: 'hidden' | 'rail'` lives in `stores/nav.ts`, and the rail
CSS ships in `AppSidebar.svelte` today even though nothing sets the
constant to `'rail'`.** The brief asked for this to be a one-line flip
later, not a rebuild — so the rail-mode styles (narrower width, icon-only,
centered) are written and gated behind `class:rail={SIDEBAR_MODE ===
'rail'}` now, verified to compile and typecheck clean, just never
exercised by the shipped default. This is deliberate dead-but-correct code
for a named, planned migration path, not speculative scope creep — it's
the one thing the brief explicitly asked to pre-build.

**`$lib/components/icons.ts` is a new module, not an extension of
`stacks/icons.ts`.** The brief said "extend icons.ts," but the only
`icons.ts` in the repo lives under `components/stacks/` — a
feature-scoped catalog for the loop-stack cards, never imported from
outside that folder. Importing sidebar/shell glyphs from a feature folder
(or vice versa) would be a backwards dependency for global chrome that
outlives any one feature. A handful of the new icons echo existing
`stacks/icons.ts` glyphs in shape (loop, cron, wrench, sliders) since
those already read correctly for their nav destinations — duplicated as
tiny SVG strings rather than imported, matching this codebase's existing
convention of no single universal icon registry.

## Backend-1 — task identity, execution, control signals, event routing

**There is no server-side "stack"/"plan" concept, so run-stack execution
*and* pause/drain/bump are a purely client-side TS state machine
(`stores/stackRun.ts`), not a new Rust orchestration layer.** The pre-flight
gate's own go/no-go question was whether the pool can interrupt a running
task; it can only cooperatively cancel at two checkpoints in the attempt
loop (`crates/lopi-agent/src/runner/run_loop.rs:111,242`), never mid-
subprocess. Rather than building (or faking) deeper interruption, the
sequencer submits one card's task at a time via the real `createTask`,
waits for it to reach a terminal `AgentState.status` through the app's
already-live `agents` store, and only checks pause/drain state *between*
cards. That gives exactly the brief's own definitions — "pause: halt after
current iteration completes," "drain: let current loop finish, then
stop" — for free, with zero pool/runner changes. `bumpCard` similarly never
touches the pool; it's a pure array swap (`bumpInOrder`) gated on a client-
held `cursor`, reflected into both the run's own plan and the pane's
rendered card order.

**`stores/stackRun.ts` does not import `./agents` directly — every function
that needs to observe task completion takes a `statusSource` parameter
instead.** `stores/agents.ts` pulls in `$app/environment` (a SvelteKit
virtual module unresolvable outside a Vite build), which would have made
the sequencer's own logic untestable under this repo's `tsx`-script test
convention (no Vitest/Playwright/Jest is a committed dependency — see the
UI-2 V&V audit's G5). Taking the live status store as an injected
`Readable<Map<string, {status?: string}>>` instead means `runStack`/
`resumeStack`'s call sites (Svelte components) pass in the real `agents`
store, while `stackRun.test.ts` substitutes a plain `writable(new Map())` —
same shape, zero new test-runner dependency. 26 new integration-style tests
(ordering, halt-on-failure, pause/resume, drain, bump + its illegal-
transition rejections, schedule) run this way, mocking only `fetch`.

**Execution order is bottom-of-stack (oldest) first, derived by reversing
the pane's own card array — not a separately-tracked order field.** The
composer prepends new cards to index `0` (`addCard`), so a pane's array is
newest-first; the settled mockup's own chrome ("new prompts prepend to the
top; the stack flows down to the currently-executing loop at the bottom")
confirms this is the intended reading, not an accident of the data
structure. `executionOrder(cards)` is `[...cards].reverse()` — a run's
`order`/`cursor` snapshot this once at launch (`runStack`) rather than
re-deriving it live, so a composer edit mid-run can't reshuffle a plan
already in flight.

**Run-menu intent semantics, decided once here rather than re-litigated
per caller:** *Run once* forces `max_iterations: 1` on the outgoing
`CreateTaskOptions` only — it never mutates the card's own stored
`maxIterations` (including the `0`/∞ sentinel case), so toggling back to
"Run now" later still uses whatever the card actually has configured.
*Dry run* is `dryRunStack` — pure, total, never calls `createTask`; it
resolves every card's config against pane defaults (the same resolution
`cardToTaskPayload` does) and flags an empty goal or a guardrail toggled on
with an empty command. *Schedule stack* is deliberately minimal, and the UI
says so: `ScheduleBody.goal` is a single `String` with no multi-goal
pipeline concept server-side (confirmed by reading the type, not assumed),
so `scheduleStack` attaches the given cron to only the bottom-of-stack
(first-to-run) card via the real `createSchedule`, and reports every other
card back as `skippedCardIds` rather than silently dropping them or faking
a multi-card schedule. Wiring the rest would need a real backend change
(`ScheduleSpec.goal: String` → `Vec<String>`) that's out of scope here.

**Per-card event isolation reuses the pre-existing `GET
/api/tasks/:id/stream` SSE endpoint and the frontend's pre-existing
`transcripts`/`agents` stores verbatim — no new transport.** `stream_task`
(`crates/lopi-ui/src/web/task_stream_handlers.rs`) already filters the
shared broadcast bus by `event_task_id(&ev) == target_id` for every
`AgentEvent` variant that carries one; it had no test proving isolation
under concurrency, only that it existed. Added
`task_stream_tests.rs::task_stream_isolates_concurrent_tasks_with_zero_cross_talk`:
two concurrent SSE subscriptions on the same bus, ten interleaved events
per task id, and an explicit assertion that the cross-talk count is `0` in
both directions — proof, not a log line. The frontend side needed zero new
plumbing: `StackOutput.svelte` already read `stores/transcript.ts` keyed by
`taskId`, built in UI-2 before any card ever had a real one.

**Fixed a pre-existing empty-repo bug in `api.ts::createTask`, found by
actually running a stack against a live backend, not just by unit tests.**
`CreateTaskRequest.repo` is `Option<String>` and falls back to the server's
own configured repo path when the key is *absent* — but `createTask` always
sent `repo` in the JSON body, so a blank default (`""`, this repo's own
"auto" sentinel, and the Tasks page's blank-by-default field) deserialized
to `Some("")`, which the runner then tried to `git2::Repository::open("")`
and failed outright, 100% of the time, for every stack until a user
manually picked a non-default repo. Fixed by omitting the `repo` key
entirely when it's falsy (`...(repo ? { repo } : {})`); this is shared code
so it also fixes the same latent bug on the pre-existing Tasks page for
free. Caught only because Phase 5's manual verification pointed a real
`lopi sail` at a disposable scratch repo and clicked "Run now" for real —
the unit/integration test suites, which mock `createTask`'s transport
layer, could not have surfaced this.

**Phase 0 (CI gate integrity) landed inside this same sprint rather than as
a separate PR, since the brief made it blocking-but-not-necessarily-
separate.** Of the original 11 `continue-on-error: true` steps in
`konjo-gate.yml`, 2 were removed outright (the Wall-3 "fail if BLOCKER"
step, and the `konjo-gate` summary job's `needs:` list, which silently
excluded `mutation`/`review` from the merge-blocking check entirely); the
remaining 9 each got a one-line comment naming exactly why they're still
soft and a `TODO` for when to flip them, rather than a silent blanket
policy. `StackConnector`'s budget badge (visually reads as enforced;
nothing enforces it) was hidden per the V&V audit's own escalation, not
restyled — restyling would still imply *some* real state.

## UI-2 — Card controls, popovers, config drawer, live output, pane chrome

**Config lives in an inline drawer of five live `Dropdown.svelte` selectors,
not read-only chips that open a secondary menu.** The settled mockup shows
`.cfgchip` elements — static text that opens a `dmenu` on click — but the
UI-2 brief's own settled spec (§4) explicitly names the drawer as "five
selectors... built on `Dropdown.svelte`." Per this repo's standing rule that
the brief wins on data/wiring while the mockup wins on appearance, the
drawer renders actual interactive selects (`dense` mode, chip-sized via
flex-wrap) rather than reproducing the mockup's click-to-open-secondary-menu
interaction. Consequence: the drawer's chips are always "live," never
requiring an extra click to discover they're editable — a strict UX
improvement over the mockup, not a regression, but a deliberate
appearance/behavior split worth flagging so a future pixel-diff pass doesn't
"fix" it back to static chips.

**The iteration pill and the guardrails max-iter stepper edit the literal
same `StackCard.maxIterations` field — there is no separate "loop count" vs.
"max iterations" concept.** UI-1's `StackCard.loopN` (set by the composer's
`xN` grammar) was renamed/folded into `maxIterations`, matching the backend
field name (`LoopConfig.max_iterations`) exactly rather than keeping a
UI-only synonym. `stepMaxIterations` floors at 2 and wraps to the infinite
sentinel (`0`) below that, and un-wraps back to the floor (never `1`) when
incrementing from infinite — this is a deliberate cleanup of the settled
mockup's own stepper math, which (traced through literally) can decrement
from 2 to 1 and clamp back to 2, never actually reaching `0` via `-1` steps
in practice. The brief's prose ("floor 2; below floor ⇒ ∞") describes the
*intended* behavior more clearly than the mockup's JS achieves it, so the
prose was implemented, not the literal mockup logic.

**`stores/stack.ts` grew a pane-keyed layer on top of the existing pure
single-array ops, rather than rewriting those ops to take a pane key
directly.** UI-1 built `addCard`/`removeCard`/`duplicateCard`/`reorderCard`/
`insertCardAt` as pure `StackCard[] → StackCard[]` functions with their own
unit tests; UI-2 needed two independent panes (`stack.insert(stackKey,
index, loop)` from the pre-flight gate). Rather than threading a `stackKey`
parameter through every existing op (which would have meant re-testing
already-correct logic), `applyToPaneCards(state, key, fn)` is the one new
primitive — it dispatches any pure card-list transform to the named pane
and leaves every other pane's array reference untouched (verified by
identity-equality in the test, not just value-equality, since Svelte's
`{#each}` keying benefits from the other pane's reference staying stable).
`insertIntoPane`/`reorderInPaneRelative`/etc. are thin wrappers composing
`applyToPaneCards` with the pre-existing ops.

**`StackOutput` reuses `stores/transcript.ts`'s existing per-`task_id`
block feed verbatim, rather than inventing a new live-output data model.**
The UI-2 brief flagged per-card `AgentEvent` routing as unbuilt (`AgentEvent`
keys on `task_id`, no card/stack id exists). Investigating the actual
frontend surface (not just the backend event shape) found that
`stores/transcript.ts` — built for the Forge's transcript pane — already
folds the flat `AgentEvent` stream into per-`task_id` `TranscriptBlock[]`
(`thinking`/`tool_call`/`status`/`assistant_text`). Since a stack card *is*
a task the moment it runs (one `task_id`, no fan-out), this store already
answers "what happened for this specific run" with zero new plumbing —
`StackOutput` maps those four block kinds onto the mockup's
thinking/tools/actions/output categories (`status` → `actions`, the one
non-obvious mapping) and takes a `taskId` prop rather than owning any event
subscription itself. The real gap the brief identified is narrower than it
first reads: it's not "no per-task output feed exists," it's "no card is
ever assigned a real `taskId`" — which is squarely the pause/drain/bump
execution-signal gap, not a data-modeling gap. `StackOutput` needs no
changes when that gap closes.

**`budget` (auto/200k/none) is treated as client-only, same as
`branch`/`autonomy` — despite the brief's own WIRED/CLIENT-ONLY table not
mentioning it either way.** Grepping `CreateTaskRequest`/`Task`/`LoopConfig`
turned up no budget field of any shape (not even the scalar
`budget_tokens: u64` UI_PLAN.md's Backend Bindings table describes as
"partial" — that field exists on `LoopConfig`, repo-level, but nothing
threads a per-task budget preset onto `CreateTaskRequest`). Per the brief's
"if you find a field is actually wired, prefer wiring it" instruction (which
implies the reverse for a field found *not* wired), `budget` gets the same
`// TODO(backend)` treatment as the two fields the brief already named.

**The `/stacks` composer keeps a "pane defaults" panel above the two panes,
which the settled mockup doesn't show at all.** The mockup hardcodes a
single global `DEF` object (`{model, effort, repo, branch, autonomy}`) with
no editor UI — there was never a control to change it in the interactive
prototype. Since the config drawer's entire "override" concept needs
something concrete to override *away from*, and UI-1 had already built a
working defaults panel (`Panel` + five `Dropdown`s bound to
`stores/stackDefaults.ts`), UI-2 kept and extended it (added the missing
`branch` field) rather than deleting working, tested chrome to match a
mockup that simply never modeled where defaults come from.

## Guardrails — gate / until / on_fail

**`gate` = precondition, `until` = exit-condition — not the same shape,
modeled as two separate `Option<String>` fields, not one.** `gate` blocks
the loop from ever starting; `until` is checked after every iteration and
can end the loop early as a success. Conflating them into one field (as
earlier "Limits" exploration docs did) would have made "runs once before"
and "runs every iteration, can end the loop" indistinguishable without a
second flag anyway — two named fields is the simpler contract.

**`OnFail::Stop` had to become a no-op, not a "halt after one failure."**
The brief's own wording ("Stop → halt the loop") reads like Stop should cut
the retry loop short on the first failure. That's incompatible with the
hard kill-test-#1 requirement — every config written before this sprint has
no `on_fail` field, `#[serde(default)]` fills `OnFail::Stop`, and those
configs must behave *exactly* as they did before, i.e. keep retrying with
backoff until `max_retries`/`max_iterations` is exhausted. Since `OnFail` is
a plain enum (not `Option<OnFail>`) on `LoopConfig`, there is no way to
distinguish "user explicitly chose Stop" from "field was absent" — so
`Stop`'s runtime effect **must** be the pre-existing behavior verbatim.
Consequence: `Stop` and `Backoff` are currently behaviorally identical
(both call `backoff_secs(attempt, 500)`); `Backoff` exists as an explicit,
named choice for the same wait. `Continue` is the one real behavioral
difference this sprint adds — it skips the pause and retries immediately.
Flagging this rather than silently resolving it: if a future sprint wants
`Stop` to mean "halt after one failure," `Task.on_fail` needs to become
`Option<OnFail>` (mirroring `gate`/`until`/`max_iterations`) so "unset"
and "explicitly Stop" are distinguishable again.

**`until` is checked once per iteration, at the same point `score.passed()`
already was — not re-checked after the in-place fix retry.** `run_loop.rs`'s
existing flow computes a `score`, and on failure attempts one in-place fix
with its own re-score. Extending `until` to both checkpoints would double
the shell-exec cost per iteration for a condition that, by construction,
either passed already (loop already exited) or didn't (nothing changed
about the *first* score's shell check by fixing lint/test errors in a
second pass). Kept to one checkpoint per the brief's "keep it minimal"
instruction; the effective condition becomes
`score.passed() || until_satisfied`, changing nothing when `until` is
`None` (the existing shell call is skipped entirely — `check_until`
short-circuits on `None` before spawning anything).

**Shell execution: `sh -c`, not a fixed-binary invocation.** Every existing
shell-out in this codebase (`scorer.rs`, `worktree.rs`, `repos_handlers.rs`,
`manager.rs`) runs one fixed, known binary (`git`, `cargo`, `npm`, `gh`)
with explicit argv — none of them interpret a free-form command *string*.
`gate`/`until` are user-supplied strings (`"cargo test"`, `"./kill_test.sh"`,
`"exit 1"`), so they need shell interpretation to support that grammar at
all. `run_guard_command` (`lopi_core::loop_config`) wraps `sh -c <cmd>` —
the minimal necessary deviation — while keeping the *rest* of the
invocation (`tokio::process::Command`, `.current_dir(repo)`, `.status()`,
check `.success()`) identical to the codebase's existing pattern. Lives in
`lopi-core` (not `lopi-agent`) since it's a pure, dependency-light
primitive any future consumer (a stack-wide dry-run preview, say) can reuse
without pulling in the whole agent runner.

**`Backoff`'s reuse is proven by a property test, not exact equality.**
`backoff_secs` includes `rand::random()` jitter, so two calls with
identical arguments never produce identical `Duration`s — asserting
`on_fail_wait(Backoff, n) == backoff_secs(n, 500)` directly is not
possible. Instead, `guardrails.rs`'s test samples many calls and asserts
every wait falls inside `backoff_secs`'s own `[0, ceiling]` band for that
attempt, and that at least one sample is nonzero — a hardcoded *second*
delay constant would either never vary or exceed the ceiling, so the
property still catches drift without needing determinism.

## UI-1 — Static loop-stack + selector row

**`/stacks` stood up as a new route, `/loop` untouched.** Per `UI_PLAN.md`
§6: the existing `/loop` page is a read-mostly *loop-as-code cockpit*
(health telemetry, effective `.lopi/loop.toml`, the autonomy ladder,
self-prompt strategy, schedules) — a genuinely different surface from an
interactive stack-of-prompts composer. Building the new UI in place would
have destroyed that content as a side effect. Two routes coexist; folding
one into the other (as a tab, or renaming `/loop` → `/loop/config`) is left
for later, once the new UI has parity on what people actually use from the
cockpit.

**Stack store shape: pure ops + a thin `writable` wrapper, no persistence.**
`stores/stack.ts` mirrors the `layout-core.ts`/`layout.ts` split — `addCard`/
`removeCard`/`duplicateCard`/`reorderCard`/`insertCardAt` are plain
`StackCard[] → StackCard[]` functions (directly unit-testable, no Svelte),
wrapped by a `writable<StackCard[]>` for the UI. No `localStorage`: unlike
`launchControls`/`layout.ts`, a stack is a to-be-run queue the operator is
actively composing, and no server-side stack concept exists yet to reconcile
against on reload (per `UI_PLAN.md`'s Gap Map) — silently caching a stale
queue across reloads would be worse than starting empty. Revisit once stack
persistence (client or server) is actually built.

**Eval suites are client-side static config this slice, by design, not by
accident.** `PRESET_CATALOG` in `stores/stack.ts` hardcodes each preset's eval
list verbatim from the task brief. No `EvalDef`/`EvalSuite` backend concept
exists (`UI_PLAN.md`'s Gap Map) — evals shown on a card are decorative counts
and names only; nothing here executes, scores, or persists an eval. UI-2's
evals popover will need real backend fields before "toggle an eval" means
anything; this slice deliberately stops at "look right."

**Autonomy selector uses the real `AutonomyLevel` semantics, not the
mockup's mismatched copy.** `UI_PLAN.md` flagged that `lopi-creation-flow.html`'s
L1–L4 "leash" labels (writer/director/advisor/autonomous) don't map to the
actual backend enum (`ReportOnly`/`DraftPr`/`VerifiedPr`/`AutoMerge`).
Rather than ship UI that reads correctly but lies about what the levels
actually do, `stores/stackDefaults.ts`'s `AUTONOMY_OPTIONS` reuses
`loop/+page.svelte`'s existing `ladderHint()` wording for each tag — the two
autonomy surfaces in the app now agree. It is still an in-memory default,
unbound to any backend field (`CreateTaskRequest` doesn't expose autonomy
yet); it just isn't wearing a costume that misdescribes L3/L4.

**Repo dropdown is new frontend work, not a relabel.** `GET /api/repos`
existed and worked, but no frontend consumer did (`UI_PLAN.md`'s Reuse Map).
Added `listRepos()` to `api.ts` and wired it into the stacks selector row
with a graceful fallback to a single "auto" option if the fetch fails (e.g.
a static preview with no backend) — matches the composer's overall
"nothing here is a hard backend dependency" posture.

**Card-bar buttons (loop pill, cron, shield, evals, duplicate, drag,
delete) render disabled this slice, on purpose.** The brief's pre-flight
kill-test requires the pure array ops (`duplicateCard`/`reorderCard`/
`insertCardAt`) to exist and be tested now, but wiring them to on-card
buttons is explicitly UI-2 scope (`NEXT.md`) — those buttons would need
live drag interaction, the guardrails/evals popovers, and cron popover
plumbing this slice doesn't build. Shipping them as visible-but-disabled
(rather than hidden) keeps the card's final layout stable across UI-1→UI-2,
so UI-2 wires behavior into existing chrome instead of reflowing the card.

## Git hygiene — fixed the committed DRY violations (`dry_check.py`: 794 → 12)

**Starting state confirmed, then a delta reported before fixing:** the last
"Gate verification" note named four offenders (the `api_plan.rs`/
`stability/mod.rs` Task-builder pair, the `lopi-git` worktree/rebase test
overlap, `dlq_handlers.rs`, `task_stream_handlers.rs`). Running `dry_check.py`
fresh found **46 file pairs / 794 raw window-matches** — the four named
offenders were all still present, but so were ~40 more pairs never
individually named (same-file internal repetition in several crates, and a
large `lopi-ui` test-boilerplate cluster). Fixed in priority order below;
final state is **12 raw matches across 4 file pairs (3 distinct justified
reasons — `dag.rs` accounts for two of the four pairs under the same sqlx-
boilerplate reasoning)**, each a documented residual — not silently accepted,
each has a concrete structural reason `dry_check.py` cannot see.

**De-duplicated (real fixes, one source of truth each):**
- `api_plan.rs`/`stability/mod.rs` test-builder pair → `lopi-agent::test_support::make_test_task`, itself simplified to delegate to `Task::new` instead of re-listing all 20 fields.
- `api_plan.rs::build_user_prompt` / `stability::build_stability_prompt` (a *second*, previously-unnamed duplicate between the same two files — real production prompt-building logic, not test code) → shared `lopi-agent::prompt::build_user_prompt`; `build_stability_prompt` is now a one-line delegate. The original author's comment ("kept standalone to avoid coupling to the private `api_plan` module") is resolved by the new module living at the crate root, not inside `api_plan`.
- `dlq_handlers.rs`, `task_stream_handlers.rs` (self-duplicate 404/500 response bodies, and a repeated log-row→JSON mapping) → `dlq_not_found`/`dlq_internal_error`, `log_rows_to_json`/`logs_internal_error`.
- `crates/lopi-agent/src/runner/run_loop.rs` (self-duplicate rollback+checkout, 7×, and rollback+status(Retrying), 3×) → `abort_attempt` free fn + `AgentRunner::abort_and_mark_retrying` method.
- `crates/lopi-context/src/window.rs` (self-duplicate auto-evict-toward-threshold block in `push`/`push_tool_pair`) → `ContextWindow::evict_toward_threshold`.
- `crates/lopi-core/src/config_tests.rs` (self-duplicate temp-TOML-file test setup) → `write_temp_lopi_toml` + `temp_config_with_report_channel`.
- `crates/lopi-git/src/worktree.rs` (`run_git`/`run_git_stdout` self-duplicate) → `run_git` now delegates to `run_git_stdout`.
- `crates/lopi-orchestrator/src/scheduler.rs` (self-duplicate `ScheduleEntry` test fixtures, 3 pairs) → `make_entry` helper.
- `crates/lopi-remote/src/whatsapp.rs` ↔ `crates/lopi-ui/src/web/api_middleware.rs` (byte-identical `constant_time_eq` — security-relevant, genuinely dangerous to drift) → `lopi_core::security::constant_time_eq`, one implementation for both crates.
- `crates/lopi-remote/src/whatsapp.rs`, `crates/lopi-webhook/src/github.rs` (self-duplicate axum test-request boilerplate) → `post_webhook` helper in each crate's own test module (kept separate — see residual note below on why these two crates can't share one).
- `crates/lopi-spec/src/lib.rs` (self-duplicate extractor-dispatch-and-tag-error-handling for `.rs`/`.py` branches) → `scan_with` helper.
- `crates/lopi-spec/src/{rust_extractor.rs,python_extractor.rs}` (byte-identical `name_to_description`) → moved to the crate root, both modules import it.
- `crates/lopi-toon/src/lib.rs` (byte-identical "spec example" JSON fixture in two tests) → `spec_example()` helper.
- `crates/lopi-toon/src/encode/helpers.rs` (`encode_scalar_value`/`encode_cell` identical but for one bool) → shared `encode_scalar_common(v, delim, in_cell)`.
- `crates/lopi-toon/src/decode/parser.rs` (self-duplicate "parse remaining object fields at depth+1" loop in two `parse_array_body` branches) → `Parser::parse_remaining_object_fields`.
- `crates/lopi-ui/src/web/{tests.rs,tests_extended.rs}` — by far the largest cluster (**593 of the original 794 raw matches**): both files are `include!()`-ed into one module, so a single `get_req`/`send_req`/`test_app_with_store` helper trio (added to `tests.rs`) resolved the entire cross-file and self-file axum test-request boilerplate at once. Two Python scripts did the mechanical call-site rewrite (regex-matched the exact `Request::builder()...oneshot()...unwrap()` shape); every rewritten test was individually re-run green before and after.
- `crates/lopi-context/tests/tool_pair_atomicity.rs` (self-duplicate `push_tool_pair(make_msg(...), make_msg(...))` fixture, 4×) → `push_pair` helper.
- `crates/lopi-context/tests/{phase_eviction.rs,conclusion_preservation.rs,budget_lifo.rs,tool_pair_atomicity.rs}` (four different-arity `TaggedMessage` builders, all re-listing the same 9-field literal) → `tests/common/mod.rs` (the standard Rust idiom for code shared across integration-test binaries), each file's own narrower helper now delegates to `common::make_msg` with its fixed defaults.
- `web/src/lib/*.test.ts` (9 files: `api`, `badges`, `excitement`, `events`, `markdown`, `agentReducer`, `transcript`, `layout-core`, `session-groups`) all hand-rolled the same pass/fail-counter + `eq`/`ok` assertion harness (two variants: `Object.is` and `JSON.stringify` comparison) → `web/src/lib/test-harness.ts`, exporting a `record` primitive plus `eq`/`eqIs`/`ok`/`summary`/`namedSummary` built on it. Files needing the `Object.is` variant import `eqIs as eq` (aliased, so call sites didn't need touching); files with a custom approx-comparator (`excitement.test.ts`'s `close()`) call the new `record` primitive directly instead of mutating raw counters (which import bindings can't do). Every one of the 9 files was individually re-run via `npx tsx` before and after, plus a full `npm run check` — all pass, 0 TS errors.

**Left as documented residuals (4 file pairs, 12 raw matches, 3 distinct reasons) — not fixed, with why:**
- **`crates/lopi-git/src/worktree/tests.rs` ↔ `crates/lopi-git/tests/rebase.rs`** (identical `fn git(repo, args)` test helper). Structural, not fixable without a worse trade: `worktree/tests.rs` is a `#[cfg(test)] mod` compiled *inside* the library crate (`use super::*` gives it access to private items like `worktree_slug`/`add_args`), while `tests/rebase.rs` is a separate integration-test binary with only the crate's public API. Rust has no shared-code mechanism between those two contexts short of making the helper `pub` (pollutes the public API for a test-only convenience) or adding a new dev-only shared crate (out of scope — "no new dependency").
- **`crates/lopi-memory/src/store/{dag.rs,q_routing.rs,verifier.rs}`** (identical `.fetch_all(&self.read_pool).await?; Ok(rows) }` tail + adjacent `#[cfg(test)] mod tests` preamble). Each function queries a different table into a different row type (`DagNodeRow`, `RoutingQValueRow`, `VerifierVerdictRow`); the only thing matching is how any `sqlx` `fetch_all` call necessarily ends. No real abstraction exists here without genericizing over the query and row type, which sqlx itself already is the abstraction for.
- **`crates/lopi-remote/src/whatsapp.rs` ↔ `crates/lopi-webhook/src/github.rs`** (the `#[cfg(test)] #[allow(...)] mod tests { use super::*; use axum::{ ... }` preamble). Pure boilerplate common to any axum-handler test module in this codebase — not meaningfully shared logic, and coupling two unrelated crates' test preambles together to satisfy a textual match would be exactly the "contort real code" the brief warned against.

`dry_check.py` was NOT run with any scoped ignore/allowlist (the tool has none — checked its full source: no per-pair suppression mechanism exists, only `--staged-only`/`--changed-only`/`--warn-only` mode flags). The residual above is accepted at the repo level, documented here per the brief's fallback option.

**Decision:** dropped the local worktree-isolation stash created before this
session's sync with `origin/main`. `origin/main`'s own `WorktreeManager`
(RAII `Worktree`, slug-based naming, `WT_META_LOCK`, `gc`/`list`/`prune`,
`pool/mod.rs` + `pool/worktree.rs` split) is the kept implementation —
confirmed, not assumed, more capable than the stashed version, which had no
equivalent for `gc`/orphan-detection and split its capability across a
single-file `pool.rs`.

**Redundancy proof (21 of 25 stash files):** every stash file mapped to an
`origin/main` file/mechanism implementing the same capability — see the
full file-by-file table produced during this pass. Two design-surface
differences noted but not blocking: (1) main's `LoopConfig.isolation:
IsolationMode` is a simpler enum toggle vs. the stash's `WorktreeConfig`
(configurable root/base-ref/cleanup-age) — same core capability, less
configurable; (2) `add_detached` branches from local `HEAD` unconditionally,
where the stash had a `BaseRefPolicy::RemoteHead` default — a real behavioral
difference, judged non-blocking since the overall architecture choice
(main's `WorktreeManager`) was already decided, not something this pass
re-opened.

**What was NOT superseded (2 files, different severity):**
- `crates/lopi-ui/src/web/worktree_handlers.rs` (`GET /api/worktrees`) — no
  web-exposed worktree listing exists anywhere on `main` today; CLI parity
  exists (`src/worktree_commands.rs::{list,gc}`). Minor, accepted as a gap
  rather than salvaged, since the underlying capability is reachable via CLI.
- **`docs/ui/{lopi-loop-stacks-3-output,lopi-scope-and-test-plan,lopi-selectors-panes}.html`**
  — the actual design mockup source material `UI_PLAN.md` (already merged)
  was written against. Unrelated to worktree isolation; only present in this
  stash because the original `git stash push` swept up everything uncommitted
  at the time. **Extracted before the drop** (`git checkout stash@{0} --
  docs/ui/`) and left staged, uncommitted, for separate review — not lost.

**Honest DRY-gate outcome — do not overstate:** the stash was never applied
to the working tree, so it could not have been contributing to
`dry_check.py`'s failures in the first place. Proven directly: ran the check
before the drop (stash present but unapplied) and after (stash gone) — the
failing-file set is byte-identical both times (`diff` exit 0). **Dropping
the stash changed nothing about the DRY gate.** The gate still fails on
committed code — the same pre-existing set recorded in the prior "Gate
verification" entry (`api_plan.rs`/`stability/mod.rs` test-builder pair,
`lopi-git` worktree/rebase test overlap, `dlq_handlers.rs`,
`task_stream_handlers.rs`, and others) — which remains its own, separate
cleanup, not addressed by this pass. `cargo test --workspace` (704
passed/1 failed, the same pre-existing unseeded `qlearned_favours_highest_
reward_member` flake) and `cargo clippy --workspace -- -D warnings` (clean)
confirm dropping the stash broke nothing, as expected since it was never
applied.

## Sprint 5 — Expose Loop Fields on `CreateTaskRequest` (`crates/lopi-core/src/task.rs`, `crates/lopi-ui/src/web/{types.rs,handlers.rs}`, `crates/lopi-agent/src/claude.rs`, `crates/lopi-orchestrator/src/pool/run_loop.rs`)

**Gate verification (evidence, not assertion) — merge-prep pass:**

- **`dry_check.py`** fails on both this branch and clean `origin/main`. Proof:
  stashed the branch's tracked changes (working tree then byte-identical to
  `origin/main`, confirmed via `git diff origin/main --quiet`), ran the
  checker, restored the stash, ran it again. File-level failing set: identical
  (`diff` exit 0). Pair-level failing set (`fileA ↔ fileB`, line numbers
  stripped so this branch's line-shifts don't mask a real comparison): **46
  pairs on origin/main, 46 on the branch, `comm -13`/`comm -23` both empty —
  zero pairs added, zero removed.** This branch adds no new duplicate.
  Confirmed separately: exactly one definition each of `ReportChannel::parse`
  (`report.rs:43`), `select_model` (`claude.rs:45`), `resolve_verifier`
  (`verifier.rs:34`) — every call site reuses the one definition.
- **`npm run check`** originally reported 7 errors, all in `markdown.ts`/
  `highlight.ts`/`parser.test.ts` (never touched by this branch) importing
  `marked`/`dompurify`, which were listed in `package.json` but never
  installed in this checkout. After `npm install` (53 packages): **0 errors**,
  2 pre-existing warnings in files this branch never touched
  (`HelpOverlay.svelte` a11y, `fleet/+page.svelte` CSS). `api.ts` — this
  branch's only frontend change — was clean before and after.
- **`cargo test --workspace`** (nextest unavailable in this environment,
  same as the prior session — used plain `cargo test`): 704 passed, 1 failed.
  The failure, `constellation::tests::qlearned_favours_highest_reward_member`,
  is an **unseeded statistical test** (200 ε-greedy Q-learning trials against
  a `b_count > 120` threshold, no fixed RNG seed — a pre-existing violation of
  this repo's own "seed everything stochastic" rule). Confirmed flaky by
  direct measurement: 5 isolated reruns, 1 failure (20%), with zero code
  changes. Confirmed unrelated to this branch: `git diff origin/main --stat --
  crates/lopi-orchestrator/src/constellation* crates/lopi-orchestrator/src/q_router.rs`
  is empty — this branch has never touched that code. Not fixed here (out of
  this sprint's scope); flagged as its own follow-up rather than silently
  re-run until it happened to pass.
- **`clippy --workspace --all-targets -D warnings`**: clean. **`RUSTDOCFLAGS=
  "-D missing_docs" cargo doc --no-deps --workspace`**: exits 0 (pre-existing
  `rustdoc::broken_intra_doc_links` warnings on `TopologyHint`/`StreamEvent`/
  `types`/`JobScheduler` are warnings, not `missing_docs` errors, and none are
  in this branch's new fields' doc comments). No reference to the old
  `select_model` signature (`-> &'static str`) survives anywhere in the
  workspace — grepped explicitly.

**Decision (`max_iterations: 0` is the infinite-loop sentinel — a one-way
door):** `Task.max_iterations: Option<u8>` uses `0` to mean "no cap," not an
`Option`-based ∞ or a separate boolean. This was chosen deliberately over the
`Option` alternative (locked in per the sprint brief) and matches the "0 =
disabled/unbounded" convention `LoopConfig` already uses for
`no_progress_limit` and `budget_tokens` — no new convention introduced.
**One-way-door consequence:** every consumer of `AgentRunner.max_turns` had to
be audited for "0 means unlimited" rather than "0 means immediately expired."
Two call sites got this wrong by default and were fixed as part of this
sprint: the hard-stop check in `runner/run_loop.rs` (`turn_count > max_turns`
would have fired on the very first turn) and the CLI flag pass-through
(`ClaudeCode::with_max_turns` would have sent a literal `--max-turns 0` to
the real `claude` subprocess). Both now special-case `max_turns == 0` to skip
the cap/flag entirely. Any future code that reads `max_turns` must do the
same — there is no compiler enforcement of this invariant.

**Decision (scope expanded from "expose existing fields" to "add two new
`Task` fields"):** the sprint brief's original ask was pure surface exposure
— wire already-tested fields through to the web API. Recon before writing
any code found that `Task.model`/`Task.effort` had **no existing backing at
all** (`select_model` is a pure heuristic reading nothing stored; "effort" is
a verifier-only concept) and `max_iterations` lived only on the repo-level
`LoopConfig`, never on `Task`, with no per-task override precedent. Exposing
these as dead `CreateTaskRequest` fields with nowhere to bind would have been
worse than not exposing them — silent, misleading surface. Flagged to the
user before writing code; explicitly authorized to add the two new `Task`
fields plus the minimal read-side wiring, rather than silently inventing
fields or silently dropping them from scope.

**Decision (worker `effort` is stored, not yet folded into any prompt):**
unlike `verifier_effort` (folded into the verifier's system prompt via
`build_system_prompt`), `Task.effort` has no equivalent fold point for the
worker. The direct-API planning path's system prompt
(`api_client::LOPI_SYSTEM_PROMPT`) is `cache_control: ephemeral` and must
stay byte-identical across a task's retry loop to keep its ~90% cache-hit
rate (see Sprint G's doc comments in `runner/api_plan.rs`) — folding a
per-task hint into it would silently regress that optimization. Rather than
invent a fold point under sprint pressure, `Task.effort` is stored
(round-trips through the API, survives serialization) and left unconsumed;
folding it in is a deliberate follow-up design pass, not a default assumed
here.

**Decision (task-level override always wins, mirroring `verifier_model`):**
`build_runner`'s `max_turns` resolution is `task.max_iterations.unwrap_or(repo_max_iterations)`
and `select_model` checks `task.model` before any heuristic — both follow the
"explicit wins over default" precedent Sprint 4 already established for
`verifier_model`, rather than inventing a new precedence rule.

**Fixed in passing (was a latent gap, not introduced by this sprint):**
`LoopConfig.max_iterations` was loaded by `run_one` (for a tuple destructure)
but never actually applied to `AgentRunner.max_turns` — any repo customizing
`.lopi/loop.toml`'s `max_iterations` had that setting silently ignored.
Closed as part of wiring the task-level override, since both needed the same
plumbing. Also fixed in passing: the blocking `LoopConfig` load's `JoinError`
fallback used `.unwrap_or_default()` silently (a `no-silent-failures` gap) —
now logs via `tracing::warn!` and falls back to `LoopConfig::default()`
explicitly, so `max_iterations` lands on its safe default (25) rather than
`u8::default()` (0 — the new infinite sentinel) in that rare failure path.

## Sprint 4 — Verifier as Explicit Gate (`crates/lopi-agent/src/verifier.rs`, `crates/lopi-agent/src/runner/verifier_runner.rs`, `crates/lopi-core/src/{loop_config.rs,task.rs}`, `crates/lopi-orchestrator/src/pool/run_loop.rs`)

**Decision (never-grade-your-own-homework default):** when `verifier_model` is
unset, the resolved verifier model must differ from the worker model that
produced the diff being graded. Documented default: **Opus**, unless the
worker itself already ran on Opus (an escalated retry, `attempt >= 2` per
`select_model`), in which case the verifier falls back to **Sonnet** instead.
This is a pure function, `lopi_agent::verifier::resolve_verifier(worker_model,
verifier_model, verifier_effort) -> (model, effort)`, unit-tested in isolation
— it is the one place this rule is enforced, so `run_verifier_pass` never
duplicates the logic. An *explicit* `verifier_model` is always honored as-is,
even if it happens to equal the worker's model — that's a deliberate operator
override, not a default, and enforcing "different" there would silently
override a user's stated choice.

**Decision (effort is a prompt hint, not a wire parameter):** `verifier_effort`
threads into `VerifierAgent::verify`'s system prompt as a plain-text
`"Reasoning effort: {effort}"` line, the same convention the web cockpit
already uses for worker-side launch controls (`web/src/lib/stores/agents.ts`
folds its `effort` selector into a planning constraint the same way — see
`CHANGELOG.md`'s "Model / effort / priority / repo / branch selectors" entry).
The Anthropic API client (`AnthropicClient::complete`) has no reasoning-effort
request parameter at all — only a token-based `task_budget` (Phase 16.6),
which is a different mechanism (self-pacing, not reasoning depth). Inventing a
wire-level parameter that doesn't exist would be scope creep beyond "activate
and parameterize" the existing VerifierAgent; folding it into the system
prompt text reuses an established pattern instead of adding a new one.

**Decision (the pool-construction seam):** `run_one`'s runner-builder chain
was extracted into `build_runner` — a pure assembly function (no I/O) that
takes every already-resolved input and returns the configured `AgentRunner`,
calling `.with_verifier()` when `task.verifier_required ||
task.verifier_model.is_some()`. This is the load-bearing kill-test seam
(Capability 2's kill-test, `PROMPTS_PLAN.md`): a unit test builds a `Task`
with `verifier_required = true` and an `AutonomyLevel::DraftPr` (L2, which
alone would *not* force the verifier) and asserts the resulting
`AgentRunner::verifier_enabled()` is `true` — without ever calling `.run()`,
so the never-before-exercised maker/checker flow is proven wired without
actually executing it. `AgentRunner::verifier_enabled()` (a `pub const fn`
getter) was added for exactly this assertion; the field itself
(`AgentRunner.verifier_enabled`) already existed but had no external reader.

**Why the seam, not a network-level assertion:** `PROMPTS_PLAN.md`'s literal
kill-test wording ("assert the client received SONNET, not OPUS") implies
intercepting the outbound HTTP call, but `AnthropicClient` has no
base-URL injection point and the workspace has no HTTP-mocking dependency.
Adding one would be a new third-party dependency and a wire-level change to
`AnthropicClient` — both outside this sprint's pre-authorized scope ("REUSE
[VerifierAgent] AS-IS... this sprint only activates and parameterizes it").
The equivalent, dependency-free proof: `resolve_verifier` (the only place a
model gets chosen) is unit-tested directly, and `verify`'s body — visible in
the diff this sprint prints — has zero remaining reference to a hardcoded
model constant; the `model: &str` parameter flows straight into `.complete()`
with no branch in between.

**What now exercises the previously-dead `.with_verifier()` path:** any task
or `.lopi/loop.toml` that sets `verifier_required = true` or a
`verifier_model`, submitted through `AgentPool::submit` → `run_one` →
`build_runner`. Before this sprint the only way to force the verifier was
`autonomy_level >= VerifiedPr` (L3/L4); that mechanism is untouched
(`requires_verifier` in `finalize.rs` still ORs both together at finalize
time). The first time this call site runs in production will be the first
real, live exercise of `VerifierAgent`'s maker/checker isolation outside its
own unit tests — treat an early failure there as expected discovery, not a
regression.

**Housekeeping:** two existing test-only `Task { .. }` struct literals
(`crates/lopi-agent/src/runner/api_plan.rs`, `crates/lopi-agent/src/stability/mod.rs`)
needed the three new fields added to compile; `dry_check.py` still flags
these two helpers as near-duplicates of each other (pre-existing, unrelated
to this sprint — both already duplicated the full `Task` literal before this
change) and unrelated pre-existing duplication elsewhere in the workspace
(`lopi-webhook`, `lopi-spec`, `lopi-remote`). No verifier logic itself is
duplicated anywhere — `resolve_verifier` and the one `.with_verifier()` call
site are each defined exactly once.

**How to apply:** any future "gate" field that should be forceable
independent of `autonomy_level` should follow this same shape — a bool +
optional override(s) on both `LoopConfig` and `Task`, `#[serde(default)]`,
read at the pool-construction seam rather than threaded through `.lopi/loop.toml`
at runtime (Task is the authoritative per-run source, matching how
`autonomy_level` already works — `LoopConfig`'s copy is the UI-editable
repo-level default/display value, not something `run_one` re-reads
automatically). Any future "resolve a value that must differ from another
value" pattern should follow `resolve_verifier`'s shape: a pure function,
unit-tested in isolation, called from exactly one production site.

## Sprint 3 — Report on Finish (`crates/lopi-core/src/{report.rs,config.rs,task.rs,event.rs}`, `crates/lopi-agent/src/runner/finalize.rs`, `crates/lopi-remote/src/telegram/notify.rs`)

**Decision (dependency edge):** neither pre-authorized edge (`lopi-agent` →
`lopi-remote`, or a trait-in-core) was taken. Reading the actual dep graph
first showed `lopi-remote` already depends on `lopi-orchestrator`, which
depends on `lopi-agent` — so `lopi-agent` → `lopi-remote` would have been a
real cycle, exactly the failure mode `NEXT.md` flagged up front. Instead,
`AgentEvent` (already in `lopi-core`, already depended on directly by both
`lopi-agent` and `lopi-remote`) gained one new variant, `ReportReady { task_id,
channel, summary }`. `emit_report` broadcasts it on the existing
`EventBus<AgentEvent>`; `lopi-remote`'s already-running `notify_loop` gained
one new match arm that calls the existing `send_msg` helper. Net new
dependency edges: **zero** — `cargo tree -p lopi-agent` / `-p lopi-remote`
are unchanged, no `Cargo.toml`/`Cargo.lock` edits at all. This is a stronger
fit than either pre-authorized option: it needed no new abstraction (the
event-bus *is* the report-sink seam) and no cross-crate call.

**Decision (chat_id):** option (a) — the report reuses the single global
`remote.telegram.chat_id` this loop was booted with. `notify_loop`'s existing
gate (`return` when `chat_id` is `None`) is untouched; `ReportReady` just adds
another event the existing `chat_id: ChatId` in scope can be sent to. **Known
limitation:** every `report = "telegram"` schedule in a given `lopi` process
notifies the same chat — there is no per-task destination yet. Building
per-task routing (option b — `ScheduleEntry` carrying a target chat id) was
explicitly out of scope this sprint (`NEXT.md`: "do NOT build a full per-task
routing system"); revisit if/when multiple distinct Telegram destinations are
needed.

**Decision (channel validation):** `report: Option<String>` (not a typed enum
field) on both `ScheduleEntry` and `Task`, per `NEXT.md`'s explicit call —
threaded from `ScheduleEntry` to `Task` in `scheduler.rs` the same one line as
`autonomy_level`. The typed side is `ReportChannel::parse(&str)` in the new
`lopi-core::report` module: `"telegram"` parses; `"whatsapp"` is a *named*
`WhatsappUnsupported` error (inbound-only Twilio webhook, no send path — not
lumped in with generic `Unknown`); anything else is `Unknown(name)`. Called
in two places, both reusing the same `parse` fn (no second scanner): (1)
`LopiConfig::load()` validates every `[[schedules]]` entry's `report` and
fails the whole load loudly on a bad channel — a typo'd config never silently
never-sends; (2) `emit_report` re-validates defensively (a `Task` can reach
`emit_report` from sources other than `ScheduleEntry`), `tracing::warn!`-ing
and skipping the broadcast rather than sending an unrecognized channel name.

**Why:** the config-load validation is the one guaranteed choke point — every
`ScheduleEntry` a user writes passes through it, so it is where a typo must be
caught, not where it's merely convenient to catch it. Re-validating at
`emit_report` costs one extra `match` and closes the gap for tasks built
outside the schedule path (API, CLI) that could carry an unvalidated `report`
string directly.

**Housekeeping:** `crates/lopi-core/src/event.rs` was already at 590 lines
(over the 500-line hard gate) before this sprint; adding `ReportReady` pushed
it to 621. Since the file-size CI gate scans *changed* files on a PR, this
sprint's edit would have tripped it. Split the file's two `#[cfg(test)]`
modules out to `event_tests.rs` / `event_wire_format_tests.rs` via the
`#[path = "..."]` pattern already used by `config_tests.rs` /
`loop_config_tests.rs` — a pure test-relocation, zero logic changes — bringing
`event.rs` itself to 323 lines. Same category of proactive split as
`run_loop.rs`'s (Sprint 2 era), just triggered by an existing-debt file this
time rather than new code.

**How to apply:** any future `lopi-agent` → `lopi-remote` (or similarly
"downstream" crate) communication should default to an `EventBus<AgentEvent>`
variant before reaching for a new dependency edge or a bespoke trait —
check `cargo tree` for the real graph first, since a plausible-looking direct
call can be a cycle in disguise. Any new `report`/channel-shaped field should
validate through `ReportChannel::parse`, not a second name-matching branch.

## Sprint 2 — Skill Arguments (`crates/lopi-skill/src/{lib.rs,invocation.rs}`)

**Decision:** empty `args` on a body containing `$ARGUMENTS` is an **empty
fill, not an error** — `$ARGUMENTS` becomes `""`, and rendering still
succeeds. And: `render_body` reuses `template::resolve` by *translating*
`$ARGUMENTS` → `{arguments}` and calling `resolve` with a one-entry
`{"arguments": args}` vars map — no second `.replace()`/scanner, per Sprint
1's hard reuse constraint. `Skill` needs no new frontmatter field for this;
`$ARGUMENTS` lives in the existing body `String`.

**Why:** an empty-fill (not an error) is the least-surprising choice —
`:kcqf` alone (no argument) is a legitimate, common invocation shape, and
`resolve` itself already treats a *present* vars entry mapped to `""` as a
perfectly valid substitution (this is distinct from a *missing* key, which
is still the loud `TemplateError` Sprint 1 built). Erroring on empty args
would penalize the common case for no real safety gain. On reuse: the
translate-then-delegate approach was chosen over extending `resolve` with a
second hole syntax (`$NAME`) because it needed **zero changes** to
`template.rs` — the smallest change that could possibly work, and it
composes: any future skill-body placeholder can follow the same
translate-to-`{hole}` pattern without `template.rs` ever learning a second
syntax. The tradeoff this creates: a skill body with a genuinely stray,
unescaped `{` (not part of `$ARGUMENTS`) will error on invocation, exactly
as a hand-written template would — skill authors get Sprint 1's `{{`/`}}`
escape rule "for free," not a more lenient bespoke rule.

**How to apply:** any future skill-body placeholder should translate to a
`{hole}` and delegate to `resolve`, not add new substitution logic. If a
skill body needs to contain a literal, un-doubled `{` going forward, that's
now a real authoring constraint worth documenting in the skill-writing docs,
not a bug in `render_body`.

## Sprint 1 — Prompt Templates (`crates/lopi-core/src/template.rs`)

**Decision:** escaping follows Rust's `format!` rule — `{{` and `}}` decode to
a literal `{` / `}`, independently of hole-matching (not a paired
`{{...}}` block). And: stop at a bare `resolve()` fn — no `PromptTemplate`
newtype.

**Why:** the escape rule is copied wholesale from a convention every
Rust contributor to this repo already knows (`format!`/`println!`), so there's
no new grammar to learn or document — `{{brace}}` reads as "the same rule as
`format!`" instead of a bespoke invention. The fn-vs-newtype call: a newtype
would only earn its keep once templates carry state beyond the string itself
(a source location, a cached parse, validation metadata) — none of which this
sprint's four call sites need. Building it now would be exactly the kind of
premature abstraction CLAUDE.md warns against; the moment a second sprint
needs more than a `&str` in, `String` (or `Result`) out, promote it then.

**How to apply:** any future sprint that touches template syntax (nested
holes, default values, conditional holes) must extend this same escape rule
rather than introducing a second one — and should re-examine the newtype
question at that point, not before.
