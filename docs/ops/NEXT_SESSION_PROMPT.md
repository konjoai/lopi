# Next Session — after macOS-Web-Parity-5 (handoff to a local/Xcode session)

**SHIPPED (pending PR).** Threaded a task's effective `repo` through the entire stack: `AgentEvent::TaskStarted` (Rust) → a new `tasks.repo` DB column (persisted the moment `TaskStarted` fires, mirroring `tasks.branch`'s exact precedent) → `GET /api/tasks`/`:id` + the WS snapshot → both web's and macOS's client decode paths. This closes the structural gap Parity-4 flagged, **and fixes a real bug in web itself**: `agentReducer.ts`/`types.ts` already read/declared `repo` client-side, entirely dead because the Rust backend never sent it — web's own `byRepo` Budget panel has apparently never worked. macOS's `LiveAgent` gained `repo`, and Budget gained the "by repo" panel Parity-3 deliberately deferred pending exactly this.

**TESTS — this one's different from every prior round.** The Rust and web layers are **fully verified**, not "written, not built": `cargo build --workspace`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` are all green; `npm test` is green (after running `svelte-kit sync` once — this container's `web/` had never had it run, a missing environment step, not a code issue; `web/node_modules` also needed a first-time `npm install`). New Rust tests: `task_repo.rs`'s round-trip/no-op/overwrite suite, `streaming.rs`'s snapshot repo-presence cases, and an end-to-end `tests_extended.rs` test that drives the real axum router (not mocked) through `GET /api/tasks`/`:id`. New web tests: `parser.test.ts`'s snapshot whitelist cases. **Only the macOS Swift side is written-not-built** — same standing constraint as every prior round, now narrowed to just one layer instead of the whole change.

**NEXT SESSION:**
1. **Compile-verify the macOS side** — `cd macos && xcodegen generate && xcodebuild -scheme Lopi build`, then `xcodebuild -scheme Lopi test` (acceptance bar: `StatsParityTests`'s new taskStarted/snapshot repo cases, `BudgetBreakdownTests`'s `groupCostByRepo` coverage).
2. **Live smoke test once compiling:** run a real task on a known repo, confirm `Overview`'s kanban cards already show the right repo (via card config, unaffected by this change) and that `Budget`'s new "BY REPO" panel populates with the running/finished task's actual repo, basenamed.
3. **Worth flagging upstream (not a macOS-parity task, but found during this sprint):** confirm with a human whether web's `byRepo` panel has genuinely never rendered real data in production, or whether some other path fed it that this sprint's research missed — the fix here should make it start working going forward regardless, but "was this silently broken since it shipped" is worth closing out explicitly rather than left as an inferred assumption.
4. **The known-gap backlog from diffing `web/`'s git history is exhausted again** (as of `a2ce843`, unchanged since Parity-4's check). Re-run `git log --oneline -- web/` against current macOS state before assuming there's a next obvious target; the last three rounds (Overview, Budget, this one) all started that way.

---

# Next Session — after macOS-Web-Parity-4 (handoff to a local/Xcode session)

**SHIPPED (pending PR).** Small, well-scoped round: `ConfigView.swift` and `CronView.swift` each gained a page-level header ("Configuration"/"Scheduling" + subtitle), matching web's `0cdd3a0` design-system-alignment commit — and, more to the point, matching the header convention `BudgetView`/`OverviewView`/`DashboardView` already use on macOS itself. Deliberately not touched: the web-only "Onboard" screen (no macOS nav equivalent, a platform-structural asymmetry like `Dashboard`'s own one-way gap) and `a2ce843`'s focus-ring CSS recolor (no macOS analogue — AppKit's native focus ring has no equivalent seam). No new tests: both changes are static header text, same no-dedicated-test precedent as every other page header here.

**TESTS.** `cargo build --workspace` green (no Rust touched). Written, not built — same standing constraint as every prior Swift round.

**NEXT SESSION:**
1. **Compile-verify before writing any new Swift**, same as every round: `cd macos && xcodegen generate && xcodebuild -scheme Lopi build`.
2. **The macOS-web parity backlog is thin right now** — three rounds (Overview kanban + blocked-status fix, Budget breakdown, Config/Cron headers) have worked through every web commit touching `web/` since the last full parity audit (`PARITY_AUDIT_2026-07-16.md`) up through `a2ce843` (2026-07-22, the newest `web/`-touching commit as of this writing). Before starting a new round, **re-run the same method these four rounds used**: `git log --oneline -- web/` and diff anything newer against the current macOS state — don't assume the backlog is still empty by the time this is read.
3. **Two structural gaps surfaced across these rounds, neither mechanical, both worth a real audit session:** (a) `LiveAgent` has no `repo` field — blocks a `byRepo` Budget panel and keeps `Store/Overview.swift`'s goal/repo column stuck at `"—"`; threading it through means touching the wire event model (`crates/lopi-core`'s task-started event, `AppModel+Live.swift`'s decode), not just the Swift client. (b) web's Budget redesign switched several elements from state-reactive (jade/flame/rose) to fixed literal colors; macOS still branches on burn state. Neither was fixed opportunistically during these rounds — both need a human call on which platform's behavior is actually correct before mechanically porting either direction.

---

# Next Session — after macOS-Web-Parity-3 (handoff to a local/Xcode session)

**SHIPPED (pending PR).** macOS's Budget view catches up to web's 2026-07-22 `feat(budget)` sprint: new `Networking/BudgetModels.swift` (`BudgetBreakdown` decoding `GET /api/budget/breakdown`) + `LopiClient.budgetBreakdown()`, new `Store/BudgetTrend.swift` (pure `weekdayAbbrev`/`trendBars`/`trendDelta`), and `BudgetView.swift` gains a 7-day spend-trend chart, a by-model cost breakdown, an alert-threshold slider, and TOKENS/RUNNING stat cards (6 total, up from 4). Deliberately not ported: the "by repo" breakdown — macOS's `LiveAgent` has no `repo` field yet (same pre-existing gap `Store/Overview.swift` already flags), and no color-scheme reconciliation with web's newly-fixed (non-state-reactive) meter/stat-card colors — see `LEDGER.md`'s `macOS-Web-Parity-3` entry for why both were deliberately left alone rather than folded into this sprint.

**TESTS.** `cargo build --workspace` is green (no Rust touched). **Written, not built** — same standing constraint as every prior Swift round. New `BudgetBreakdownTests.swift` covers `BudgetBreakdown` decoding (including a missing-keys case) and every branch of `weekdayAbbrev`/`trendBars`/`trendDelta`. None of it has been compiled or run.

**NEXT SESSION (the exact next task, in order):**
1. **Compile-verify before writing any new Swift.** `cd macos && xcodegen generate && xcodebuild -scheme Lopi build`, then `xcodebuild -scheme Lopi test` — acceptance bar is `BudgetBreakdownTests` passing alongside the existing suite.
2. **Live smoke test:** run `lopi sail` against a repo with some real turn history, open Budget, confirm the 7-day trend bars and by-model breakdown render with real data (not just the empty-state copy), and that the alert-threshold slider persists across a relaunch the same way the hourly-cap presets already do.
3. **Two carried-forward items, both cited in `LEDGER.md`'s entry, neither urgent:** (a) threading a `repo` field through the live event model (`LiveAgent`/`AppModel+Live.swift`) so a `byRepo` panel becomes buildable — also unblocks Overview's own goal/repo column, which has carried the same `"—"` placeholder since before Parity-2; (b) deciding whether web's fixed-color burn meter/stat cards (vs. macOS's still-state-reactive ones) is a deliberate design call worth reconciling, or a stray regression not worth chasing — needs a human call, not a mechanical port.
4. **After Budget, the next candidate parity sweep:** diff web's git history for `web/src/routes/config/+page.svelte`, `schedules/+page.svelte`, `onboard/+page.svelte` against macOS's `ConfigView.swift`/`CronView.swift` — commit `0cdd3a0` (2026-07-22, same day as the budget sprint) aligned Config/Schedules/Onboard to the same design system Budget/Loop/Overview already share (shared page-header treatment, `StatCard` reuse). Mostly cosmetic/design-system alignment rather than a missing capability, so lower priority than a real feature gap, but not yet audited against macOS's current state the way Budget/Overview were.

---

# Next Session — after macOS-Web-Parity-2 (handoff to a local/Xcode session)

**SHIPPED (pending PR).** macOS's `/overview` rewritten from a flat per-agent rollup table to the 4-column kanban board web shipped on 2026-07-21 (`stores/stackOverview.ts`/`StackOverviewCard.svelte`, commit `2dee147`) — the same design macOS's own Overview port (`ef2bd20`, 2026-07-17) predated by four days. New `macos/Lopi/Store/StackOverview.swift` (Swift port of `stackOverview.ts`), rewritten `OverviewView.swift` + new `StackOverviewCardView.swift`, click-to-focus via `AppModel.focusedStackKey` + `ForgeView`'s new fading-ring flash. Along the way, found and fixed a real (not cosmetic) bug: `StackRun.swift`/`StackRunControls.swift` marked every failed/cancelled stack card `.done` instead of a new `.blocked` status — `CardStatus`/`StackCard.blockReason` never picked up web's "round 2, item 3" addition. Duplicate/clone paths (`StackOps.swift`/`StackPaneOps.swift`) now also reset `blockReason`. Dead per-agent-rollup code (`overviewRows`/`OverviewFilter`/etc., `Store/Overview.swift`) deleted rather than left the way web left its own equivalent. Full reasoning in `LEDGER.md`'s `macOS-Web-Parity-2` entry.

**TESTS.** `cargo build --workspace` is green (no Rust touched). **The Swift side is written, not built** — same standing constraint as every prior Swift round in this repo. New `StackOverviewTests.swift` ports `stackOverview.test.ts`'s assertions; `StackRunTests.testFailingCardHalts` gained a blocked-status assertion; `OverviewTests.swift` trimmed to what's left post-port. None of it has been compiled or run.

**NEXT SESSION (the exact next task, in order):**
1. **Compile-verify before writing any new Swift.** `cd macos && xcodegen generate && xcodebuild -scheme Lopi build`, then `xcodebuild -scheme Lopi test` — acceptance bar is `StackOverviewTests`/`StackRunTests.testFailingCardHalts` passing alongside the existing suite (which now has two new `CardStatus`/`StackCard` fields threaded through it). Also `cd packages/LopiStacksKit && swift test` for the `CardStatus`/`StackRun` changes. Fix whatever the Linux-authored diff got wrong.
2. **Live smoke test:** run a real Loop Stack chain to a failure (an easy repro: a card whose goal can't possibly succeed, or `on_fail: continue` with a card that fails fast) and confirm the failed card shows the rose blocked state + inline reason on `ForgeView`'s `StackCardView`, and that `/overview`'s board buckets that stack into Done with the "failed" meta text and rose accent — the same live-proof discipline `PARITY_AUDIT_2026-07-16.md`'s KT3 used for the schedule popover.
3. **Carried forward, not attempted this sprint:** `blockReasonFor`'s richer web behavior (preferring a live verifier-gap/task-status detail over the generic fallback string) needs `StackRunSeams.waitForTerminal` to return more than a bare `TerminalStatus` enum — a larger seam change than this sprint's Overview-board scope. The macOS-web icon-system pairing and sidebar/layout pixel-gap measurement from `PARITY_AUDIT_2026-07-16.md` remain open and unrelated to this sprint.

---

# Next Session — after iOS-Web-Parity-Plan-1 Phase 0 (handoff to a local/Xcode session)

**SHIPPED.** Two PRs, both merged to `main`: **#146** — `docs/ops/IOS_WEB_PARITY_PLAN_2026-07-23.md`, a citation-backed audit of `macos/LopiIOS/` against web's 6-item nav plus a 7-phase plan to close the gaps; **#147** — Phase 0 of that plan, porting web's Composer-Grammar-1 `/` → `;` rename into `packages/LopiStacksKit/Sources/LopiStacksKit/StackOps.swift`'s shared `CARD_COMMANDS`/`STACK_COMMANDS` grammar (fixes macOS and iOS in one change), killing `/loop/N` outright (not renaming it — `xN`/`×N` is the sole loop-count grammar, matching web), and updating the per-platform call sites that aren't shared (`StackCardView.swift`/`StackControlDockView.swift` on macOS, `StackCommandBar.swift`/`StackDetailScreen.swift` on iOS).

**TESTS.** `cargo build --workspace` is green (no Rust touched by either PR). **The Swift side is written, not built** — this session ran on a Linux host with no Xcode, the same standing constraint as every prior Swift round in this repo (`docs/ops/IOS_RESEARCH_1_SPIKE.md`, Verify-4, macOS-Loop-Stacks-1). `StackStoreTests.swift` gained a new `testComposerGrammarRenameAcceptance` (the `;model/sonnet`/`;effort/high`/`;branch/main`/`;autonomy/L2`/`;eval/kcqf` table) plus renamed `testInlineCommandAutocomplete`/`testDetectPendingCommand` assertions, but none of it has been compiled or run — this is real, unverified risk, not a formality.

**PUSHED.** Both merged: `817a9d5` (PR #146), `953883b` (PR #147). This handoff section itself is the next commit on this branch.

**NEXT SESSION (the exact next task, in order):**
1. **Compile-verify Phase 0 first, before writing any new Swift.** On a machine with Xcode:
   - `cd macos && xcodegen generate && xcodebuild -scheme Lopi build` (macOS target)
   - `xcodebuild -scheme LopiIOS build` (iOS target)
   - `cd packages/LopiStacksKit && swift test` — acceptance bar is `testInlineCommandAutocomplete`/`testDetectPendingCommand`/`testComposerGrammarRenameAcceptance` passing alongside the existing ~60 ported assertions, untouched by this change.
   - Fix whatever the Linux-authored diff got wrong — expect at most a small, single-root-cause gap, per the discipline every prior "written not built" round in this repo has actually hit (one closure-capture bug, one `@MainActor` gap — never a pile of typos).
   - Live smoke test: type `;model/` in the composer on both platforms, confirm the value picker appears; confirm the old `/model/` no longer triggers anything (hard cutover, by design).
2. **Then Phase 1 of `docs/ops/IOS_WEB_PARITY_PLAN_2026-07-23.md`: RunMenu + bump on iOS.** iOS's `StackDockView` (`StackCommandBar.swift`) currently has only a bare "run stack" button that always calls `.run` directly — no Run once / Dry run / Schedule stack / Pause / Resume / Drain, and no bump (▲/▼ reorder) UI, both of which macOS (`RunMenuView.swift`) and web (`RunMenu.svelte`) already have. This is an iOS-specific regression versus *both* other platforms, not a platform-parity nice-to-have — closes the biggest in-surface gap on the one surface iOS already ships, before Phases 2–6 (Budget/Scheduling/Loop/Overview/Config, all currently missing entire iOS screens) add new nav destinations.

**DISCOVERIES worth carrying forward:**
- **iOS's missing surfaces (Budget/Loop/Scheduling/Config) are a UI-screens problem, not a networking problem** — `project.yml`'s `LopiIOS` target already compiles in `Lopi/Networking`/`Lopi/Store` wholesale, and the macOS views for those surfaces (`BudgetView.swift`, `LoopView.swift`, `CronView.swift`, `ConfigView.swift`) have zero AppKit-only dependencies. This changes the shape of Phases 2–6: expect narrow-layout SwiftUI adaptation, not new backend integration.
- **`LopiStacksKit`'s shared-package boundary is real leverage.** One edit to `StackOps.swift` fixed the grammar on both native platforms at once — the same pattern should be checked for before assuming any future macOS/iOS divergence needs two fixes.

**HEALTH: Yellow.** Structurally sound (tests written, docs updated, `cargo build` green, both PRs passed CI and merged clean) but the core Swift logic change has never been compiled — that's the one thing standing between "done" and "shipped" here, and it's squarely a local-session task.

---

# Next Session — after Browser-Pane-1

Browser-Pane-1 confirmed the Browser pane cleanly shows the real, already-running `lopi sail` dashboard (real data, zero new code) and that Claude navigates there autonomously from a natural, mechanism-blind prompt — both true even before any `CLAUDE.md` note existed (see `LEDGER.md`'s "Browser-Pane-1" entry for the full finding). A `CLAUDE.md` "Live Dashboard (Browser Pane)" section was added anyway, to make the check-for-already-running / start-if-needed / `preview_start`-explicitly sequence explicit rather than rely on every future session re-deriving it.

**One unresolved kill-test, carried forward: does a genuinely cold Claude Code Desktop session actually rely on that new `CLAUDE.md` section, or does it keep succeeding independently of it the way this session's test subagent did?** The `Agent`-tool subagent spawned to test this inherited a `CLAUDE.md` context snapshot from the start of the Browser-Pane-1 conversation — *before* the note was added — so its success only proves the underlying capability doesn't strictly need the note; it isn't evidence the note works. Testing this for real requires quitting and relaunching the actual Claude Code Desktop app (a new top-level process, not a subagent spawned mid-session) and asking something like "what's lopi running right now" cold, with no other hint. If a fresh top-level session cites or clearly acts on the `CLAUDE.md` section, this closes clean. If it (still) succeeds without ever touching that section, the honest conclusion is that the note is unnecessary — remove it as noise rather than leave a rule nothing reads.

---

# Next Session — after Permission-Modes-1

`Task.permission_mode` / `CreateTaskRequest.permission_mode` now thread end
to end: `lopi-core::PermissionMode` (four headless-safe values, CLI-literal
serde tags, default `bypassPermissions`) → `apply_cli_caps` (now emits
`--permission-mode` unconditionally, folded in from being per-site) → all
three `claude -p` spawn sites → the web dropdown in both
`StackConfigPopover.svelte` and `ConfigDrawer.svelte`, fully wired through
`cardToTaskPayload`/`cardToTaskPayloadForRunOnce`/`paneSubmitPayload` (unlike
`autonomy`, which stays client-only). See `LEDGER.md`'s `Permission-Modes-1`
entry for the kill-test evidence and the one-way-door decisions (the
four-mode subset, the CLI-literal enum strings, the `BypassPermissions`
default, and why the flag folded into `apply_cli_caps` instead of staying
per-site).

**Two concrete items carried forward, both explicitly out of scope for this
sprint:**

1. **Wire `require_plan_approval` into the frontend.** Real, already
   server-side (`Task.require_plan_approval` /
   `CreateTaskRequest.require_plan_approval` / `plan_gate.rs`'s
   channel-based human-approval gate on the first attempt's plan), and
   architecturally the closest thing lopi has to Claude Code's own `plan`
   mode — but it needs its own approve/reject UI for
   `AwaitingPlanApproval`/`PlanProposed` before it's safe to expose as a
   toggle. Flipping it on today would let an operator silently strand a
   task for an hour (the gate's auto-reject-on-timeout) with no UI telling
   them a plan is even awaiting approval. Build the approve/reject surface
   first, then wire the toggle.
2. **`.lopi/loop.toml` repo-level `default_permission_mode`.** `LoopConfig`
   already carries `permission_allow`/`permission_deny` (used by this
   sprint's KT2, unmodified); a repo-level default permission mode is a
   reasonable follow-up but touches the TOML schema and the
   `/api/loop-engineering` read surface, which has no write path today
   either. Deliberately deferred — this sprint was "start with web," not
   repo config.

**Also flagged, not carried forward as a sprint (informational only):**
KT4 (the account lopi's production deployment authenticates as — `auto`
mode's model/provider/plan/Team-Owner-toggle eligibility) and KT5 (the
deployed container's actual runtime user) were both left unverified this
sprint — the sandbox that ran it had no visibility into the production
account's credentials and no `fly`/attended access to the live container.
A session with either kind of access should close these two rather than
continuing to defer them; see `LEDGER.md`'s entry for exactly what's
missing from each.

---

# Next Session — after Composer-Grammar-2

Real Claude Code `/name` command discovery + composer hookup landed for
Phases 1-2 (backend discovery + frontend autocomplete/chip wiring), fully
tested and merged. **Phase 3 — the actual `claude -p` pass-through — did
not ship.** It is gated on a live-proof kill-test this session's sandboxed
environment cannot run: a nested `claude` CLI invocation is blocked at the
permission-classifier level (confirmed by attempting it with a real fixture
repo, not assumed blocked).

**The one concrete item carried forward — run kill-test 1 for real:**

1. **Find an environment where a `claude` CLI call isn't self-referentially
   blocked** (the user's own machine, or wherever this repo's prior
   "M3 + real auth" sessions ran from — `LEDGER.md`'s MAXX/quota entries and
   the iOS-Research-1 entries used the same phrase for the same class of
   problem, though those were missing-hardware/missing-toolchain, not
   permission-classifier, blockers).
2. **The fixture-repo protocol is already built out** — don't re-derive it,
   re-run it: create `.claude/commands/foo.md` with a body that prints a
   literal marker string (e.g. `KILLTEST_FOO_EXPANDED`) and nothing else, no
   tool use. Run `claude -p "/foo" --output-format stream-json --verbose
   --dangerously-skip-permissions` two ways: (a) the command as the bare
   entire prompt, (b) the command embedded mid-string inside prose shaped
   like `crates/lopi-agent/src/claude_support.rs::build_plan_prompt`'s real
   TOON-wrapped output (goal + constraints + allowed/forbidden dirs +
   pattern-memory table preamble, `/foo` somewhere in the middle, more prose
   after). Check the `stream-json` output's system-init event for the
   command name appearing in `slash_commands`, and confirm the fixture's
   *actual body content* executes (the literal marker string appears in the
   response) — not just the literal `/foo` text echoed back uninterpreted.
3. **Branch on the result, per the original sprint brief's Phase 3:**
   - **Passed both ways** (command expands even embedded mid-prompt): no
     code change needed. A `/name` token a user picks from the composer
     autocomplete already flows into `build_plan_prompt`'s wrapped prompt
     unmodified, since nothing strips or escapes it — Phase 3 is "done" by
     inaction, just needs the CHANGELOG/LEDGER entries confirming it.
   - **Failed embedded, passed bare only**: add the bypass Phase 3
     originally specified — detect a goal text whose leading token matches
     a discovered `/name` (reuse
     `lopi_skill::discover_claude_commands(repo)` against the task's own
     repo) and route around `build_plan_prompt`'s wrapping entirely via a
     new `ClaudeCode::run_raw(prompt)` that sends the goal text bare to
     `-p`. Small change — the live proof was always the hard part, not the
     code.
   - Either way, once resolved: verify end-to-end through lopi's own real
     task-submission path (not `claude` invoked in isolation) — a card whose
     goal is literally `/foo` (or `/foo` embedded in a longer prompt,
     depending on which branch fired) should produce a task whose plan/
     implement output shows the fixture command's real body content, not a
     literal `/foo` string surviving untouched in the model's response.

**Also worth revisiting once kill-test 1 resolves:** whether the composer
should visually distinguish "this `/name` token will definitely work"
(passed embedded) from "this only works as the very first thing typed"
(failed embedded) — today the autocomplete offers every discovered command
identically regardless of position in the goal text, which would be
actively misleading if kill-test 1 comes back position-sensitive.

---

# Next Session — after Composer-Grammar-1 (web)

`/` is now fully vacated on web: every lopi-specific composer command
(`model`/`effort`/`branch`/`autonomy`/`eval`/`guard`/`schedule`/`maxx`) moved
to a new `;` catch-all prefix, `/loop/N` was killed outright (`xN` is the
sole loop-count grammar), and `ChipInput.svelte`'s resolved-token chips now
reuse `ConfigDrawer.svelte`'s real per-field colors (`chip-model` cyan,
`chip-branch` green, `chip-effort` reconciled to the real ember, `chip-command`
renamed to `chip-autonomy`). `:alias`/`@repo`/`×N` untouched. See `LEDGER.md`'s
`Composer-Grammar-1 (web)` entry for the full reasoning, including why this
was a deliberate hard cutover with no backward-compat shim.

**Two concrete items carried forward:**

1. **`/` is safe to claim for real Claude Code slash commands now (the
   sprint this one was explicitly gating).** Web's `stack.ts`/`StackCard.svelte`/
   `StackControlDock.svelte` no longer read or write anything under a `/`
   prefix — that character is fully free for a real Claude Code `/`-command
   hookup in the same goal/cmdbar fields, with no autocomplete collision
   against lopi's own grammar (which now lives entirely under `;`).
2. **Resolved (`iOS-Web-Parity-Plan-1` Phase 0) — `LopiStacksKit`'s
   `StackOps.swift` (`CARD_COMMANDS`/`STACK_COMMANDS`/`commandAutocomplete`/
   `detectPendingCommand`/`commandValueAutocomplete`) now speaks the same
   `;` prefix as web, fixing both macOS and iOS in one shared-package change
   (`StackCardView.swift`/`StackControlDockView.swift` on macOS,
   `StackCommandBar.swift`/`StackDetailScreen.swift` on iOS were also updated
   for their own literal grammar-hint chips and trigger-character text-field
   logic, which aren't shared). `/loop/N` was killed outright on the Swift
   side too — `xN`/`×N` is the sole loop-count grammar, matching web.
   `stack.test.ts`'s kill-test-1 table (`;model/sonnet`, `;effort/high`,
   `;branch/main`, `;autonomy/L2`, `;eval/kcqf`) is ported as
   `StackStoreTests.testComposerGrammarRenameAcceptance`. **Still owed:** this
   was written on the same Xcode-less host as every prior Swift round in this
   repo — compile-verify (`xcodegen generate && xcodebuild -scheme Lopi
   build`, `swift test` for `LopiStacksKit`) before treating it as done.

---

# Next Session — after Stack-Chain-1 / Popover-Fix-1 / Parity-Audit-1

Server-side whole-stack cron scheduling shipped end-to-end (schema →
`ChainScheduleManager` with proven restart-resume → REST → web + macOS
wiring), the web popover overflow bug is fixed and live-verified, and a
citation-backed parity audit landed. Three concrete items carried forward:

1. **Run KT3 for real: screenshot the macOS schedule popover at a short
   window height.** `request_access` for the `Lopi` app was denied this
   session, so `macos/Lopi/Views/Forge/StackControlDockView.swift:217-225`'s
   `arrowEdge: .top` values (inconsistent with `StackCardView.swift:529-533`'s
   `.bottom`) were audited but deliberately left unchanged — see `LEDGER.md`'s
   "macOS `arrowEdge` values were audited but deliberately left unchanged"
   entry for why guessing would have been worse than leaving it alone. Build
   and run the app (`cd macos && xcodegen generate && open Lopi.xcodeproj`,
   ⌘R), resize the window to ~700px tall, add a 1-card stack, open the
   schedule popover, toggle "run on a schedule" on, and screenshot. If it
   clips: normalize the inconsistent `arrowEdge` values (`StackControlDockView`
   is the one this sprint is actually about — the stack-level dock pinned to
   the pane's bottom edge). If it doesn't clip (plausible — `.popover` is
   native `NSPopover`-backed on macOS and may already self-correct): leave it
   alone and note in `docs/ops/PARITY_AUDIT_2026-07-16.md` that KT3 resolved
   "already fine," closing that open row.
2. **Resolve the `LopiUITests` code-signing mismatch and actually run it.**
   `xcodebuild build-for-testing` succeeds; `xcodebuild test
   -only-testing:LopiUITests` fails to load the test bundle
   ("mapping process and mapped file (non-platform) have different Team
   IDs") — a local DerivedData signing inconsistency, not a project.yml or
   test-code issue (the app + `LopiTests` build and run fine with the same
   config). Likely fix: clean DerivedData
   (`rm -rf ~/Library/Developer/Xcode/DerivedData/Lopi-*`) and rebuild, or
   check `CODE_SIGN_STYLE: Automatic`/`DEVELOPMENT_TEAM: ""` in
   `macos/project.yml` resolves consistently for all three targets on this
   machine. Once it runs, `StackChainScheduleUITests.swift`'s two tests are
   the acceptance bar (chain-schedule popover opens with the cron builder;
   popover stays within the window frame at 700px height — the macOS
   analogue of the web Playwright regression test).
3. **Finish the parity audit's deferred items**, all blocked by the same
   macOS-access gap: the full 48-icon SF-Symbols-vs-SVG pairing (§3 of the
   audit doc did only a structural comparison), the sidebar/layout pixel-gap
   measurement (web half is tractable without macOS access — do that first,
   independently), and saving the web-side evidence as actual PNG files
   under `docs/ops/evidence/parity/` (this session's web evidence is DOM-
   measurement based, not saved screenshots — see the audit doc's
   "Verification method" section for why).

Also carried forward from the sprint's explicit non-goals, not forgotten:
**chain run-until-goal / no-progress-detection stays client-side-only.**
Porting `stackRun.ts`'s `acceptance`/`noProgressLimit` machinery
(lines 86-104) to the server-side `ChainScheduleManager` is real future
work, deliberately deferred — the current server chain always runs a fixed
number of passes (in practice, one full pass per fire), it does not re-run
toward a goal the way the client-side sequencer can.

**Also flagged: the parity audit's "Orphaned (backend-only)" row is a
candidate cleanup sprint**, not fixed this session (out of scope for a
scheduling/popover sprint). `web/src/lib/api.ts:633-640`'s comment claiming
`/api/agents/health/summary`, `/api/audit`, `/api/patterns`,
`/api/quality/trend`, `/api/tools*` "serve the native macOS admin panels" is
stale — `LEDGER.md`'s `macOS-Parity-Cut-1` entry removed those exact panels
from macOS too, so these routes (plus `/api/health`, `/api/spec`,
`/api/plans`, `/api/routing/q-values`, `/api/agents/:id/dag`,
`/api/agents/:id/checkpoint`, `/api/agents/:id/rate-limit`,
`/api/cache/agent/:agent`, `/api/tasks/:id/logs`, `/api/tasks/:id/stream`)
are genuinely dead on both platforms today. A cleanup sprint should either
delete them or wire a real caller — not leave the stale comment standing.

---

# Next Session — after the iOS-Research-1 spike / kill-test harness prep / eval-enforcement brief

This sprint prepared three things and closed none of them — by design, per
its own scope (see `LEDGER.md` for the full reasoning). Three concrete items
carried forward, none resolved here:

1. **Phase 1 needs its M3 compile pass.** `packages/LopiStacksKit/` (the
   `Stacks/` domain layer, extracted per `docs/ops/IOS_RESEARCH_1_SPIKE.md`)
   was written and grep-verified, never compiled — this host has no Xcode.
   Run `xcodegen generate && xcodebuild -scheme Lopi build`, then
   `cd packages/LopiStacksKit && swift test` (the 60 ported assertions are
   the acceptance bar, same as Verify-4). Expect at least one real gap:
   `CardOrbState.swift` stayed in the app target because it transitively
   depends on SwiftUI-importing `Store/` types — that's a flagged design
   question for a future sprint, not something the M3 pass needs to solve.
2. **Run the Phase 2 kill-test harness on real hardware.** MAXX kill tests
   1–2 are still open (unchanged from before this sprint) — instrumenting
   the actual `rate_limit_event` cadence needs a real `lopi run` session with
   real Claude Code auth across low/mid/high utilization, which no sandboxed
   session can do. The harness is built and unit-tested
   (`crates/lopi-agent/src/quota_kill_log.rs`); running it is
   `bash .konjo/scripts/quota-kill-test-log.sh --goal "..." --repo <clone>`
   (never the repo you're editing — see the standing `GitManager` guidance
   below). Read the resulting NDJSON log per the script's own printed
   instructions to answer kill test 1 (threshold-gated vs. every-turn) and
   kill test 1's second question (`resetsAt` reliability for both window
   types).
3. **Wes decides on eval-enforcement from `docs/ops/EVAL_ENFORCEMENT_DECISION.md`.**
   The real finding: "does the evaluator land server-side" is already
   answered (yes, since A1/A3 — server and web both apply/send
   `acceptance`/`budget_tokens` today; only macOS drops them, a bug, not a
   scope gap, already flagged as its own follow-up task). The actual open
   question is whether acceptance should stay purely opt-in (today's
   behavior) or become enforced by default/for specific dispatch paths — the
   doc lays out three framings without recommending one.

---

# Next Session — after MAXX (Phase 0–2)

MAXX (opportunistic backlog dispatch, gated on quota headroom) landed all
three phases in one sprint: `resets_at` plumbing + `QuotaTracker` +
`GET /api/quota` (Phase 0), `MaxxEntry`/`/api/maxx`/`MaxxLoop` tick
(Phase 1), and the cardbar button + `MaxxPopover` (Phase 2), built to the
locked design mockup. `0.7.0` → `0.10.0` (see `LEDGER.md` for why the jump).

**The one real gap: kill tests 1–3 were never run.** They call for
instrumenting a live `lopi run` session with real Claude Code auth across
low/mid/high utilization — logging every NDJSON line type across a session to
learn (1) whether `rate_limit_event` fires every turn or only past a
threshold, (2) whether `resetsAt` is reliably present for both `five_hour`
and `seven_day`, and (3) if kill test 1 shows threshold-gating, the real
USD/token cost of a canary probe. None of that is answerable without a real
account and real billed turns — not something a sandboxed session can do.

**What this means concretely:**

- `maxx_loop.rs`'s thresholds (`HEADROOM_UTILIZATION_MAX = 0.5`,
  `HEADROOM_RESET_WITHIN_SECS = 2h`) are reasoned defaults, never validated
  against a real quota timeline. They may be too loose, too tight, or
  checking the wrong thing entirely once real `rate_limit_event` behavior is
  known.
- **If kill test 1 shows the event is threshold-gated** (only fires past
  `surpassedThreshold`, e.g. 0.75), `QuotaTracker` has *no signal at all*
  while quota is comfortably low — exactly the state `headroom_favorable`
  needs to detect "high headroom." The canary-probe fallback the sprint
  brief flags becomes load-bearing, not an edge case, and isn't built yet
  (deliberately — building an unvalidated probe that spends real quota to
  answer a question kill test 1 was supposed to answer first would have been
  backwards).
- The design degrades safely either way — no observation or missing
  `resets_at` is always "don't dispatch" — but "safe" isn't the same as
  "useful." A MAXX entry might simply never fire in practice if kill test 1's
  answer turns out to be threshold-gated and no canary probe exists to work
  around it.

**Before enabling MAXX for anyone beyond an explicit opt-in tester:**

1. Run the three kill tests on real hardware with real Claude Code auth
   (the sprint brief's Pre-flight section has the exact protocol). Log every
   `rate_limit_event` line across a session spanning low/mid/high
   utilization.
2. If threshold-gated: measure real canary-probe cost, then decide whether
   to build it (only if "genuinely negligible" — otherwise the sprint
   brief's own fallback is "off by default, staleness means don't dispatch,"
   which is already what's shipped).
3. Re-tune `HEADROOM_UTILIZATION_MAX`/`HEADROOM_RESET_WITHIN_SECS` against
   real observed timelines, not the reasoned guesses currently in place.
4. Only then consider exposing the quiet-hours/headroom-gate fields as
   editable in `MaxxPopover` (currently fixed defaults, per the locked
   Phase 2 spec) — no point building that UI before the underlying signal
   is trusted.

**Explicitly out of scope for MAXX still** (per the sprint brief, not
revisited): quota-gated cron scheduling on `SchedulePopover`; Budget Modes;
wiring `Priority` into actual queue dequeue order; multi-account quota
tracking; backlog reprioritization/bin-packing in the tick.

---

# Next Session — after Creation-Flow-1 (macOS)

Both halves of Creation-Flow-1 have landed: `[0.6.0]` (web) and `[0.7.0]`
(macOS) each replaced their composer with a live draft `StackCard` + a sectioned
templates control. The models are 1:1; the tests are literal ports.

**The one real limitation left: web and macOS keep *separate* template
libraries and do not sync.** Web persists to `localStorage`, macOS to
`UserDefaults`, both under `lopi.templates.v1` with the same JSON shape — but
they are two physical stores that never talk. A template saved on one surface is
invisible on the other, and neither survives moving to a new machine/browser.

**Next sprint (only if the need is real): backend template persistence/sync.**
So a user's template library follows them across machines and surfaces. This
needs a real backend — a `templates` table + REST endpoints (`GET/POST/DELETE`),
a scope decision (per-user vs. per-repo vs. global), and both clients switched
from their local store to the API with an offline fallback. Do not build this
until the cross-machine need is real — client-only was the deliberate, honest
choice for the creation-flow sprints, not an oversight.

---

# Next Session — after Verify-4

Verify-4 (addendum in `docs/ops/LIVE_UI_STATUS_FINAL.md`) closed the macOS Loop
Stacks loop the way every prior round did: **compile first, trust nothing until
built.** The `macOS-Loop-Stacks-1` code (PR #84, `[0.4.0]`) — 4,354 lines authored
on a Linux host that can't build Swift — was compiled for the first time on the M3,
attended, with real `claude` CLI runs (no mocks, no `?demo=1`).

**Result: macOS Loop Stacks is genuinely confirmed, not shipped-on-faith.**

- **Phase 0 (build):** one real defect, one root cause (`SchedulePopoverView.swift:109`
  `$0` closure-capture) → fixed → clean build, zero warnings suppressed.
- **Phase 1 (tests):** one compile-gap (`StackRunTests` `Mock` not `@MainActor`) →
  fixed → **60/60 pass**, zero behavioral drift in the ported assertions.
- **Phase 2:** single-card regression held — bare pane is the old Forge pane, launches
  identically.
- **Phase 3:** connector insert-between, all four popovers, goal toggle + stop-reason
  banner, run-until-goal halt (`goalMet`). **Every WIRED `CreateTaskBody` field
  confirmed on the wire** (`max_iterations`=26/`on_fail`=Continue/`gate`/`until`/
  `client_ref`); `budget_tokens`+`acceptance` confirmed absent; evals confirmed
  client-only (chain acceptance runs as a spawned `s1::stack-eval::0` verify task).
- **Phase 4:** two simultaneous multi-card stacks, **zero cross-talk** (distinct
  branches, distinct per-stack `client_ref`s, divergent mid-run progress, independent
  completion).

The two Phase 0/1 fixes are committed as a `[0.4.0]` correction (CHANGELOG, not a
silent amendment). **There are no open product findings left.**

## 1. Land Verify-4 (housekeeping)

- Branch `docs/verify-4-loop-stacks` off `origin/main` (`9edca88`): the two Swift
  first-compile fixes + the Verify-4 addendum + `docs/screenshots/verify-4/`.
  Open the PR; Wall-2/Wall-3 gate as usual.
- **Standing guidance baked in from a process finding:** never point a live run at
  the repo you're editing — lopi's `GitManager` checks out `lopi/<taskid>-attempt-N`
  branches in the backend's cwd and `git clean`s untracked files. Run `lopi sail`
  from a throwaway clone for attended macOS runs (Verify-4 did this after the first
  run hijacked the working tree).

## 2. iOS-Research-1 — the next real work

With the audit chain (Verify-1 → Fix-2 → Verify-2 → Fix-3 → macOS-Loop-Stacks-1 →
Verify-4) fully closed, **iOS-Research-1** is next. Its open **R-1 package question**
(extract `macos/Lopi/Stacks/` into a shared Swift package for iOS reuse) is now
**cheaper to answer**: Verify-4 proved the `Stacks/` domain layer compiles and
passes its 55 ported assertions with **zero SwiftUI/AppKit imports** — the
prerequisite that makes a shared-package extraction "a move, not a rewrite"
(`[0.4.0]` Phase-1 note) is empirically confirmed, not just claimed. Scope the
extraction spike against that now-verified boundary.

## 3. Decisions already closed (do not re-litigate)

- macOS-visual parity confirmed on the real display (Verify-2).
- macOS stats/cost parity confirmed (Fix-3; its live backend was re-exercised during
  the Verify-4 session).
- **macOS Loop Stacks confirmed end-to-end (Verify-4)** — build, ported tests, bare
  regression, live multi-card stack, dual-stack concurrency. WIRED = the five
  `CreateTaskBody` loop fields; `budget_tokens`/`acceptance` intentionally unwired;
  evals is client-only intent (spawned verify task, never a wired field).
- Bare-pane launch uses `paneSubmitPayload`; the `Stacks/` domain layer is
  UI-framework-free (proven compilable + tested), ready for `iOS-Research-1`'s
  shared-package question.
