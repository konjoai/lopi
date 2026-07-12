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
