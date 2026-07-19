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
2. **macOS still speaks the old `/`-prefixed grammar — genuine, unclosed
   platform divergence.** `StackCardView.swift`/`StackControlDockView.swift`
   (and whatever `LopiStacksKit` Swift file mirrors `stack.ts`'s
   `CARD_COMMANDS`/`commandAutocomplete`/`detectPendingCommand`) were not
   touched this session — the sprint brief scoped every referenced file to
   web, and this environment has no Xcode to compile-verify a Swift change
   against (same standing constraint prior sessions hit). The divergence is
   cosmetic, not functional — each platform's composer only ever parses its
   own locally-typed text into the same `card.config` wire fields, so
   behavior is identical, only the shortcut *text* a user types differs by
   platform. Port the identical `/` → `;` rename (plus the `/loop/N` removal)
   to the Swift side once a session with real Xcode access is available;
   `stack.test.ts`'s new kill-test-1 table (`;model/sonnet`, `;effort/high`,
   `;branch/main`, `;autonomy/L2`, `;eval/kcqf`) is the literal acceptance
   bar to port over as Swift assertions.

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
