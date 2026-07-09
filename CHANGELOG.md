# Changelog

## [0.2.3] — Eval-Execution-1 (A1): the Konjo Verifier becomes a tiered eval executor 🎯

Promotes the working, probe-validated Konjo Verifier from a finalize-gate
double-check into a **tiered eval executor** that scores a loop against an
explicit, machine-checkable goal — and closes the verifier's fail-open hole.
Builds on Research-1 (PR #69). This is *promote + harden*, not greenfield: the
judge is reused verbatim.

### Added
- **The goal/acceptance object** (`lopi-core::acceptance` — cross-cutting seam #1): one `Acceptance { checks: Vec<AcceptanceCheck> }` schema usable at loop *and* stack scope. Each `AcceptanceCheck` is `{ tier, spec, weight, required }`; `EvalTier` (`ExecutionOk`/`ShellTest`/`Judge`/`Suite`) serializes to the UI's exact `base`/`test`/`judge`/`suite` union so the inert `EvalRef` tags become the authoring surface. `CheckSpec` carries the tier payload (`ExecutionOk` | `Shell{cmd}` | `Judge{rubric, metric}` | `Suite{name}`), with an objective `MetricGate{name, op, threshold}` for gates like `coverage >= 0.8`. Added `Task.acceptance: Option<Acceptance>` (`None` ⇒ legacy `score.passed()` gate, unchanged for every existing task).
- **The one eval-result object** (`lopi-core::eval_outcome` — seam #3): `EvalOutcome { verdict, score, per_check, critique }`, designed now for its three future consumers — A2 reflection reads `critique`, A3 ratchet reads the weighted scalar `score`, A3/B1 termination reads `verdict` + the persisted trajectory. `Verdict` is `Pass`/`Fail`/`Error` where **`Error` is an explicit not-passing state** (fail-closed). Aggregation is fail-closed: any required `Error` ⇒ `Error`; else any required `Fail` ⇒ `Fail`; non-required checks feed only score + critique.
- **The pluggable evaluator interface + tiered executor** (`lopi-agent::eval` — seam #2): one `TierEvaluator` trait with four impls behind a `TieredEvaluator` that runs checks cheapest-tier-first and **short-circuits on the first required failure before paying for the judge** (the objective-to-deterministic routing rule). `JudgeEval` delegates to a pluggable `Judge` whose production impl `VerifierJudge` wraps the existing `VerifierAgent` verbatim; `ExecutionOkEval`/`ShellTestEval` are the deterministic floor; `SuiteEval` is a thin KCQF wrapper. Every tier is fail-closed.
- **Score-history persistence** (`lopi-memory` `eval_outcomes` table + `store::eval_outcomes` — seam #4): `save_eval_outcome`, `load_eval_outcomes`, and a new `score_trajectory(task_id)` query (the progress signal A3's ratchet/no-progress and B1's stack termination read — previously the raw rows existed but no query surfaced the trajectory).
- **The committed 24-fixture regression suite** (`crates/lopi-agent/tests/eval_regression.rs` + `tests/fixtures/eval_regression.json`): the Research-1 probe's throwaway fixtures (real pass/fail + the 7 gaming patterns) are now a durable, **CI-hard-gated** safety net (`konjo-gate.yml` G2, no `continue-on-error`). Proves the executor scores all 24 correctly, routes objective failures away from the judge (0 judge calls when the deterministic floor can decide), and catches every gaming pattern.
- **A1 wiring for the client eval UI** (`web/src/lib/stores/stack.ts::evalsToAcceptance` + `api.ts` `Acceptance` types): a card's `evals` checklist now compiles into a real `Acceptance` on the outgoing `CreateTaskOptions` — `base`/`test` collapse into one deterministic `execution_ok` check, `judge` evals fold into one judge rubric, each `suite` eval becomes a suite check. Evals stop being intent-only. Backend `CreateTaskRequest` gained `acceptance` + `verifier_fail_open`.

### Changed
- **The fail-open hole is closed (Phase 0, BLOCKING).** A verifier API/parse error no longer returns `true` ("proceed to commit") — it records a not-passing ERROR verdict and **blocks finalize** (`verifier_runner::verifier_error_proceeds`, fail-closed by default). The tiered executor is wired into `finalize` *before* the autonomy verifier gate: a non-passing `EvalOutcome` rolls back, routes its critique into the next attempt's constraints (exactly like the verifier's fix-hints), and retries. Additive — a task with no acceptance is untouched and the existing verifier critique-routing still fires.
- Operators can opt a low-trust loop back into fail-open with the new `Task.verifier_fail_open` (default `false` = fail-closed).

### Notes — the four settled seams + the honest boundary
- **Seams settled once for A1→A2→A3→B1:** (1) one `Acceptance` schema, (2) one `TierEvaluator` interface, (3) one `EvalOutcome` result, (4) score-history in SQLite. A2/A3/B1 consume these without re-litigating them.
- **Objective-to-deterministic routing rule:** a criterion that can be made machine-checkable routes to a deterministic tier / `MetricGate`, never the judge — cheaper and un-gameable. Asserted by the regression suite.
- **Input-completeness is a permanent design constraint, stated honestly:** the judge catches only gaming *visible in the inputs it is handed*. A1 passes the **full** diff into `EvalContext` (the executor is no longer the truncation point) and a missing metric reading fails closed, but the verifier's own documented internal bound remains the judgment ceiling. Anyone adding a judge eval must ensure the signal to catch the gaming is in the inputs — or make the criterion objective.

## [0.2.2] — Stack-1: stack-level controls + the purple stack control dock 🟣

### Added
- **Stack-level config** (`stores/stack.ts`'s new `StackConfig`, one per pane): `loopCount` (chain repeat count, `0` = ∞, reusing the exact `stepMaxIterations`/`maxIterationsLabel` sentinel the per-loop iteration pill already used), `scheduled`/`cron` (whole-chain cron — STUBBED, see Fixed/Notes), `guardrails: StackGuardrails` (`onFail` + `budget` — no `gate`/`until` at chain scope, see Notes), `evals` (chain-acceptance checklist — CLIENT-ONLY), and `defaults: StackDefaults` (model/effort/repo/branch/autonomy — WIRED). `stores/stackDefaults.ts`'s single app-wide `writable` is gone; every pane now carries its own `config.defaults` object.
- **Stack-level ops** (`duplicateStack`/`reorderStacks`/`moveStackBeforeOrAfter`/`deleteStack` in `stores/stack.ts`, none of which existed before this sprint — `panes` was a fixed two-element array with no pane-level ops at all): pure, unit-tested, isolated per pane. `duplicateStack` clones a pane's title/config/cards with fresh ids and reset run state; `deleteStack` refuses to empty the last remaining pane (no pane-creation affordance exists yet to recover).
- **`StackControlDock.svelte`** — the purple stack control area at the base of every pane, matching `docs/ui/lopi-stack-control-area.html`'s settled "collapsible dock" option (shipped default): STACK chip, header row (chip + hide-when-expanded summary + collapse chevron) always visible, controls expand in the middle, full-width **run stack** button pinned at the bottom in both states. Reuses the exact per-loop controls — `Popover.svelte` (gained a `'config'` kind), the iteration-pill stepper, and generalized `SchedulePopover`/`GuardrailsPopover`/`EvalsPopover` (now value+callback props instead of `card`/`paneKey`, so the same components mount scoped to one loop or the whole stack) — plus a new `StackConfigPopover.svelte` (`Dropdown.svelte` × 5, editing the stack's own defaults directly). Copy/drag/delete wire to the Phase 1 stack ops; drag-to-reorder mirrors `StackCard.svelte`'s within-pane card drag one level up (`stores/stacks/dnd.ts`'s new `draggingPane`). The sticky-to-bottom placement mode from the mockup ships as unused, always-compiled CSS behind `stores/stack.ts::STACK_CONTROL_MODE` (`'dock' | 'sticky'`, currently `'dock'`) — the exact `SIDEBAR_MODE` precedent from Shell-1: flipping the one constant later is the whole migration.
- **Chain loop + chain on-fail** (`stores/stackRun.ts`): `runStack` snapshots `loopTarget`/`onFail` from the pane's `config` at launch (same reasoning as the existing `order` snapshot); `advance()` repeats the same execution order `loopTarget` times (`0` = ∞, always pause/drain-checked between cards so an infinite chain can never spin past a user's pause/drain request). Chain-level `onFail` reuses the per-loop `OnFail` vocabulary, reinterpreted at chain scope: `stop` halts the whole chain immediately (the pre-Stack-1 hardcoded behavior, now the explicit default); `continue` skips past a failed card to the next one in the same pass; `backoff` ends the current pass early but still attempts the next repetition. A chain that pressed on past a failure still settles as `phase: 'error'` overall (`hadFailure`), never silently reports `'done'`.
- `web/src/lib/stores/options.ts` — the pure, static option catalogs (`Option`/`MODEL_OPTIONS`/`EFFORT_OPTIONS`/`PRIORITY_OPTIONS`/`labelFor`) split out of `controls.ts`, which `controls.ts` now re-exports verbatim for every pre-existing call site.

### Changed
- **Precedence rule (decide-and-document):** a loop's own `model`/`effort`/`repo`/`branch`/`autonomy` override its stack's default, which falls back to the app-wide baseline: `loop ?? stack.default ?? DEF`. `cardToTaskPayload`'s pre-existing `card.config.field ?? defaults.field` resolution already *was* this rule structurally (a stack's `defaults` is always a concrete object, never "unset") — Stack-1 made the fallback source per-pane instead of a single global store, and added a table-driven test proving a loop override beats its stack default and an unset loop inherits it.
- **Precedence rule #2:** while a stack's own schedule is on, or its loop-count isn't `×1`, it governs the chain as a unit — a card's own `scheduled` cron is rendered as inert ("governed by stack — won't fire on its own") rather than actively firing, in both `StackCard.svelte`'s summary line/cardbar button and `StackConnector.svelte`'s cadence badge. Pure predicate: `perLoopScheduleGoverned`.
- `/stacks` dropped its single global "Pane defaults" selector row — each pane edits its own defaults via its control dock's config popover instead.

### Fixed
- Nothing broken; the fix-shaped item this sprint is architectural: `stores/stackDefaults.ts` used to import `MODEL_OPTIONS` from `controls.ts`, which imports `$app/environment` — invisible in the browser, but the moment `stores/stack.ts` needed a stack-default factory (this sprint), that chain would have broken `stack.test.ts`'s plain-`tsx` run (`$app/environment` only resolves inside a Vite build). Splitting the pure catalogs into `options.ts` (see Added) keeps `stack.ts` — and everything that imports it — tsx-testable, same reasoning `stackRun.ts`'s own doc comment already documents for why it takes `statusSource` as a parameter instead of importing `./agents` directly.

### Notes — WIRED vs CLIENT-ONLY vs STUBBED (this sprint)
- **WIRED:** stack config defaults (resolved into every loop's real `CreateTaskOptions` at the payload step); chain loop-count + chain on-fail (real client-sequencer behavior); "run stack" (already real via Backend-1's sequencer).
- **CLIENT-ONLY, honestly inert:** stack evals (chain-acceptance intent only — eval execution doesn't exist anywhere yet); stack guardrails' `budget` (unenforced, same as the per-loop budget decision).
- **STUBBED:** stack schedule (whole-chain cron) — editable and stored, never calls `createSchedule`/fires anything; `scheduleStack` (Backend-1) can only ever attach one cron to one card server-side (`ScheduleBody.goal: String`, no multi-goal pipeline), so a real whole-chain cron needs backend work this sprint didn't do. The dock shows an explicit "not yet enforced" hint whenever the toggle is on, rather than looking enforced.
- Chain guardrails deliberately have no `gate`/`until` fields (only `onFail`/`budget`) — there is no server-side "whole client-side stack" for a shell precondition/exit-condition to run against, so those two fields simply don't exist at chain scope rather than being rendered as would-be-inert controls.

## [0.2.1] — Shell-1: Loop Stacks as default view, off-canvas sidebar 🍔

### Added
- `docs/ui/lopi-app-shell.html` — the settled visual target, fully-hidden variant (also sketches the icon-rail variant as a toggle, documenting the shape without shipping it).
- `AppSidebar.svelte` — an off-canvas left sidebar (`translateX(-100%)` when closed) with a scrim, replacing the old horizontal top-tab bar. Closes on scrim-click, `Escape`, or selecting a nav item; traps focus within the panel while open (`Tab`/`Shift+Tab` wrap); returns focus to the hamburger button on close; `inert` when closed so a keyboard user tabbing through the page can't land on off-screen links; `prefers-reduced-motion` disables the slide transition via CSS only.
- `stores/nav.ts` — `NAV_ITEMS` (the same 14 destinations the old tab bar had, mirrored in order), `isActiveRoute`/`activeNavItem`/`isImmersiveRoute` (pure, unit-tested — 19 assertions in `nav.test.ts`), a shared `sidebarOpen` store, and the `SIDEBAR_MODE: 'hidden' | 'rail'` constant that gates the closed style — flipping it to `'rail'` is the entire migration to a persistent icon strip, no rebuild, since the rail CSS already ships (just unused while `'hidden'`).
- `$lib/components/icons.ts` — the sidebar's own icon set (hamburger, close, and one glyph per destination). Deliberately separate from `stacks/icons.ts`, which is a feature-scoped catalog, not shared chrome.

### Changed
- **Loop Stacks (`/stacks`) is now the app's default view.** `/` redirects there via a `+page.ts` `load()` (reversible — delete the file to restore the old default). Forge (the old `/`) moved to `/forge`, a purely mechanical relocation of its 5-line wrapper page — zero content changes, confirmed by diff (no route's internal page file changed except the move itself).
- `+layout.svelte`'s topbar lost its horizontal tab bar and gained a hamburger button (`aria-label="Toggle navigation"`, `aria-expanded`) that toggles `stores/nav.ts::sidebarOpen`. The "Add pane" button's `pathname === '/'` check became `pathname.startsWith('/forge')` to keep firing on the same page, just at its new address.
- `app.html`'s static `<title>`/description no longer hardcode "Forge" — they were never route-aware to begin with (this is a client-rendered SPA shell, not per-page SSR metadata), so a Forge-specific title stopped being accurate the moment Forge stopped being the default page.

### Notes
- No page's internal behavior changed — verified by `git diff --stat` scoped to `web/src/routes/` excluding exactly the four touched files (`+layout.svelte`, the root `+page.svelte`/`+page.ts`, and Forge's moved `+page.svelte`): empty diff.
- Manually verified against a built `vite preview`: `/` lands on `/stacks`; the sidebar's bounding box is off-screen (`x: -250`) on load; hamburger/scrim/Esc/nav-item-click all open or close it correctly; clicking "Loop" both navigates and closes the sidebar; `prefers-reduced-motion` collapses the transition duration to effectively `0`.

## [0.2.0] — Backend-1: stack execution, control signals, event routing 🔌

### Added
- `stores/stackRun.ts` — the client-side stack-run sequencer. `runStack(paneKey, intent, defaults, statusSource)` launches a pane's cards bottom-to-top via the real `createTask`, waiting on each one's terminal `AgentState.status` through the app's existing `agents` store before launching the next. `pauseStack`/`resumeStack`/`drainStack`/`bumpCard` are a pure client-side control-signal layer — no pool/runner changes, since there's no server-side "stack" concept to interrupt. `scheduleStack` wires "Schedule stack" honestly-minimally: one cron on the bottom-of-stack card only, reporting every other card back as `skippedCardIds` rather than faking a multi-goal schedule.
- `RunMenu.svelte` is now genuinely wired: Run now/Run once/Schedule stack/Dry run when idle, Pause/Resume + Drain once a run is active. `StackPane.svelte`'s run-stack button doubles as a pause/resume toggle and shows a dismissible error/dry-run-result banner.
- `crates/lopi-ui/src/web/task_stream_tests.rs` — a new integration test (`task_stream_isolates_concurrent_tasks_with_zero_cross_talk`) proving `GET /api/tasks/:id/stream`'s per-task filtering under concurrency: two simultaneous SSE subscriptions, ten interleaved events per task, cross-talk count asserted at `0` in both directions.
- `Task`/`CreateTaskRequest`/`CreateTaskResponse` gained `client_ref: Option<String>` — an opaque caller-supplied id (a stack card's own id) echoed back verbatim and persisted alongside the task, so a client can durably associate its own concept of "what asked for this" with the `TaskId` the pool assigns, independent of any server-side dedup. `api.ts::effectiveTaskId(resp)` resolves `duplicate_of ?? id`, the id a caller should actually track.
- `web/src/lib/stores/stackRun.test.ts` — 26 tests covering execution ordering, halt-on-failure, pause/resume, drain (non-resumable), bump (+ its illegal-transition rejections), and schedule-stack, all against a mocked `fetch` and a fake status store (no new test-runner dependency).

### Fixed
- `api.ts::createTask` no longer sends an empty `repo` as `""` — it's omitted from the request body entirely so the server's `Option<String>` falls back to its own configured repo, instead of failing outright trying to open a git repo at an empty path. This was a 100%-reproducible failure for every stack run (and the pre-existing Tasks page) until a user manually picked a non-default repo; caught only by manually running a stack against a live `lopi sail` instance, not by any mocked test.
- CI (`konjo-gate.yml`): the Wall-3 "fail if BLOCKER verdict" step now actually hard-fails (was `continue-on-error: true` with an `!= '0'` condition that never matched a real blocker exit code); the `konjo-gate` summary job's `needs:` list now includes `mutation`/`review`, which it previously omitted — both gates could fail outright without blocking merge. The remaining 9 soft-fail steps each got a one-line justification + `TODO` instead of being silently left as-is; none were reintroduced or newly softened.
- `StackConnector.svelte`'s budget badge is hidden (not restyled) until budget enforcement is real, per the UI-2 V&V audit's escalation.
- `test_app_with_store()` (a pre-existing, previously-uncalled test helper) never actually wired `.with_store()` into the pool, so no HTTP-created task in any test using it ever persisted; fixed as part of adding the `client_ref` round-trip tests that first exercised it.

### Notes
- Coverage gate: real workspace line coverage is 68.34% (23,355 lines found, 15,960 hit — computed by parsing `lcov.info`'s `LF:`/`LH:` directly, since `cargo llvm-cov report --json` doesn't support `--workspace` and was silently scoping to the root binary crate alone). Below the 80% floor; the gate stays soft with a `TODO` rather than blocking merge on a pre-existing gap this sprint didn't introduce.
- Out of scope this sprint (unchanged): eval execution/enforcement, budget enforcement, multi-pane/overview, effort→thinking-budget, ratchet/beats-best, severity, and a real multi-card-per-pane output surface (routing is proven; the UI is still one `StackOutput` per running card).

## [Unreleased] — UI-2 V&V: audit + coverage-gap closure 🔍

### Added
- `docs/ui/UI-2-VV-report.md` — a read-only verification pass over merged PR #64: all five hard gates evaluated with cited evidence (test names, `file:line`, computed-style checks), a **GO** for the backend phase, and two escalations (a budget badge that visually reads as enforced when nothing enforces it; pre-existing repo-wide CI soft-fail policy in `konjo-gate.yml`, unrelated to #64).
- `stack.test.ts` gained 18 tests closing three real coverage gaps the audit found: cross-pane reorder isolation (proving `applyToPaneCards`-dispatched reorder never touches another pane), a 9-row table-driven WIRED round-trip test for `cardToTaskPayload` (plus a key-completeness assertion and a standalone `until`-off test), and a "custom cron never snaps to a matching preset" test. 103 → 121 assertions; repo-wide total 426 → 444.

### Notes
- No shipped-code defects found — nothing in PR #64 needed fixing. This audit found gaps in test *coverage*, not correctness.

## [Unreleased] — UI-2: Loop Stack card controls, popovers, config drawer, live output 🃏

### Added
- `/stacks` now renders two independent panes side by side (`stores/stack.ts`'s
  new pane-keyed layer — `panes`, `insertIntoPane`/`applyToPaneCards`, the
  pre-flight gate's `stack.insert(stackKey, index, loop)`), each with its own
  composer (new prompts prepend to the top), card stack, and run-stack footer.
- New shared `Popover.svelte` primitive: floats near its trigger with a tail,
  flips above when the viewport is too short, clamps horizontally, closes on
  outside-click/Escape/scroll, collapses to a bottom sheet under 520px, and
  keeps only one popover open at a time app-wide.
- `StackCard.svelte` rewritten: runtag (idle/queued/running/done), alias chip,
  iteration bar, hide-inactive summary lines (schedule/guards/evals), cardbar
  with an inline hover-expand iteration stepper, and drag-to-reorder within a
  pane (`reorderInPaneRelative`/`moveCardBeforeOrAfter`).
- `SchedulePopover.svelte` (WIRED — `cron.raw` mirrors `ScheduleEntry.cron`):
  enable toggle, frequency presets ⇄ raw-cron two-way sync, a new `Combo.svelte`
  type-or-pick numeric input for hour/minute, and a real bounded cron simulator
  (`computeNextRuns`) driving the next-runs footer.
- `GuardrailsPopover.svelte` (WIRED — `gate`/`until`/`onFail` map onto
  `CreateTaskOptions`): gate/until toggles + shell inputs, on-fail segmented
  control, budget segmented control (client-only), and the max-iterations
  stepper shared with the cardbar's iteration pill.
- `EvalsPopover.svelte` (client-only, per the brief's honesty rule — no eval
  execution exists server-side): flat checklist over the full `EVAL_CATALOG`
  with tier badges, baseline locked-on, and KCQF/security/research suite
  shortcuts.
- `ConfigDrawer.svelte`: five `Dropdown.svelte`-based selectors
  (model/effort/repo/branch/autonomy) overriding pane defaults; model/effort/
  repo are WIRED, branch/autonomy stay client-only.
- `StackConnector.svelte`: dotted cyan cadence badge when the card above is
  scheduled, sun budget badge otherwise, hover-reveal insert-between block.
- `StackOutput.svelte`: live output attachment for the single running card,
  genuinely wired to `stores/transcript.ts`'s existing per-`task_id` block
  feed (thinking/tools/status/assistant_text → thinking/tools/actions/output),
  collapsed by default, 5s orange flash on the combined running card + output
  block (respects `prefers-reduced-motion`).
- `RunMenu.svelte` (stub — Run now/Run once/Schedule stack/Dry run all
  no-op, `// TODO(backend)`): opens/closes off the pane footer's chevron.
- `stores/stack.ts::cardToTaskPayload` — a pure, unit-tested mapping from a
  card's guardrails/config onto the real `createTask(goal, repo, priority,
  options)` shape, proving the WIRED fields round-trip correctly even though
  no run-stack action calls `createTask` yet.

### Changed
- `stores/stackDefaults.ts` gained a `branch` field + `BRANCH_OPTIONS` (the
  config drawer's fifth selector).
- `StackCard.loopN` renamed to `maxIterations` throughout, matching the
  backend's `max_iterations` field name; every fresh card now starts from
  the backend default (`25`) instead of "unset."

### Removed
- `StackComposer.svelte` — superseded by each `StackPane`'s own inline
  composer (the mockup's per-pane composer, not a single shared one).

## [Unreleased] — Guardrails: Gate / Until / On-Fail 🚧

### Added
- `LoopConfig`/`Task` gain `gate: Option<String>` (precondition, must exit 0 before the loop starts), `until: Option<String>` (exit-condition, checked after each iteration — exit 0 ends the loop early as a success), and `on_fail: OnFail` (`Stop`/`Continue`/`Backoff`, default `Stop`) — all `#[serde(default)]`, no change to existing configs.
- New `lopi_core::loop_config::run_guard_command` shell-exec helper (`sh -c`, exit-status only) shared by `gate`/`until`; `Stop`/`Backoff` reuse the existing full-jitter `backoff_secs` rather than a second delay constant.
- `POST /api/tasks` now accepts `gate`/`until`/`on_fail` (mirrored in `web/src/lib/api.ts`'s `CreateTaskOptions`, types only).

## [Unreleased] — UI-1: Static Loop-Stack + Selector Row 🥞

### Added
- New `/stacks` route (existing `/loop` cockpit left untouched) — a static, in-memory loop-stack composer: fused type-first/preset-grid/inline-grammar creation flow, read-only prompt cards (preset pill, spec line, guardrails/evals summary lines, static UI-2 placeholder buttons), and a stack-defaults selector row (model/effort/repo/autonomy) reusing `Dropdown.svelte` + `controls.ts`.
- `stores/stack.ts` — pure, unit-tested ordered-array ops (add/remove/duplicate/reorder/insert) plus the `:alias "goal" @repo xN` composer grammar parser and the 5-preset client-side eval-suite catalog.
- `api.ts` gains `listRepos()` (`GET /api/repos`), now consumed by the stack composer's repo dropdown.

## [Unreleased] — Git hygiene: fix committed DRY violations 🧹

### Fixed
- `dry_check.py`: 794 → 12 raw window-matches (46 → 4 file pairs, 3 documented reasons). Extracted
  shared helpers across `lopi-agent`, `lopi-context`, `lopi-core`, `lopi-git`,
  `lopi-orchestrator`, `lopi-remote`/`lopi-ui` (a real security-relevant
  `constant_time_eq` unification), `lopi-spec`, `lopi-toon`, `lopi-ui`'s test
  suite, and 9 web `*.test.ts` files. 3 remaining pairs are documented,
  justified residuals (structural unit/integration-test split, generic sqlx
  boilerplate, axum test-module preamble) — see `LEDGER.md`.

## [Unreleased] — Sprint 5: Expose Loop Fields on `CreateTaskRequest` 🌉

### Added
- `POST /api/tasks` now accepts `verifier_required`/`verifier_model`/`verifier_effort`,
  `report` (validated via the existing `ReportChannel::parse`), `max_iterations`
  (`0` = infinite, a new sentinel), and new `Task.model`/`Task.effort` overrides
  — all optional, `#[serde(default)]`, no change when omitted.
- `select_model` and pool `build_runner` now honor `Task.model`/`max_iterations`
  as explicit overrides over the heuristic/repo `LoopConfig` default.

## [Unreleased] — Sprint 4: Verifier as Explicit Gate 🔬

### Added

**Verifier as Explicit Gate** (`lopi-core`, `lopi-agent`, `lopi-orchestrator`)
- **`LoopConfig`/`Task` gain `verifier_required: bool`, `verifier_model: Option<String>`,
  `verifier_effort: Option<String>`** — a per-loop "require the Konjo Verifier"
  toggle independent of `autonomy_level`, `#[serde(default)]` and round-trip-safe.
- **`VerifierAgent::verify` is parameterized** (`model: &str, effort: Option<&str>`)
  instead of hardcoding `MODEL_OPUS`; a new pure `resolve_verifier` picks a
  model that differs from the worker's when `verifier_model` is unset
  ("never grade your own homework").
- **Pool construction now calls `.with_verifier()`** — its first real call site
  ever — when `verifier_required` or `verifier_model` is set on the task.

## [Unreleased] — Sprint 3: Report on Finish 📣

### Added

**Report on Finish** (`lopi-core`, `lopi-agent`, `lopi-remote`)
- **`ScheduleEntry::report` / `Task::report`** (`Option<String>`) — declare a
  channel (only `"telegram"` reachable today) a completed run's summary is
  routed to; validated loudly via `ReportChannel::parse` (`lopi-core`) at
  config-load time, never a silent no-op.
- **`AgentEvent::ReportReady`** — the L1 `emit_report` hook now broadcasts this
  over the existing `EventBus<AgentEvent>` when a channel is declared;
  `lopi-remote`'s Telegram notifier delivers it via the existing `send_msg`.
  Zero new crate dependencies — both sides already depended on `lopi-core`.

## [Unreleased] — Sprint 2: Skill Arguments 🎯

### Added

**Skill Arguments** (`lopi-skill`)
- **`Skill::render_body(&self, args: &str)`** — substitutes `$ARGUMENTS` by
  routing through Sprint 1's `lopi_core::resolve_template` (one `{arguments}`
  hole, one-entry vars map) — no second substitution layer.
- **`lopi_skill::parse_invocation(":name args")`** — pure prefix parser;
  wired at the CLI's `lopi run --goal` boundary (`resolve_skill_invocation`)
  so `:kcqf vectro` resolves to the named skill's rendered body before
  `Task::new` ever sees it. Unknown skill names fail loudly, never pass
  through as a literal goal. Telegram ingestion untouched this sprint.

## [Unreleased] — Sprint 1: Prompt Templates 🧩

### Added

**Prompt Templates** (`lopi-core`)
- **`lopi_core::template::resolve(template, vars)`** — pure `{name}`-hole
  substitution resolved at enqueue time, so Claude only ever sees the final
  literal string; `{{`/`}}` escape to literal braces, an unfilled hole is a
  loud `TemplateError::UnresolvedVariable`, never a silent passthrough.
- **`Task::from_template(template, vars)`** — the one call site wiring
  templates into task creation; `Task::new` is untouched and stays the default.

## [Unreleased] — Sprint U: DAG-Structured Retry + Time-Travel Replay 🕸️

### Added

**Loop Engineering — Phase 16.6 Per-run drill-down trace** (`lopi-memory`, `lopi-ui`, web, macOS)
- A **Recent Runs** panel on the Loop screen: each run expands an
  attempt-by-attempt trace — lifecycle stages (plan→implement→test→score),
  per-attempt pass%/lint/diff/tokens/cost, the verifier verdict (passed/
  confidence + gaps), and captured errors. Backed by
  `GET /api/loop-engineering/runs` + `/runs/:id`, projecting `attempts` +
  `turn_metrics` + `verifier_verdicts` (`lopi-memory/store/run_trace.rs`). The
  single-run counterpart to the aggregate Loop Health view.

**Loop Engineering — Phase 16.3 Loop Health observability + stall guard** (`lopi-agent`, `lopi-memory`, `lopi-ui`, web, macOS)
- **No-progress stall guard** — the loop halts early when the weighted score
  stops improving for `LoopConfig.no_progress_limit` consecutive attempts
  (design-doc gap #7), instead of burning the whole retry budget on a stuck
  loop (`update_no_progress_streak`, wired into `run_loop.rs`).
- **`GET /api/loop-engineering/health`** projects data the loop already persists
  (`attempts`, `turn_metrics`, `verifier_verdicts`) into one observability
  snapshot: headline KPIs (runs, attempts, success rate, verifier pass rate,
  spend, tokens), per-attempt score series, outcome distribution, token/cost
  burn (`lopi-memory/store/loop_health.rs`).
- **Loop Health view on both surfaces** — KPI tiles, sparklines (score/attempt,
  context pressure, diff size, cost burn), and an outcome-distribution bar,
  leading the Loop screen. Web composes `StatCard`+`Sparkline`; macOS composes
  `Charts.Sparkline`.
**Loop Engineering — Phase 16.7 Earned-Trust Auto-Promotion** (`lopi-core`, `lopi-memory`)
- **The loop now *earns* its autonomy instead of having it assigned.** A repo or
  schedule that strings together N consecutive clean, verifier-passed runs is
  promoted one rung up the L1→L4 trust ladder; a post-merge revert revokes that
  standing. This is the phased-rollout "confidence control" from the
  loop-engineering design (CSA Agentic Trust Framework, 2026), and the last
  research-ranked follow-on in `docs/LOOP_ENGINEERING.md` §6.
- **`earned_trust`** — a new pure state machine in `lopi-core`: `EarnedTrust`
  (`level` + `clean_streak`) advanced by three total, saturating transitions —
  `on_clean_run(promote_after, ceiling)` (streak++ → promote one rung at the
  threshold, capped at `ceiling`), `on_failed_run` (breaks the streak but never
  demotes — a failure simply doesn't *earn* promotion), and `on_revert(floor)`
  (the decisive "trust was misplaced" signal — demote one rung toward `floor`).
- **`AutonomyLevel`** gains `from_rank` / `promoted` / `demoted` saturating
  ladder helpers (mirroring `SelfPromptStrategy`'s rank arithmetic).
- **`LoopConfig`** gains two loop-as-code levers: `promote_after` (`0` = the
  default → auto-promotion disabled) and `trust_ceiling` (caps the climb so
  unattended auto-merge stays opt-in; defaults to `DraftPr` → no headroom until
  raised). `validate()` flags a `trust_ceiling` that sits at/below
  `autonomy_level` while `promote_after > 0` — a config where promotion can
  never fire.
- **`lopi-memory`** — a `trust_ledger` table (`scope`, `level`, `clean_streak`)
  with `load_trust` + `record_clean_run` / `record_failed_run` / `record_revert`
  that apply the pure transitions and persist; each returns the resulting level
  for the caller to seed the next run.
- **Tests** — 8 state-machine cases (streak/promote/cap/disable, failure-holds,
  revert-demotes-toward-floor), ladder-helper saturation, `LoopConfig` lever
  defaults + TOML round-trip + the unreachable-ceiling validation, and 4
  in-memory ledger persistence round-trips. Live recording wiring (schedule-id
  plumbing → `set_schedule_autonomy`), GitHub revert detection, and the web/macOS
  Loop-screen surface are the immediate follow-on.

**Loop Engineering — Phase 16.6 Token-Budget Enforcement** (`lopi-agent`, `lopi-orchestrator`)
- **The model now self-paces instead of being hard-cut.** `LoopConfig.budget_tokens`
  (already a loop-as-code lever) is wired to the Anthropic **task budget** beta
  (`output_config.task_budget`, header `task-budgets-2026-03-13`) on the direct-API
  planning path: the model sees a running countdown and finishes gracefully within
  the budget rather than being truncated mid-thought by `max_tokens`. This is the
  "critical safety adjacency" called out in `docs/LOOP_ENGINEERING.md` §6.
- **`api_budget`** — a new module holding the pure, unit-tested decision logic:
  `supports_task_budget` (the beta is **model-gated** to Opus 4.7/4.8 + Fable 5 —
  silently dropped on the Haiku/Sonnet tiers used for cheap early attempts, which
  would otherwise 400), `effective_task_budget` (resolves + **clamps** up to the
  API's 20,000-token minimum so an under-minimum config never errors), and
  `task_budget_output_config` (wire shape). `stream_plan` only forwards the result.
- **Runner** — `AgentRunner::with_task_budget(budget_tokens)` (`0` = inherit the
  global cap → no budget). Wired from `.lopi/loop.toml` in both the `lopi run` CLI
  path and the orchestrator pool, alongside the existing self-prompt levers.
- **Tests** — model-gating, none-without-request, below-minimum clamping,
  pass-through, and wire-shape unit tests for `api_budget`; runner builder tests
  for the `0 → None` / positive-`→ Some` mapping. The `stream_plan` streaming-IO
  shell is excluded from mutation testing (logic lives in the tested helpers).

**Loop Engineering — Phase 16.5 Adaptive Strategy Escalation** (`lopi-core`, `lopi-agent`, `lopi-orchestrator`, `lopi-ui`, web, macOS)
- **The loop now climbs its own ladder.** Instead of pinning one self-prompt
  strategy for a whole run, `escalate_strategy` makes the agent apply
  progressively more cognitive scaffolding the longer a task resists a fix:
  cheap `Direct` retries first, then Reflexion → Self-Refine → Plan-Then-Act.
  `SelfPromptStrategy::escalated(base, attempt)` climbs one S-rung per failed
  attempt (capped at S4, starting from the configured base) — a pure, saturating
  function. Backed by RefineCoder (arXiv:2502.09183).
- **Runner** — `AgentRunner::with_strategy_escalation` + `effective_strategy(attempt)`;
  the adaptive-retry path now frames the failure with the *effective* strategy
  for that attempt. Loaded from `.lopi/loop.toml` in the `lopi run` CLI and the
  orchestrator pool.
- **API** — `GET /api/loop-engineering` config now carries `escalate_strategy`
  and an `escalation_ladder` (attempt → strategy preview); new
  `POST /api/loop-engineering/escalation` toggles it (persisted to `.lopi/loop.toml`).
  All loop-as-code writes now share one `persist_loop_update` helper.
- **Web + macOS** — an "Adaptive escalation" switch on the Loop screen plus a live
  per-attempt ladder (`#1 S2 → #2 S3 → #3 S4 …`).
- **Tests** — pure escalation math (`from_rank`/`escalated`, saturation +
  base-relative), runner `effective_strategy` unit tests, handler ladder test,
  two HTTP e2e tests, an `api.test.ts` case; verified live against `lopi sail`.

**Loop Engineering — Phase 16.4 Self-Prompting Strategy Engine** (`lopi-core`, `lopi-agent`, `lopi-orchestrator`, `lopi-ui`, web, macOS)
- **Direct agents to prompt *themselves*.** A new `SelfPromptStrategy` (S1–S4) is
  the highest-leverage loop lever: the text the agent feeds back into its own
  next planning step after a failed attempt. `crates/lopi-core/src/self_prompt.rs`
  implements four research-backed strategies as pure `frame(base, attempt)`
  transforms:
  - **S1 Direct** — raw failure, verbatim (legacy default; byte-identical).
  - **S2 Reflexion** — name the root cause, then try a *different* approach
    (Shinn et al. 2023).
  - **S3 Self-Refine** — critique against correctness/coverage/minimality, then
    revise (Madaan et al. 2023).
  - **S4 Plan-Then-Act** — write a numbered plan before editing (Plan-and-Solve).
- **Loop-as-code, editable from the UI.** `LoopConfig` gains a `self_prompt`
  field and a `save_to_repo` writer; the new `POST /api/loop-engineering/strategy`
  validates a tag and persists it to `.lopi/loop.toml` (422 on unknown tags).
  `GET /api/loop-engineering` now carries a `self_prompt_strategies` catalog,
  each entry with a **live preview** of the self-prompt it generates.
- **Wired live into the runner.** `AgentRunner::with_self_prompt` routes the
  adaptive-retry failure block through the chosen strategy before injecting it
  into the next planning prompt — honored by both the `lopi run` CLI path and the
  orchestrator pool, loaded from `.lopi/loop.toml`.
- **Web + macOS.** A new "Self-Prompting Strategy" panel on the Loop screen:
  a picker, strategy cards (active state), and a live self-prompt preview.
- **Tests.** Pure-function strategy tests + `save_to_repo` round-trips in
  `lopi-core`; catalog/handler tests in `lopi-ui`; three HTTP-level e2e tests
  (`web/loop_tests.rs`) covering snapshot read, persisted round-trip, and the
  422 reject path; an `api.test.ts` case for the web client. Verified against a
  live `lopi sail` server end-to-end.

**Loop Engineering — Phase 16.2b runner enforcement** (`lopi-agent`, `lopi-git`)
- The **L1–L4 autonomy ladder now changes end-of-loop behavior** — previously
  `autonomy_level` was configurable and observable but ignored by the runner.
  A new shared `AgentRunner::finalize` (`crates/lopi-agent/src/runner/finalize.rs`)
  replaces both `open_pr` call sites in `run_loop.rs` (main success + post-fix
  success) and branches on `task.autonomy_level`:
  - **L1 `report_only`** — commit to the branch, log a diff/score report, return
    `Success` with `pr_url: None`. No PR is opened.
  - **L2 `draft_pr`** (default) — open a **draft** PR (the GitHub review is the
    human gate).
  - **L3 `verified_pr`** — force the Konjo verifier on (regardless of
    `verifier_enabled`) **before** opening a normal PR.
  - **L4 `auto_merge`** — verifier must pass and the score must clear the gate,
    then open a PR and **auto-merge** (`gh pr merge --auto --squash`).
- **`GitManager`** (`crates/lopi-git/src/manager.rs`) gains `open_draft_pr` and
  `auto_merge`; PR/merge argument building is factored into pure, unit-tested
  helpers. The verifier now also runs on the post-fix success path for L3/L4.
- `run_loop.rs` was split into focused modules (`finalize`, `plan_gate`,
  `plan_steps`, `seed`, `speculative`) to stay under the 500-line file gate.

**Loop Engineering — Phase 16.2 sidebar screen** (`lopi-ui`, `web/`, `macos/`)
- **`GET /api/loop-engineering`** aggregation endpoint composes one read-only
  snapshot for the primary repo: effective `.lopi/loop.toml` (with validation),
  the L1–L4 autonomy ladder, discovered skills (`.claude/skills/*/SKILL.md`) +
  rules (`.claude/rules/*.md`), live schedules with their trust level, and the
  Konjo quality-gate catalog.
- **New Loop screen on both surfaces** (web `/loop`, macOS `Loop` nav) rendering
  that snapshot in Konjo identity: Effective Config card, the colored autonomy
  ladder, scheduled loops each with a **Trust-Level dropdown** (the one writable
  control → `POST /api/schedules/:id/autonomy`), skills, rule chips, and the
  three quality-gate walls. Built in lockstep — the web and macOS screens share
  the same payload and layout.

**Loop Engineering — Phase 16.1 backend** (`lopi-core`, `lopi-memory`, `lopi-ui`, CLI)
- **`LoopConfig` + `AutonomyLevel`** (`crates/lopi-core/src/loop_config.rs`): the
  "loop as code" schema loaded from `<repo>/.lopi/loop.toml` (autonomy level,
  intent anchor, enabled skills/rules, permission policy, no-progress + iteration
  caps, per-run budget) and the L1–L4 phased-autonomy ladder (report-only →
  draft-PR → verified-PR → auto-merge) with capability gates and `validate()`.
- **Per-schedule trust level** persisted: new `autonomy_level` column on the
  `schedules` table (idempotent migration), plumbed through `ScheduleRow` /
  `ScheduleInput` / `ScheduleSpec` → `Task`, with a `set_schedule_autonomy`
  store method and a `POST /api/schedules/:id/autonomy` endpoint for the
  forthcoming Loop Engineering Trust-Level dropdown.
- **`lopi loop validate` / `lopi loop show`** CLI: validate a repo's loop config
  in CI (non-zero exit on issues) and inspect the effective values.
- Full design + the five feature options (A–E) and the build sequence are
  written up in [`docs/LOOP_ENGINEERING.md`](docs/LOOP_ENGINEERING.md) and
  catalogued as Phase 16 in [`docs/COMPETITIVE_ROADMAP.md`](docs/COMPETITIVE_ROADMAP.md).

**Forge multi-agent cockpit — web + macOS** (`web/`, `macos/`)
- **Sessions sidebar** lists every task whether mounted or not. Closing a pane
  now *parks* the session in the sidebar instead of deleting it; a dedicated
  trash action is the only permanent delete.
- **Deleted-session resurrection bug fixed.** Closing a pane and deleting a
  session were conflated, and a best-effort server `DELETE` left the snapshot
  free to re-hydrate "deleted" sessions on reload. A new layout layer
  (`web/src/lib/stores/layout*.ts`, `macos/.../Store/PaneLayout.swift`)
  separates close-pane from delete-session and tombstones deletions so the
  snapshot reducer can never bring them back. A persisted "known" set tells a
  genuinely-new task apart from a returning one, so fresh tasks still auto-open.
- **Resizable auto-tiling pane grid** (default 4): 2 = halves, 3 = thirds,
  4 = quarters, with drag-resizable column/row gutters and drag-to-reorder.
- **Model / effort / priority / repo / branch selectors** via custom Konjo
  dropdowns, persisted and wired into task submission as planning constraints.
- **macOS native Forge**: a `Canvas`-based ever-morphing fire/ice **orb**
  driven by phase/activity/pressure, the resizable grid, sidebar, and selectors
  — reaching parity with the web Forge. New `Forge` nav section is now the
  landing screen. (macOS is compile-unverified in this CI environment.)
- Pure layout algorithms are unit-tested (`layout-core.test.ts`, 32 cases).
- The `AgentEvent → AgentState` reducer is split out of `agents.ts` into
  `stores/agentReducer.ts`, bringing `agents.ts` back under the 500-line gate.

### Changed

**Sessions sidebar — drag-into-pane, filter, status grouping** (`web/.../SessionSidebar.svelte`)
- **Drag a session row directly onto a specific pane** to mount it there (real
  HTML5 DnD via `application/x-lopi-session`); the new `mountInPane` layout
  action removes it from any slot it already held, so dragging never
  duplicates a pane. Clicking a row still drops it into the first free pane.
- **Filter box** — case-insensitive match across goal / repo / branch, with a
  clear button and a "no matches" empty state.
- **Status grouping** — sessions split into sticky `active` / `done` / `failed`
  headers (newest-first within each, empty groups hidden). Pure, testable logic
  lives in `session-groups.ts` (**16 tests**); the component stays a renderer.

**Reducer test coverage** (`web/.../agentReducer.test.ts`)
- The `AgentEvent → AgentState` reducer (split into `agentReducer.ts`) shipped
  without tests. Added **28 cases** covering every variant —
  queue/start/turn-metrics/status/score-clamp/completed/verdict transitions,
  events for unknown tasks (no-op), and immutability (input map + agent never
  mutated). The extraction is what made this testable in isolation.

**Springy, interruptible tile motion** (`web/.../TileGrid.svelte`)
- Adding or removing a pane was instant. Now the surviving tiles **glide** to
  their new tracks (FLIP, 420ms `cubicOut`) while the added/removed tile
  **scales** in/out (`backOut` pop on enter). The cell list is keyed and never
  changes during a gutter drag, so the spring can't fight a live resize. The
  divider gutters ease to their new boundaries on re-flow and snap instantly
  while dragging. 60fps, interruptible, no layout thrash.
- **macOS parity** (`PaneGridView.swift`): the native grid gets the same
  behaviour via `.animation(.spring(response:0.42, dampingFraction:0.82),
  value: count)` plus a scale+opacity pane transition — keyed on `count` so a
  gutter drag never fights the spring.

### Fixed

**Forge panes never went live — reactivity bug** (`web/.../AgentGrid.svelte`)
- Panes resolved their agent through a helper called in markup
  (`agent={agentFor(index)}`). Svelte tracks an expression's dependencies
  *syntactically* — it sees `agentFor` and `index`, never the `$agents` /
  `$paneSlots` stores read **inside** the function — so the grid evaluated
  once at mount (agents still empty; mock/live data arrives ~1.5s later) and
  then froze on the idle state forever. Every pane showed "— idle —" with an
  empty ring even though the sessions sidebar (which iterates `$agents`
  directly) correctly listed every running agent, and the layout had already
  mounted them into slots. Replaced the helper with a reactive
  `$: paneAgents = $paneSlots.map(...)` derivation that names both stores, so
  panes now light up the moment an agent appears. This is what makes the Forge
  actually *live* — orbs, metrics, logs and phase all render on first paint.

**`AgentDag` execution trace** (`crates/lopi-agent/src/dag.rs`)
- Models one agent attempt as a directed acyclic graph of pipeline stages —
  `NodeKind = Plan | Implement | Test | Score | Verify | Diff | Pr`, each a
  `DagNode { kind, status, depends_on, output_hash }`.
- `canonical()` builds the linear pipeline; `resume_point()` returns the
  earliest non-`Done` node (the partial-restart entry point); `reset_from()`
  rewinds a node + downstream while preserving upstream memoized output;
  `complete_node()` / `fail_node()` / `set_status()` drive transitions;
  `edges()` exposes the graph; full serde round-trip.
- Grounded in the Scheduler-Theoretic Framework (arXiv 2604.11378): partial
  restart from failed nodes beats linear retry. 14 unit tests.

**Idempotency safeguard** (`dag.rs`) — *discovery-driven*
- `NodeKind::is_side_effecting()` (`Pr` opens a PR); `DagNode.idempotency_key`
  records the committed external effect and is **preserved across
  `reset_from`** (unlike `output_hash`); `should_execute()` skips an
  already-committed side-effecting node so a replay reuses the effect instead
  of opening a duplicate PR. Grounded in ACRFence (arXiv 2603.20625) on
  semantic-rollback hazards in agent retry.

**`agent_dag_nodes` persistence** (`lopi-memory`)
- One row per pipeline stage; `upsert_dag_node` (upsert on `(task_id, kind)`)
  + `load_dag_nodes`. Edges are derived from `depends_on`, so no redundant
  edges table. 3 tests.

**`GET /api/agents/:id/dag`** (`lopi-ui`)
- Returns `{ task_id, nodes, edges }`; edges derived from each node's
  `depends_on`. Unknown task → empty graph (200). 2 tests on the graph shaper.

**`AgentDag::from_rows` + `lopi replay`**
- `AgentDag::from_rows` (`lopi-agent/dag_rows.rs`) reconstructs a DAG from
  persisted `agent_dag_nodes`; `NodeKind` / `NodeStatus` gain `FromStr`.
- `lopi replay --task <id> [--from <node>] [--dry-run]` loads the persisted
  DAG, resolves the restart stage (explicit `--from` or the resume point), and
  prints the partial-restart plan — which stages re-run, which reuse memoized
  upstream output (♻️), and which side-effecting stages are skipped because
  their external effect already landed (⏭️, idempotency-key reuse). Read-only
  for now; live re-execution rides on the runner producer. 7 tests.

**Mutation gate** (`.cargo/mutants.toml`)
- New cargo-mutants config scoped-excluding the CLI entry point (`main`) and
  two pure-IO shells (`replay_commands::run` / `print_plan`) — they hold no
  branching logic, delegating to the unit-tested `replay_plan` / `classify` /
  `resolve_restart`. The replay plan computation is fully mutation-covered.

### Notes
- The runner producer (wiring `AgentRunner` to build/persist the DAG and emit
  `node_id` on events) and the TUI "DAG" tab follow — the producer requires
  splitting the 606-line `run_loop.rs` and live-agent validation, so it is held
  for an environment that can exercise a real run. See PLAN.md Sprint U.

---

## [Unreleased] — Sprint T: Topology-Adaptive Routing + Q-Learning 🧭

### Added

**Q-learning router** (`crates/lopi-orchestrator/src/q_router.rs`)
- `QRouter` — an epsilon-greedy contextual-bandit router over a
  `(task_type → agent_config)` Q-table. `select` explores with probability ε
  (default 0.1) and otherwise exploits the highest-valued action; `update`
  folds a normalised reward in via `Q ← Q + α·(reward − Q)` (default α = 0.5).
- `snapshot` / `hydrate` round-trip the table for persistence; `q_value`
  exposes a single cell. All inputs (ε, α, reward) are clamped to `[0, 1]`.
- 9 unit tests (update math, clamping, greedy + explore selection,
  snapshot↔hydrate, param clamping).

**`routing_q_values` table** (`lopi-memory`)
- `MemoryStore::upsert_q_value` (upsert keyed on the `(state, action)` PK) and
  `load_q_table` (most-recently-updated first). 3 tests.

**`GET /api/routing/q-values`** (`lopi-ui`)
- Returns the persisted Q-table as JSON for inspection.

**Topology classifier corpus** (`lopi-orchestrator::topology`)
- Expanded to a 30-case labelled corpus spanning all four topologies plus the
  hybrid/tie fallback.

**`Strategy::QLearned` in the constellation router** (`lopi-orchestrator::constellation`)
- New routing strategy: dispatch selects the member with the best learned
  Q-value for the constellation (state = constellation name, action = agent id),
  exploring epsilon-greedily via the shared `QRouter`.
- `ConstellationRouter::record_outcome(constellation, agent_id, reward)` feeds a
  task's quality reward back into the Q-table; `q_snapshot()` exposes it.
- `constellation.rs` (690 lines, over the 500 budget) split into
  `constellation/{mod,types,select,tests}.rs` — each well under 300 — to clear
  the file-size gate before the feature landed. Behaviour preserved; 4 new tests.

**Topology classifier wired into the dispatch path** (`lopi-orchestrator::pool`)
- `AgentPool::submit()` now fills in `Task::topology` via the keyword classifier
  when the task carries no explicit hint — logged, advisory, and never blocks
  submission. The hint flows through to the runner via `Task::topology`.
- `effective_topology(&task)` helper (explicit hint, else classify the goal).
- `pool.rs` (929 lines, well over budget) split into
  `pool/{mod,types,registry,run_loop,tests}.rs` — each ≤ 354 lines — to clear
  the file-size gate. Behaviour preserved; public API (`crate::pool::*`)
  unchanged. 3 new tests.

### Notes
- Remaining Sprint T work (`AgentPool::dispatch` topology branching, the
  `low_confidence` Haiku fallback, task-type-keyed Q-state, and the RoundRobin
  benchmark) is deferred — `pool.rs` is also over the size budget and touches
  the live agent-spawn loop. See PLAN.md Sprint T.

---

## [0.19.0] — Sprint S: Konjo Verifier + macOS app + web overhaul 🔬🖥️

### Added — Konjo Verifier (Sprint S)

**`VerifierAgent`** (`crates/lopi-agent/src/verifier.rs`)
- Rubric-guided Opus second-score pass. After the heuristic `Score` passes,
  `run_verifier_pass` sends `{goal, plan, diff, test_output, rubric}` to Opus and
  parses a `VerifierVerdict { passed, gaps, fix_hints, confidence }`.
- On rejection, `fix_hints` are appended to `Task::constraints` and the task
  retries with them as hard requirements. Verifier errors are non-fatal (the
  runner proceeds, `tracing::warn!`).
- **Rubric resolution chain:** `Task::rubric` →
  `.konjo/rubrics/feature_completeness.toml` (via `verifier::resolve_rubric` /
  `load_rubric_file`) → `default_rubric()`. `Rubric::from_toml_str` in
  `lopi-core` keeps the parse IO-free; the runner reads the file with `tokio::fs`.

**Persistence + events**
- `verifier_verdicts` SQLite table + `save_verifier_verdict` / `load_verifier_verdicts`
  (`lopi-memory/src/store/verifier.rs`).
- `AgentEvent::VerifierVerdict { task_id, passed, gaps, fix_hints, confidence }`
  on the event bus.

**Surfacing**
- Web: Pulse feed + Router tab render verifier (and budget) events.
- macOS: live cockpit cognition viz includes verdicts.
- Telegram: `/dock` rows carry a 🔬✅ / 🔬❌ marker for the latest verdict.

**Rubrics + docs**
- Three canonical rubrics in `.konjo/rubrics/`: `feature_completeness.toml`,
  `refactor_safety.toml`, `security_audit.toml`.
- `KONJO_VERIFIER.md` documents the rubric format, the resolution chain, and the
  brand position ("the only orchestrator that grades its own work before a PR").
- Sprint S1: Konjo CLI/TUI overhaul — REPL, slash commands, bypass mode.

### Added — Native macOS app

- SwiftUI dashboard in `macos/`: scaffold (Phases 1–2 + Cron), all admin panels
  (Phase 5), live cockpit with real-time cognition visualisation and Konjo motion.
- Durable cron schedules + config REST API (macOS Phase 0).

### Added — Web UI (the Forge) OpenClaw-parity overhaul

- New tabs: `pulse`, `router`, `logs`, `debug`, `config`; reactive orb with
  colored reactions; global logs API; Tools tab.
- Live SSE log tail in the Tasks drawer + quality-trend sparkline.
- `web/mod.rs` split into static + middleware modules to hold the file-size gate.

### Tests
- Verifier resolution chain + parse tests (`lopi-core`, `lopi-agent`).
- Workspace total: **631 passing**, 0 failing.

---

## [Unreleased] — Sprint R: Telegram Bot Overhaul ⛵️

### Added

**Full remote control surface** (`crates/lopi-remote/src/telegram/`)
- Rewrote `telegram.rs` as a module (`telegram/mod.rs`, `handlers.rs`, `monitor.rs`, `callbacks.rs`, `notify.rs`, `format.rs`) — all files under 400 lines
- 19 commands: `/task`, `/urgent`, `/critical`, `/status`, `/fleet`, `/dock`, `/cancel`, `/retry`, `/schedules`, `/run`, `/tail`, `/learn`, `/patterns`, `/approve`, `/cost`, `/draft`, `/submit`, `/cancel_draft`, `/help`

**Completion notifications** (`notify.rs`)
- `notify_loop` subscribes to `EventBus<AgentEvent>` and pushes Telegram messages on `TaskStarted` (attempt 1 only), `StatusChanged` (Implementing/Testing only), `ScoreUpdated` (score ≥ 0.75), `TaskCompleted` (always), `TaskCancelled`, `BudgetExceeded`
- PR URLs sent as **separate follow-up messages** for one-tap copyability
- Goal cached from `TaskQueued` events — completion messages include the task goal
- Suppresses `TurnMetrics`, `LogLine`, `PoolStats` — zero noise

**New monitoring commands** (`monitor.rs`)
- `/fleet` — running agents + queued tasks + pool stats + today's token cost, with [Refresh] [Dock] inline buttons
- `/dock [N]` — last N tasks (default 8, max 20) with status emoji and relative timestamps
- `/tail <id> [N]` — last N log lines for a task (default 10, max 30)
- `/cost` — today's tokens/cost + all-time task count + budget limits
- `/schedules` — all configured cron entries with next fire time
- `/run <name>` — trigger a named schedule immediately

**New task commands** (`handlers.rs`)
- `/critical <goal>` — critical priority queue at front
- `/cancel <id>` — sends cancel signal to running agent via `pool.cancel_by_prefix()`
- `/retry <id>` — looks up failed task by ID prefix, requeues at HIGH priority

**Draft mode** (`handlers.rs`)
- `/draft` — enter multi-line input mode; each plain-text message appends a line
- `/submit` — joins draft lines and queues as a task
- `/cancel_draft` — discard current draft

**Formatting helpers** (`format.rs`)
- `short_id()`, `priority_badge()`, `status_emoji()`, `relative_time()`, `format_uptime()` — consistent display across all commands

**`lopi-orchestrator` additions**
- `AgentPool::running_agents() -> Vec<RunningAgentInfo>` — lock-free snapshot for fleet display
- `AgentPool::cancel_by_prefix(prefix)` — cancel by ID prefix without needing the full UUID
- `TaskQueue::peek_queued() -> Vec<(Priority, String)>` — priority-sorted snapshot for fleet display
- `RunningAgentInfo` struct exported from `lopi-orchestrator`

**`sail_commands.rs`** — `spawn_telegram()` now passes `pool`, `bus`, `schedules`, and `notify_chat_id` through to `telegram::run()`

### Tests
- `format.rs`: 10 tests (short_id, priority_badge, all status_emoji variants, relative_time suite)
- `notify.rs`: 4 tests (success/no-PR/failed completion messages, budget exceeded format)
- `handlers.rs`: 4 tests (dock N parsing, tail arg parsing, auth check logic)
- `monitor.rs`: 4 tests (tail arg parsing, schedule name trim)
- **22 new tests**. Workspace: 499 → **571 passing**, 0 failing.

---

## [Unreleased] — Sprint P: Production Deployment + Tier Gating 🚀

### Added

**`CustomerTier` enum** (`lopi-core::tier`)
- `Free | Starter | Growth | Enterprise` variants with `max_agents()`, `display_name()`, `price_usd_cents_per_month()`, `features()`, `from_stripe_name()`.
- `Display` + `FromStr` round-trip; full `serde` support for wire serialization.
- 6 unit tests: serde round-trip, max_agents, from_stripe_name, display, price ordering.

**Tier column in `github_installations`** (`lopi-memory`)
- Idempotent `ALTER TABLE … ADD COLUMN tier TEXT NOT NULL DEFAULT 'free'` migration.
- `MemoryStore::set_installation_tier(installation_id, tier)` — updates tier on subscription event.
- `MemoryStore::customer_tier(customer_id)` — reads active installation tier; defaults to `Free` when absent.
- 3 new tests: set/get tier, unknown customer defaults to Free, upgrade+downgrade cycle.

**Stripe subscription → tier wiring** (`lopi-app::stripe`)
- `customer.subscription.created` / `customer.subscription.updated` — extracts tier from `items[0].price.nickname` or `metadata.lopi_plan`; reads `metadata.lopi_installation_id` to update the correct installation row.
- `customer.subscription.deleted` — downgrades tier to `Free`.
- `extract_tier_from_subscription()` + `extract_installation_id()` helpers — no stub logic remaining.

**`GET /api/plans`** (`lopi-ui::web`)
- Returns a static JSON array of all four `CustomerTier` variants with `id`, `name`, `price_usd_per_month`, `max_agents`, `features`.
- 2 endpoint integration tests: response shape + field presence.

**Tier-aware `max_agents` cap in `lopi sail`** (`src/sail_commands.rs`)
- `tier_capped_max_agents()` reads `LOPI_CUSTOMER_ID` env var, queries the DB for the customer's tier, and caps the requested `--max-agents` to `tier.max_agents()`.
- Logs a tracing `info!` event when the cap is applied. Falls back gracefully on DB error or absent env var.

**`Dockerfile`** (repo root)
- Multi-stage build: `rust:1.87-slim-bookworm` build stage → `debian:bookworm-slim` runtime.
- Non-root `lopi` user; persistent `VOLUME ["/home/lopi/.lopi"]`; `EXPOSE 3000 3002`.
- Dependency-layer caching via manifest-only pre-build stub.

**`fly.toml`** (repo root)
- Two process groups: `app` (`lopi serve-app` on 3002) and `web` (`lopi sail` on 3000).
- Persistent `lopi_data` volume mounted at `/home/lopi/.lopi`.
- HTTP health checks on `/` (app) and `/api/health` (web); TLS + HTTP on 443/80.

### Tests
- 6 `lopi-core::tier` tests + 3 `lopi-memory::installations` tests + 2 `lopi-ui::web` tests = **11 new**.
- Workspace: 488 → **499 passing**, 0 failing.

---

## [Unreleased] — P1 Agent Survivability Sprint 🚦

### Added

**P1.1 — Cost governor + circuit breakers** (`lopi-core::BudgetScope`,
`lopi-ratelimit::budget`)
- Three-tier hierarchical budget enforcement: `Fleet` → `Agent` → `Task`.
- `BudgetGovernor` wraps three `Arc<CircuitBreaker>`. `check()` walks the
  breakers innermost-first and returns the tightest enclosing scope that
  refuses, so the runner can attribute the failure correctly.
- `record_success(cost)` / `record_failure()` / `record_cost_only(cost)`
  feed each scope. `states()` returns a snapshot for `/metrics` exposition.
- `BudgetError::Exceeded { scope, limit_usd }` vs.
  `BudgetError::BreakerOpen { scope }` — distinguishes "hourly cap reached"
  from "too many consecutive failures".
- `BudgetConfig` defaults: $25/hr fleet · $5/hr agent · $1.50/hr task.
- New `AgentEvent::BudgetExceeded { task_id, scope, limit_usd, burned_usd }`
  — runner emits this the moment `check()` refuses, so the Forge UI can
  flag the breach before the next turn fires.

**P1.2 — OpenTelemetry spans behind `otel` Cargo feature** (root crate)
- Workspace deps `opentelemetry` · `opentelemetry_sdk` ·
  `opentelemetry-otlp` · `tracing-opentelemetry` are now `optional = true`
  and gated by `otel = ["dep:…"]`.
- Four GenAI-semconv-aligned spans wrap each agent turn:
  `lopi.agent.think` (planning step) · `lopi.agent.act` (`claude.implement`) ·
  `lopi.agent.score` (`scorer.score`) · `lopi.agent.task.complete` (terminal
  success return).
- Wrapped with `.instrument(span)` so the runner's outer future stays
  `Send` and the pool's `JoinSet::spawn` accepts it.
- `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_SERVICE_NAME` env vars honoured.
- Zero OTel runtime cost when the feature is off.

**P1.3 — Durable checkpoint + resume** (`lopi-memory::CheckpointRow` +
`lopi resume` + `POST /api/agents/:id/checkpoint`)
- New `agent_checkpoints` table with `idx_checkpoints_task_created` index.
- `CheckpointInput` builder · `MemoryStore::save_checkpoint` ·
  `latest_checkpoint` · `list_checkpoints`.
- `lopi resume --agent-id <uuid>` CLI subcommand loads the most-recent
  checkpoint and prints a human-readable summary (attempt, state, repo,
  hash, plan preview, score).
- `POST /api/agents/:id/checkpoint` accepts a JSON body
  `{state, attempt?, last_plan?, last_score?, repo_path?, context_hash?}`
  and persists it. Returns 201 with `{checkpoint_id, task_id}` or 400 for
  a non-UUID id. Sits behind Bearer auth + per-IP rate limiting.

**P1.4 — Structured output schema validation** (`lopi-core::schema`)
- Optional `Task::output_schema: Option<serde_json::Value>`. When present,
  the runner validates the scorer's JSON projection against it after each
  attempt.
- Pragmatic JSON Schema subset (`type`, `required`, `properties`, `enum`)
  — dep-free beyond `serde_json` to keep `lopi-core` at tier 1. Unknown
  keywords are permissive (ignored, not rejected).
- Process-wide `lopi_schema_violations_total{kind=…}` counter exposed via
  `/metrics`. Labels: `type`, `required`, `enum`, `property`.
- On validation failure: increments counter, warns the bus, stashes the
  violation summary as `last_error` so the next planning prompt sees it
  (via adaptive retry), rolls back git, and retries.

### Documentation

- **`PLAN.md`** — new "Researched Feature Roadmap" section: P1/P2/P3
  tiers covering MCP+A2A, multi-tier memory, human-in-the-loop pause
  points, constellation auto-scaling, compile-time policy proc macro,
  hierarchical agent delegation, and fleet replay.

### Tests

- `lopi-core::budget` — 2 unit tests (scope wire-string + JSON round-trip).
- `lopi-ratelimit::budget` — 6 governor unit tests.
- `lopi-memory::checkpoints` — 4 store unit tests.
- `lopi-ui::web::tests` — 2 endpoint integration tests for checkpoint route.
- `lopi-core::schema` — 10 validator unit tests including realistic score
  schema and counter increment.

### Architecture

- `.konjo/arch.toml` layer rules honoured: `BudgetScope` (tier 1) lives in
  `lopi-core`; `BudgetGovernor` and underlying `CircuitBreaker` (tier 2)
  live in `lopi-ratelimit`. No upward dependency.

## [0.17.0] — Sprint O: GitHub App Server Scaffold 🔐

### Added

**`crates/lopi-app/`** — new crate: GitHub App OAuth + Stripe webhook server
- `AppConfig::from_env()` — loads `GITHUB_APP_ID`, `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_REDIRECT_URI`, `GITHUB_WEBHOOK_SECRET`, `STRIPE_WEBHOOK_SECRET` at startup; gracefully degrades when absent
- `GET /app/install` — redirects to GitHub App installation page
- `GET /app/callback` — exchanges OAuth code for access token; stub for customer record creation
- `POST /app/webhook` — HMAC-verified GitHub App installation events; on `created`: upserts installation, provisions per-customer `MemoryStore`; on `deleted`: marks installation inactive
- `POST /stripe/webhook` — HMAC-SHA256 + timestamp replay protection (300s window); dispatches on `customer.subscription.{created,updated,deleted}`
- 6 unit tests (HMAC validation for both GitHub and Stripe)

**`crates/lopi-memory/src/store/installations.rs`** — GitHub App installation ledger
- `github_installations` table: `installation_id`, `customer_id`, `account_login`, `account_type`, `status`, timestamps
- `upsert_installation(id, login, type)` — idempotent; handles reinstalls
- `delete_installation(id)` — marks as `'deleted'`
- `customer_for_installation(id)` — lookup by installation_id (active only)
- `list_installations()` — all active installations
- `sanitise_customer_id(login)` — lowercase, alphanumeric + hyphen only
- 5 unit tests: install/delete/reinstall/list/sanitise

**`lopi serve-app` CLI command** — start the lopi-app server
- `lopi serve-app [--port 3002] [--host …]`
- Prints credential status at startup: `✅ configured` or `⚠️ missing` per service
- Provisions `MemoryStore` from the shared `db_path()`

**`web/src/routes/onboard/+page.svelte`** — customer onboarding page
- 3-step install flow: install App → `lopi spec --save` → `lopi watch-gap-fill`
- "Install GitHub App" button → `lopi serve-app` install endpoint
- Pricing table: Starter $299/mo · Growth $999/mo · Enterprise $4,999/mo

### Fixed — File budget
**`store/tests.rs`** (504 lines) split into `tests.rs` (190) + `tests_extra.rs` (322)

### Tests
- 5 installations + 6 lopi-app tests (11 new)
- Workspace: 408 → **419 passing**, 0 failing. 0 clippy warnings.

---

## [0.16.0] — Sprint N: Trust Calibration + Per-Customer Isolation 🎯

### Added

**Trust calibration — `compute_weight_adjustments()` is now live**
- `crates/lopi-orchestrator/src/pool.rs`: `compute_weight_adjustments()` is now `async` and actually calls `store.compute_weight_adjustments()` — pulling score weights from annotated pattern history on every task dispatch
- Approved patterns that needed fewer attempts tighten lint/diff penalties; rejected patterns loosen them. Signal clamped to [-2.0, 2.0] × 0.005 → delta applied to weights
- Falls back to defaults gracefully when no annotations exist or the store is absent

**`lopi trust` CLI command** (`src/trust_commands.rs`)
- Shows approved vs rejected pattern counts and avg-attempt stats
- Prints current score weight adjustments (live from the DB)
- Gives direction signal: "tightening / loosening / balanced"

**`MemoryStore::open_for_customer(base_dir, customer_id)`** — per-customer isolated store
- Creates `{base_dir}/{customer_id}/lopi.db` — one SQLite file per tenant
- Sanitises `customer_id`: only `[A-Za-z0-9-_]` allowed; unsafe chars become `_`
- 2 integration tests: isolation verified by cross-store task count, path traversal sanitised

**`crates/lopi-memory/src/store/patterns.rs`** — extracted from mod.rs
- All pattern operations: `jaccard_similarity`, `keyword_fingerprint`, `find_similar_patterns`, `load_patterns`, `find_pattern_by_id_prefix`, `insert_postmortem_pattern`, `mine_patterns`, `annotate_pattern`, `load_annotated_patterns`, `compute_weight_adjustments`
- `PatternRow` struct moved here
- store/mod.rs: 557 → **310 lines** ✅

**`src/task_commands.rs`** — Watch/Tail/Dock/Cancel extracted from main.rs
- main.rs: 511 → **448 lines** ✅

### Architecture notes

Trust calibration closes the learning loop: the human annotates patterns → weights adjust → agent gets scored differently on next attempt → better patterns get approved. Over 50–200 annotated patterns, the weights converge to reflect what this specific human values. Per-customer store isolation is the SaaS tenancy primitive — each customer's pattern history, lessons, and quality runs are fully separated.

### Tests
- 2 new per-customer store isolation tests
- Workspace: 405 → **408 passing**, 0 failing. 0 clippy warnings.

---

## [0.15.0] — Sprint M: Continuous Loop + Multi-Repo 🔄

### Added

**`crates/lopi-memory/src/store/quality.rs`** — quality check run ledger
- `quality_check_runs` table: `spec_items`, `passing`, `failing`, `gaps`, `score`, `run_at`
- `MemoryStore::save_quality_run(QualityRunRecord)` — persist one run with auto-computed score
- `MemoryStore::load_quality_trend(repo_path, limit)` — fetch runs ordered by `run_at DESC`
- `MemoryStore::quality_trend_delta(repo_path)` — (latest_score, prev_score) pair for trend arrow
- `QualityRunRow::improved_vs(&prev)` — boolean trend comparison
- 5 unit tests

**`lopi gap-fill` — now persists quality data + prints trend**
- After each run: saves a `QualityRunRow` to SQLite via `save_quality_run()`
- Loads previous run and prints coverage trend: `coverage: 82% ↑ (was 76%)`
- Returns `QualitySnapshot` so the daemon loop can log without re-querying
- New `quiet: bool` param — suppresses output when called from the daemon

**`lopi watch-gap-fill` — Kitchen Loop daemon**
- `lopi watch-gap-fill [--repo .] [--interval 60] [--sail-url ...] [--run-now]`
- Runs gap-fill every N minutes (default 60), persisting results and queuing fix tasks
- `--run-now`: triggers one immediate run before the loop starts
- Ctrl-C cleanly exits the loop

**`lopi sail --repos` — multi-repo mode**
- `--repos repo1,repo2,…` — additional repo paths alongside the primary `--repo`
- Each extra repo gets its own `AgentPool` dispatch loop sharing the shared queue and bus
- Pool already routes by `task.repo_path` — multi-repo just adds parallel dispatch
- Banner prints all repos at startup

**`/api/quality/trend`** — quality trend web endpoint
- `GET /api/quality/trend?repo=<path>&limit=<n>` — returns quality check run history
- Falls back to `AppState.repo_path` when `repo` query param is absent

### Architecture notes

The `watch-gap-fill` daemon is the mechanical basis of the Kitchen Loop. Each iteration runs the full spec → test → gap detection → queue pipeline. As fix tasks complete and get merged, the next iteration finds fewer gaps — driving the autonomous quality ratchet. The SQLite trend table makes the improvement measurable rather than impressionistic.

Multi-repo dispatch works because `task.repo_path` is already a field on `Task` and the pool already routes on it. Adding `--repos` spawns parallel dispatch goroutines, each bound to one repo path. No new queue needed.

### Tests
- 5 new quality.rs tests + 2 gap_fill_commands snapshot tests
- Workspace: 399 → **405 passing**, 0 failing. 0 clippy warnings.

---

## [0.14.0] — Sprint L: Synthetic User + File Budget Fixes 🔬

### Added

**`lopi-spec/src/test_runner.rs`** — test run parser
- `run_tests(repo_path)` — auto-detects `cargo test` vs `pytest`, runs with `--no-fail-fast`, captures pass/fail per test name
- `parse_cargo_output(output)` — parses `test name ... ok/FAILED` lines into `Vec<TestRunResult>`
- `parse_pytest_output(output)` — parses `file::test_name PASSED/FAILED` lines
- `coverage_gaps(spec_items, results)` — returns spec items with no passing run (failing tests + never-ran tests)
- `TestRunResult { name, passed, error }` — serialisable result record
- 8 unit tests (cargo format, pytest format, gap detection)

**`src/gap_fill_commands.rs`** — `lopi gap-fill`
- Loads spec surface (cached or live) → runs tests → computes coverage gaps → queues fix tasks via `POST /api/tasks` on a running `lopi sail` server
- `--dry-run`: reports gaps without queuing
- `--sail-url`: configurable target (default `http://127.0.0.1:3000`)

**`lopi check --fail-on-violations`** — CI-compatible exit code
- Exits with `std::process::exit(1)` when file-size or spec-drift violations are found
- Zero means clean; non-zero blocks CI pipeline

### Fixed — File Budget Violations (all three files were > 500 lines)

**`crates/lopi-agent/src/runner/run_loop.rs`**: 651 → 480 lines
- Extracted `run_stability_preflight` + `save_stability_ledger_entry` → new `stability_runner.rs`
- Extracted `run_postmortem_if_configured` + `persist_postmortem_outcome` → new `postmortem_runner.rs`
- Moved `status()` + `emit_turn_metrics()` to `mod.rs` (always-available utilities)

**`crates/lopi-ui/src/web/mod.rs`**: 593 → 372 lines
- Extracted all 9 route handlers → new `web/handlers.rs`
- `types` module promoted to `pub(crate)` for cross-file access

**`src/main.rs`**: 560 → 486 lines
- Extracted `Commands::Run` (97-line agent loop) → new `src/run_command.rs`
- `is_self_modify_attempt`, `status_label` promoted to `pub(crate)`

### Tests
- 8 new `lopi-spec::test_runner` tests
- Workspace: 390 → **399 passing**, 0 failing
- 0 clippy warnings

---

## [0.13.0] — Sprint K: Spec Surface + KCQF 📋

### Added

**`crates/lopi-spec`** — new crate: spec surface extractor
- `SpecSurface::extract(repo_path)` — walks all `.rs` and `.py` files, extracts test function names and doc comments
- **Rust** (`rust_extractor.rs`): `#[test]`, `#[tokio::test]`, `#[async_std::test]`, `#[rstest]`, `#[proptest]`; captures preceding `///` doc comments as description
- **Python** (`python_extractor.rs`): `def test_*` and `async def test_*`; captures inline docstring as description
- `SpecSurface::save(repo)` — writes `.lopi/spec_surface.json` as a cacheable baseline
- `SpecSurface::load(repo)` — loads cached surface (returns `None` when not yet saved)
- `SpecSurface::top_descriptions(n)` — returns top N items as TOON-ready strings
- `SpecItem { name, description, kind, file, line }` · `SpecKind: RustTest | PythonTest`
- 24 unit tests across `lib.rs`, `rust_extractor.rs`, `python_extractor.rs`

**`src/spec_commands.rs`** — two new CLI commands
- `lopi spec [--repo .] [--export] [--save]` — extract + display spec surface as a table, optionally cache to `.lopi/spec_surface.json`
- `lopi check [--repo .]` — KCQF quality analysis:
  - File-size gate: reports any `.rs` / `.py` file > 500 lines (with path + line count)
  - Spec drift gate: compares live extraction against the cached baseline; lists newly removed tests as regression risks
- 4 unit tests in `spec_commands.rs` (size violations, target-skip, clean pass)

**Spec surface injection into planning** (`lopi-agent/src/runner/run_loop.rs`)
- At each run, loads `.lopi/spec_surface.json` if present; injects top 10 items as additional constraints in the planning prompt alongside patterns and lessons
- Log line: `📋 spec surface: N items loaded`

**`/api/spec` web endpoint** (`lopi-ui/src/web/mod.rs`)
- `GET /api/spec` — returns cached spec surface or runs live extraction; JSON with `count`, `rust_files_scanned`, `python_files_scanned`, `extracted_at`, `items`
- `AppState::new_with_repo(...)` — new variant that records `repo_path` for spec serving
- `serve_with_repo(...)` — new variant of `serve()` that passes repo_path into AppState; called from `sail_commands::run()` so the spec API reflects the actual sailed repo

### Architecture notes

Spec surface is the ground truth for the self-improvement loop. Injecting the top 10 descriptions into the planning prompt lets Claude know what the repo already claims to do — reducing the risk of agents writing tests that contradict or duplicate existing spec items. The spec drift check in `lopi check` is the first automated regression guard: any test that disappears between runs is surfaced before it becomes a silent regression.

### Tests

- 24 lopi-spec tests
- 4 spec_commands tests
- Workspace: 362 → **390 passing**, 0 failing

---

## [0.12.0] — Sprint J: GitHub Issue Loop 🪝

### Added

**`crates/lopi-github`** — new crate: thin GitHub REST API write client
- `GitHubClient::new(token)` — constructs a reqwest-based client with `User-Agent: lopi/<version>`
- `GitHubClient::post_comment(owner, repo, issue_number, body)` — posts a comment on any issue or PR
- `GitHubClient::add_labels(owner, repo, issue_number, labels)` — adds one or more labels

**`crates/lopi-webhook/src/issue_triage.rs`** — Haiku-powered issue classifier
- `IssueCategory: Bug | Feature | Question | WontFix` — four-way classification
- `IssueTriage { category, confidence, summary }` — structured triage output
- `classify_issue(client, limiter, breaker, model, title, body)` — calls Haiku with a byte-stable system prompt (`cache_control: ephemeral`) for cross-issue cache hits; cost ~$0.0003/issue
- `parse_triage_response(raw)` — defensive three-line parser: category, confidence (clamped 0–1), ≤120-char summary
- `format_triage_comment(triage, repo)` — formatted Markdown comment including category icon, confidence %, summary, and action description
- 14 unit tests covering parsing, edge cases, label mapping, comment formatting

**`crates/lopi-webhook/src/issue.rs`** — issue handler
- `IssuePayload` — parsed issue fields: owner, repo, full_name, number, title, body, labels
- `IssuePayload::has_lopi_fix_label()` — case-insensitive `lopi:fix` label check
- `extract_from_json(payload, full_name)` — zero-copy extraction from raw webhook JSON
- `spawn_triage(...)` — fires a Tokio background task: classify → comment → label → optionally queue fix task
- Auto-queue threshold: Bug + confidence ≥ 0.7, OR any issue with `lopi:fix` label (overrides classification)

**`crates/lopi-webhook/src/github.rs`** — extended webhook router
- `TriageConfig { api_client, github, limiter, breaker, model }` — optional triage configuration passed to `serve()`
- `serve(queue, secret, addr, triage: Option<TriageConfig>)` — updated signature; triage is opt-in, webhook returns 200 immediately while triage runs in background
- Routes `issues` event `action == "opened"` and `action == "labeled"` to `issue::spawn_triage`

**`src/main.rs`** — new CLI command
- `lopi serve-webhooks [--port 3001] [--host ...] [--webhook-secret ...] [--github-token ...] [--anthropic-key ...]`
- All credentials also read from `LOPI_WEBHOOK_SECRET`, `GITHUB_TOKEN`, `ANTHROPIC_API_KEY` env vars
- Triage enabled only when both `GITHUB_TOKEN` and `ANTHROPIC_API_KEY` are set; gracefully degrades to comment-only webhook server otherwise

### Architecture notes

The webhook server runs independently from `lopi sail` — two separate processes with separate ports (3001 vs 3000). Webhook returns 200 immediately; all AI work (Haiku triage call, GitHub API write) happens in a spawned Tokio task. If either fails, a `tracing::warn!` is emitted and the issue is skipped — webhook liveness is never blocked by external API calls.

Kitchen Loop analogy: this is the inbound side of the loop. Issues arrive from GitHub → lopi triages and queues → agents fix and open PRs → reviewer merges → patterns learned. Combined with Sprint I's lesson injection, the self-improvement cycle is now end-to-end.

### Tests
- 2 lopi-github tests (client construction)
- 14 lopi-webhook issue_triage tests
- 2 lopi-webhook issue.rs tests
- 18 new tests total. Workspace: 313 → **331 passing**, 0 failing.

---

## [0.11.0] — Sprint I: Phase 5b Self-Improvement Second Wave

### Added

**Score weights wiring** (`crates/lopi-agent/src/runner/mod.rs`)
- `AgentRunner::score_weights: ScoreWeights` — field; defaults to `ScoreWeights::default()`
- `AgentRunner::task_lessons: Vec<String>` — lessons for injection into the API planning path
- `AgentRunner::with_score_weights(weights)` — chainable builder
- Run loop now logs weighted score alongside raw score: `📊 score: pass=X% lint=Y diff=ZL (weighted=W.WW)`
- Fixed-score path also logs weighted score after the in-place fix attempt

**`compute_weight_adjustments()` in pool.rs** — free function that computes per-task score weights before handing off to the runner. Placeholder: returns defaults. Phase 5b.1 will query approved patterns for weight tuning.

**Lesson + Pattern injection** (`crates/lopi-agent/src/claude.rs`, `run_loop.rs`)
- `ClaudeCode::patterns: Vec<(String, String)>` + `ClaudeCode::with_patterns()` — tabular (keywords, constraints) pairs fed to TOON encoder at site 2
- `ClaudeCode::lessons: Vec<(String, String)>` + `ClaudeCode::with_lessons()` — (category, content) lessons from the lessons table
- `plan()` now passes both to `encode_task_context()` — TOON renders them as §9.3 tabular rows (saves ~158 tokens/attempt)
- `run_loop.rs` single memory query now builds **both** string constraints (legacy) **and** tabular pattern pairs; loads lessons via `store.load_lessons(repo_path, 10)` and stores them in `self.task_lessons` for the API path
- Extracted `plan_streaming()` → new `crates/lopi-agent/src/claude_stream.rs` (claude.rs: 474 → 408 lines)

**Post-mortem lessons** (`crates/lopi-agent/src/runner/run_loop.rs`)
- After `insert_postmortem_pattern()` succeeds, also calls `store.save_lesson(repo_path, "recovery", constraint, Some(task_id), 1.0)` — makes the constraint discoverable in future lesson injections

**API plan lessons** (`crates/lopi-agent/src/runner/api_plan.rs`)
- `build_user_prompt(task, last_error, lessons)` — appends `# Lessons from past patterns` section when lessons are non-empty
- 1 new test: `user_prompt_includes_lessons_when_provided`

**CLI annotate** (`src/main.rs`)
- `lopi learn annotate <id-prefix> <approved|rejected>` — validates annotation, resolves id prefix via `find_pattern_by_id_prefix`, calls `annotate_pattern()`

### Tests
- 1 new api_plan test. Workspace: 261 → **313 passing**, 0 failing.

---

## [0.10.0] — Sprint H: Self-Improvement Engine 🧠

### Added

**`lopi learn` CLI subcommand tree** (was a single flat command)
- `lopi learn list [--limit N] [--postmortem-only]` — sorted pattern table with id prefix, keywords, avg attempts, success %, and source emoji (📊 mined / 🧠 post-mortem)
- `lopi learn show <id-prefix>` — full pattern detail page
- `lopi learn export [--limit N]` — JSON output to stdout for analytics pipelines

**`runner::postmortem` module** (`crates/lopi-agent/src/runner/postmortem.rs`)
- `run_postmortem(client, limiter, breaker, model, goal, error_log)` — single-turn Claude reflection over a failed run. Returns one imperative constraint string (≤ 200 chars, must start with `must` / `do not` / `always` / `never`).
- `extract_constraint(raw)` — defensive validation: strips markdown bullets, takes first non-empty line, rejects fluffy non-imperative responses, truncates over-long lines.
- `run_postmortem_quiet(...)` — error-swallowing variant for terminal-failure path: never blocks task completion.
- System prompt is byte-stable for `cache_control: ephemeral` cache hits across post-mortems in a session.

**Adaptive retry** (`AgentRunner::with_adaptive_retry()`)
- New builder method, chainable on top of `with_api(...)`.
- Stashes the previous attempt's score (test_pass_rate, lint_errors, diff_lines, errors) as `last_error` after each failed attempt.
- After all retries exhausted, automatically fires `run_postmortem_if_configured()` — runs the post-mortem if both adaptive retry AND a configured `AnthropicClient` are present.
- Persists the derived constraint to the patterns table.

**`MemoryStore` additions** (`crates/lopi-memory/src/store.rs`)
- `insert_postmortem_pattern(goal_keywords, constraint) -> id` — creates a row with `derived_from_postmortem = 1`, seeded `success_rate = 0.0`.
- `find_pattern_by_id_prefix(prefix) -> Option<PatternRow>` — for `lopi learn show` UX.
- `load_patterns` ordering changed: `ORDER BY COALESCE(success_rate, 0) DESC, last_seen DESC` — real-data patterns now surface above zero-seeded post-mortem rows.

**Schema migration** (`crates/lopi-memory/src/schema.sql`)
- `ALTER TABLE patterns ADD COLUMN derived_from_postmortem INTEGER NOT NULL DEFAULT 0`.
- Fixed `apply_schema()` to correctly strip leading `--` SQL comments before the ALTER TABLE prefix check — comments above ALTER TABLE statements no longer break the duplicate-column-tolerant migration path.

### Tests

- 4 new lopi-memory tests: postmortem-pattern insert + retrieve, prefix-not-found, postmortem flag in load_patterns, ordering correctness.
- 11 new lopi-agent tests in `runner::postmortem::tests`: extract_constraint validation across 7 input shapes, build_prompt determinism + content + truncation.
- 2 new lopi-agent integration tests: `runner_default_has_no_direct_api`, `with_api_enables_direct_path` (already shipped in Sprint G).
- Workspace total: 244 → **261 passing**, 0 failed.

### Architecture note

The post-mortem fires on terminal failure (all retries exhausted) and uses Haiku for cost. A single short turn of <2000 tokens with cached system prompt costs roughly $0.0008. The constraint it derives slots into the existing `extra_constraints` mechanism in the planning prompt — no new prompt-injection plumbing required, the pattern miner already feeds patterns into TOON-encoded prose at planning time.

The `last_error` field is now stashed on the runner but not yet injected into the next attempt's planning prompt — that's a follow-up sprint (H1) since it requires touching the prompt builders in both `claude.rs::plan` and `runner::api_plan::build_user_prompt`.

---

## [0.9.0] — Sprint G: Direct Anthropic SDK planning path

### Added

**Direct API path for the planning step** (`crates/lopi-agent/src/runner/api_plan.rs`)
- `AgentRunner::plan_via_api(model, attempt) -> Result<String>` — replaces the `claude` CLI subprocess call when the runner has been wired with `AnthropicClient` via the new `AgentRunner::with_api(client, limiter, breaker)` builder.
- The CLI path remains the load-bearing default. On any direct-API failure (rate limited, breaker open, network error, 4xx/5xx) the run loop falls back to the CLI silently — an API outage cannot stall agent execution.

**Resilience layered on every API request:**
1. `CircuitBreaker::check()` — refuses if open from prior failures or if the hourly cost cap was hit.
2. `AnthropicLimiter::acquire_request(4000)` — concurrent TPM + RPM enforcement at default-pro limits (120k TPM / 15 RPM).
3. `AnthropicClient::stream_plan` — SSE streaming with `cache_control: ephemeral` on the system prompt for ~90% cost reduction on repeat calls.
4. `CircuitBreaker::record_success` / `record_failure` / `record_cost` — feeds the failure counter and hourly USD spend back into the breaker.

**Real `TurnMetrics` from API responses:**
- Every successful direct-API plan call captures real `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_write_input_tokens`, `ttft_ms`, `turn_latency_ms`, and `estimated_cost_usd`.
- `TurnMetrics` event emitted on the `EventBus` so the lopi-ui Forge animates with **real** `cost_usd` and `tokens_per_sec` instead of the phase-derived stubs (UI-2 baseline).
- Persisted to the SQLite `turn_metrics` table via `MemoryStore::save_turn_metrics`.

**Builder API:**
- `AgentRunner::with_api(client: Arc<AnthropicClient>, limiter: Arc<AnthropicLimiter>, breaker: Arc<CircuitBreaker>)` — chainable on top of `new()` or `standalone()`. `has_direct_api()` accessor for tests and tracing.
- New optional fields on `AgentRunner`: `api_client`, `limiter`, `breaker`, `session_id` (used by `TurnMetrics.session_id`).

**Prompt builder:**
- `build_user_prompt(&Task)` — deterministic markdown rendering of goal/constraints/allowed_dirs/forbidden_dirs. Same task → byte-identical prompt → cache hit on the system+user prefix.

### Changed
- `lopi-agent` now depends on `lopi-ratelimit` and `chrono` (workspace).
- `runner/run_loop.rs` planning branch routes through `plan_via_api` first when configured, with transparent CLI fallback.

### Tests
- 7 new tests in `runner::api_plan::tests`: prompt builder determinism + content + section omission, builder integration (default has no direct API; `with_api` enables it).
- lopi-agent: 10 → 17 passing.
- Workspace total: 244 passing, 0 failed.

### Architecture note
The CLI path is intentionally retained for the **implementation step** because file-edit tool access requires the `claude` CLI's native filesystem hooks. Migrating implementation to direct API would require either Anthropic's tool-use protocol with custom file-edit tools, or a sidecar that bridges API tool calls to filesystem ops — neither in scope for this sprint. Sprint G specifically targets the planning step where pure text generation suffices and prompt caching delivers the largest cost win.

---

## [0.8.0] — Observability, Correctness, Systems, Resilience

### Added

**Sprint A — Observability**
- `lopi-core`: `TurnMetrics` struct capturing token accounting (input/output/cache read/write), latency (TTFT, turn, tool execution), context pressure, eviction count, tool call count, and estimated cost per turn
- `lopi-memory`: `turn_metrics` table with `task_id` and `timestamp` indexes; `MemoryStore::save_turn_metrics()` for persisting per-turn records
- `benchmarks/corpus/README.md`: ten canonical benchmark tasks T01–T10 with measurement protocol and acceptance criteria
- `benchmarks/run.sh`: shell runner for the corpus — per-task logging, machine-readable JSON summary in `benchmarks/results/<timestamp>/`

**Sprint E — Systems**
- `src/main.rs`: mimalloc installed as global allocator (`#[global_allocator]`) — ~30% lower allocation latency on macOS vs system malloc
- `lopi-agent/runner.rs`: `backoff_secs()` — full-jitter exponential backoff (base 500 ms, cap 30 s, Uniform[0, ceiling]) applied before each retry
- `.config/nextest.toml`: nextest configuration — default profile uses `num-cpus` threads; `ci` profile adds 2 retries, 120 s test timeout, slow-timeout termination
- `.cargo/config.toml`: `[alias] t = "nextest run"`
- `crates/lopi-context/benches/eviction.rs`: three Criterion benchmarks (`evict_to_budget_100_turns`, `to_api_messages_1000_turns`, `push_at_75pct_pressure`)

**Sprint F — Resilience**
- New crate `crates/lopi-ratelimit`:
  - `TokenBucket`: async token-bucket with non-spinning `acquire()` (computed wait from deficit) and non-blocking `try_acquire()`
  - `AnthropicLimiter`: dual TPM+RPM enforcement; `default_pro()` sets 120k TPM / 15 RPM; `acquire_request()` concurrently awaits both buckets via `tokio::join!`
  - `CircuitBreaker`: Closed → Open → HalfOpen state machine with two independent trip conditions (consecutive failures + per-hour USD cost cap); hourly automatic reset

### Changed

**Sprint B — Correctness**
- `lopi-memory`: `MemoryStore` refactored to dual-pool architecture — `write_pool` (max 1 connection, serialises all INSERTs/UPDATEs/DDL) and `read_pool` (max 8 connections, read-only); in-memory tests share one pool safely
- `lopi-git`: `checkout_new_branch()` now holds a process-wide `WORKTREE_LOCK` (once_cell `Lazy<Mutex<()>>`) for the duration of the git branch + checkout sequence to prevent parallel agent races on HEAD/index
- `lopi-git`: `GitManager::worktree_env()` returns `[("CARGO_TARGET_DIR", ".cargo-target")]` — consumed by agent sub-process spawn to isolate `target/` directories
- `lopi-agent`: `AgentRunner` gains `max_turns: u32` (default 25) and `turn_count: u32`; hard stop emits `TaskStatus::Failed { reason: "TurnLimitExceeded … " }` before the turn limit is exceeded
- `lopi-agent`: `AgentRunner` gains a `CancellationToken` field alongside the existing oneshot cancel channel; `check_cancel()` checks the token first (structured shutdown path) then the oneshot (web API / CLI path)
- `lopi-orchestrator`: `AgentPool` gains a `JoinSet<()>` field for structured task tracking; tasks are spawned into the join set; completed tasks are drained on each dispatch; `shutdown()` calls `abort_all()` and drains

### Tests
- lopi-ratelimit: 10 new tests — 0 failures
- Total workspace (excluding lopi-context integration): **57 unit tests, 0 failures**
- Criterion benchmarks in lopi-context compile and run cleanly

## [0.7.0] — lopi-context: KV cache eviction layer

### Added
- `crates/lopi-context` — new crate owning all Anthropic message history as a mutable, policy-driven data structure
  - `TaggedMessage` — wire-format message with eviction metadata: `PinPolicy`, `Phase`, `tool_pair_id`, `is_conclusion`, `evict_after`
  - `ContextWindow` — the central type; `push()`, `push_tool_pair()`, `transition_phase()`, `pin_conclusion()`, `evict_phase()`, `evict_to_budget()`, `evict_turn()`, `to_api_messages()`
  - Three composable eviction policies: `PhaseTransition` (explicit phase sweep), `BudgetLIFO` (oldest-first when pressure > 75%), `ExplicitTag` (per-turn `evict_after` sentinel)
  - **Invariant: tool_use/tool_result pairs are always evicted atomically** — `evict_turn(id, force=false)` returns `OrphanedToolPair` error; `force=true` evicts both
  - **Invariant: `is_conclusion = true` turns survive all automatic policies** — only `evict_turn(id, force=true)` can remove them
  - **Invariant: `to_api_messages()` always returns turns in insertion order**, regardless of what was evicted from the middle
  - Token estimation via `tiktoken-rs` cl100k_base (text blocks: BPE; tool blocks: JSON/4; 4-token overhead per message)
  - `ContextStats`, `EvictionStats`, `EvictionRecord` for observability; eviction log ready for Phase 2 SQLite persistence
- `lopi-agent` — `AgentRunner` now carries a `ContextWindow` tracking Boot → Planning → Implementation → Testing → Conclusion phase transitions; logs `token_pressure()` at each transition via `tracing::info!`
- 20 new tests across 5 integration test files: `tool_pair_atomicity`, `phase_eviction`, `budget_lifo`, `conclusion_preservation`, `api_message_ordering` — all deterministic, no API key required
- 1 `#[ignore]` integration test (`token_estimation`) verifying estimate within 10% of Anthropic count-tokens API; run with `cargo test --test token_estimation -- --ignored`

### Changed
- `lopi-agent` depends on `lopi-context`; `AgentRunner` gains a `pub context: ContextWindow` field
- Fixed pre-existing clippy warnings in `lopi-toon` (while_let_loop, unnecessary_to_owned, manual_strip ×3), `lopi-git` (needless_match), `lopi-orchestrator` (doc quote), `lopi-ui` (unnecessary_to_owned), `lopi` main (print_literal ×2)

### Architecture note
lopi-agent currently uses the `claude` CLI binary, not the Anthropic SDK directly. `lopi-context` tracks session-phase state and token pressure across the agent run. The `to_api_messages()` output is the integration point for future direct-SDK migration. The eviction contract is established now — the wire-up to API calls is the next phase.

### Tests
- lopi-context: 20 new tests — 0 failures
- **Total: 101 tests, 0 failures**

## [0.6.0] — lopi-toon: TOON encoder/decoder + prompt token reduction

### Added
- `crates/lopi-toon` — full TOON v3.0 encoder and decoder per spec (https://toonformat.dev/)
  - `encode(value: &Value) -> String` — encodes JSON data model to TOON
  - `decode(input: &str) -> Result<Value>` — decodes TOON back to JSON with strict validation
  - `encode_task_context(goal, allowed, forbidden, constraints, patterns)` — lopi-specific helper
  - Encoder: tabular arrays (§9.3), inline primitive arrays (§9.1), expanded mixed arrays (§9.4)
  - Encoder: minimal quoting per §7.2 — reserved words, numeric-like strings, special chars
  - Encoder: canonical number format — no exponents, no trailing zeros, -0→0, NaN/Inf→null
  - Decoder: root form discovery (§5), keyed vs root array headers (§5 fix)
  - Decoder: inline arrays, tabular rows, expanded list items, nested objects
  - Decoder: `split_on_delim` respects quoted strings; strict count/width enforcement
  - 29 tests covering: all scalar types, quoting edge cases, flat/nested objects, all array forms,
    spec example round-trip, token efficiency assertion
- `lopi-agent/src/claude.rs` — TOON integrated at all three sites from token analysis:
  - **Site 1** (`plan()`, `implement()`): constraints/allowed_dirs/forbidden_dirs arrays
    encoded as TOON §9.1 inline arrays (~17 tokens/prompt saved, ~14% reduction)
  - **Site 2** (`plan()` via `runner.rs`): pattern memory injected as TOON context
    (~158 tokens/attempt saved, grows linearly with pattern count — the dominant win)
  - **Site 3** (`fix()`): error text is free-form prose — TOON intentionally skipped (no gain)
- At 100 tasks/day, estimated **-1.9M tokens/month** net reduction

### Changed
- `lopi-agent` now depends on `lopi-toon`
- `claude.rs::plan()` prompt uses `encode_task_context()` for structured context block
- `claude.rs::implement()` uses TOON scope block for allowed/forbidden dirs
- `claude.rs::fix()` uses inline TOON array for allowed_dirs (prose errors unchanged)

### Tests
- lopi-toon: 29 new tests — 0 failures
- **Total: 75 tests, 0 failures**

## [0.5.0] — Phase 4: Scheduled Tasks, Repo Profiles, lopi watch --remote

### Added
- `ScheduleEntry` type in `lopi-core` — `name`, `repo`, `goal`, `cron`, `priority`, `allowed_dirs`, `forbidden_dirs`; fully serde-compatible with `[[schedules]]` TOML arrays
- `RepoProfile` type in `lopi-core` — per-repo `.lopi.toml` profile with `allowed_dirs`, `forbidden_dirs`, `test_command`, `lint_command`, `default_constraints`, `max_retries`; `apply(&mut Task)` merges non-empty overrides
- `RepoProfile::load_from_repo(path)` — reads `<repo>/.lopi.toml`, returns `Default` if not found
- `LopiConfig::find_and_load()` — auto-discovers `./lopi.toml` then `~/.lopi/lopi.toml`
- `lopi-orchestrator::scheduler` module — `boot(entries, pool)` registers async cron jobs via `tokio-cron-scheduler`; `next_run_times(cron, n)` computes upcoming fire times
- `lopi schedules list` — prints configured schedules with next UTC run time
- `lopi watch --remote <url>` — connects to a running `lopi sail` WebSocket, injects events into local bus, drives the ratatui TUI from network events
- `lopi watch --local` — original isolated local bus behaviour
- `lopi sail` boots the cron scheduler alongside the agent pool if `[[schedules]]` are configured
- `lopi run` reads per-repo `.lopi.toml` and applies it before submitting the task
- `.lopi.toml.example` — per-repo profile template
- Updated `lopi.toml.example` with commented `[[schedules]]` examples

### Tests
- lopi-core: +6 tests (schedule_entry_deserializes, config_with_schedules, config_empty_schedules, repo_profile_default, repo_profile_apply_overrides, repo_profile_apply_skips_empty) → **20 total**
- lopi-orchestrator: +2 tests (next_run_times_valid_expr, next_run_times_invalid_expr) → **7 total**
- **Total: 46 tests, 0 failures**

## [0.4.0] — Phase 2 Full: live concurrency, ratatui TUI, full dashboard

### Added
- `AgentEvent` enum in lopi-core — rich events replacing plain `TaskStatus` broadcasts:
  `TaskQueued`, `TaskStarted`, `StatusChanged`, `LogLine`, `ScoreUpdated`, `TaskCompleted`, `TaskCancelled`, `PoolStats`
- `LogLevel` enum (`info`, `warn`, `error`, `debug`) with `AgentEvent::info/warn/error` helpers
- `AgentPool`: `DashMap<TaskId, AgentHandle>` tracking live agents with `cancel_tx: oneshot::Sender<()>`
- `AgentPool::cancel(task_id)` — graceful cancel signal to running agent
- `AgentPool::submit(task)` — enqueue + broadcast `TaskQueued` + save to DB
- `AgentPool::stats()` → `PoolStats { running, queued, succeeded, failed, uptime_secs }`
- `AgentPool::with_store()` — attach memory for pattern mining + DB persistence
- `AgentRunner` upgraded: emits `AgentEvent` at every stage (LogLine, StatusChanged, ScoreUpdated, TaskStarted); accepts `cancel_rx: oneshot::Receiver<()>` and polls cancel between stages; integrates `MemoryStore` for attempt persistence and pattern seeding
- `ClaudeCode::with_extra_constraints()` — injects memory patterns into planning prompt
- Full ratatui TUI (`lopi watch`): agent table with 7 columns, log panel (last 20 lines with level color), stats bar, help overlay, keyboard: `q/j/k/↑↓/Enter/l/Esc/?/F1`
- Full web dashboard (`index.html`): dark Konjo purple theme, live agent cards with score bar + elapsed timer + cancel button, sidebar submit form (goal/repo/priority, Ctrl+Enter), log stream, WebSocket reconnect with exponential backoff, state snapshot on connect
- `GET /api/stats` — running/queued/succeeded/failed/uptime_secs
- `DELETE /api/tasks/:id` — cancel task via HTTP (proxied to pool cancel)
- `GET /ws` — WebSocket endpoint with full state snapshot on connect, then `AgentEvent` stream; `/ws/tasks` retained for compat
- `lopi cancel <task-id>` — CLI cancel via HTTP DELETE to running sail server
- `lopi learn [--limit N]` — pretty-print mined patterns table (keywords / avg_attempts / success% / last_seen)
- `lopi dock` — pretty table output (ID / Goal / Status columns)
- `lopi run` — streams live `StatusChanged` + `LogLine` + `ScoreUpdated` events to stdout

### Changed
- `EventBus<T>` remains in lopi-core/event.rs alongside `AgentEvent` and `LogLevel`
- `lopi sail` now passes `Arc<AgentPool>` to web server; pool boots as background task
- `lopi-ui::web::serve()` signature: takes `Arc<AgentPool>` instead of raw bus
- All existing tests pass (38 total, 0 failures)

### Tests
- lopi-core: +2 tests (`agent_event_log_helpers`, `agent_event_serde_round_trip`) → 14 total
- All others unchanged: lopi-git (3), lopi-orchestrator (5), lopi-memory (11), lopi-webhook (5)
- **Total: 38 tests, 0 failures**

## [0.3.0] — Remote control + self-improvement

### Added
- `POST /api/tasks` — inject tasks into the live AgentPool queue with `goal`, `priority`, `allowed_dirs`, `max_retries`; returns `{id, goal, queued, duplicate_of}`
- `GET /api/tasks/:id` — fetch a specific task by full or prefix ID
- `GET /api/patterns` — expose mined patterns ordered by success rate
- Telegram: `/urgent <goal>` command for `Priority::High` tasks; inline keyboard (priority bump / cancel) on every queued task; `CallbackQuery` handler for button responses
- GitHub webhook: HMAC-SHA256 verification via `X-Hub-Signature-256` header; returns 401 on failure; constant-time comparison
- `MemoryStore::mine_patterns()` — extracts sorted keyword fingerprint from goal, upserts running averages into `patterns` table after each completed run
- `MemoryStore::load_patterns(limit)` — returns patterns ordered by `success_rate DESC`
- `AgentPool::with_store(store)` — attaches memory for pattern mining and `mark_completed` after each agent run
- `hmac`, `sha2`, `hex` added as workspace dependencies

### Changed
- `lopi_ui::web::serve()` now takes `TaskQueue` as third argument (task injection)
- `AppState` in `lopi-ui` now holds a `TaskQueue` handle
- `AgentPool::new()` signature unchanged; optional store via `with_store()`
- `main.rs`: `lopi sail` passes queue to both pool and web server; store attached to pool

### Tests
- lopi-memory: +4 tests (mine_patterns insert, upsert dedup, short-word skip, load ordering)
- lopi-webhook: +5 tests (valid HMAC, wrong secret, tampered body, missing prefix, empty sig)
- Total: 36 tests, 0 failures

## [0.2.0] — Live concurrency + test foundation

### Added
- `lopi-core::EventBus<T>` — thin tokio broadcast wrapper for workspace-wide event fanout
- `TaskStatus` is now `Clone + PartialEq` (derived in lopi-core)
- `AgentRunner::standalone()` — creates its own isolated bus for `lopi run`
- `AgentRunner::new()` — takes a shared `EventBus<TaskStatus>` for pool integration
- `AgentPool` now receives and propagates the shared bus to every spawned runner
- `lopi sail` boots the `AgentPool` as a background task; exposes `/ws/tasks` WebSocket endpoint
- WebSocket handler fans out serialized `TaskStatus` JSON to all connected clients; handles lag gracefully
- `lopi run` streams live status events to stdout while the agent executes
- `lopi tail --history` shows past tasks from SQLite; `--task-id` filters by prefix
- `ClaudeCode` upgraded to use `--output-format json` with `ClaudeOutput` struct and transparent fallback for older CLI versions
- `MemoryStore::open_in_memory()` for test isolation
- `MemoryStore::task_count()` helper
- 27 tests across lopi-core (12), lopi-git (3), lopi-orchestrator (5), lopi-memory (7)

### Changed
- `lopi-ui::web::serve()` now takes `EventBus<TaskStatus>` as second argument
- `lopi-orchestrator::AgentPool::new()` now takes `EventBus<TaskStatus>`
- `lopi-core` dependency added to `lopi-ui` and root binary

All notable changes to lopi.

## [0.1.0] — Initial scaffold

### Added
- Cargo workspace with 8 crates: `lopi-core`, `lopi-git`, `lopi-agent`, `lopi-memory`, `lopi-orchestrator`, `lopi-ui`, `lopi-remote`, `lopi-webhook`
- `lopi-core` types: `Task`, `TaskId`, `TaskStatus`, `Priority`, `TaskSource`, `AgentRun`, `Attempt`, `AgentState`, `Score`, `LopiConfig`
- `lopi-git`: `GitManager` (real git2 integration: branch, rollback, commit, PR via `gh`) + `DiffChecker` with allow/forbid glob enforcement
- `lopi-agent`: `AgentRunner` with the full Plan → Implement → Diff-check → Test → Score → Fix → Retry → PR loop
- `lopi-memory`: sqlx SQLite store with `tasks`, `attempts`, `patterns` tables
- `lopi-orchestrator`: priority `TaskQueue` (with goal-dedup) and `AgentPool` (Semaphore-bounded)
- `lopi-ui`: ratatui TUI dashboard + axum JSON API + minimal static dashboard
- `lopi-remote`: teloxide bot (`/help /task /status /approve`) + Twilio WhatsApp webhook
- `lopi-webhook`: GitHub webhook receiver that injects high-priority fix tasks on CI failure
- CLI binary `lopi`: `run | watch | tail | dock | sail`
- Docs: CLAUDE.md, KONJO_PROMPT.md, PLAN.md, README.md, lopi.toml.example
