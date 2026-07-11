# Next — Track A + B are built; Track C is the horizon

## macOS-Loop-Stacks-1 landed (`CHANGELOG.md` `[0.4.0]`, `LEDGER.md`) — what's owed

The native macOS Forge is now a Loop-Stacks cockpit (pure `macos/Lopi/Stacks/`
domain layer + the extended Forge UI; one `.forge` nav item). Two follow-ups,
both the same discipline as every macOS round:

1. **Live-verify (immediate).** Swift doesn't build on the Linux authoring host,
   so the ported Swift tests and the UI are written-not-built. On an M-series Mac:
   `cd macos && xcodegen && xcodebuild -scheme Lopi test` (runs
   `StackStoreTests`/`StackGoalTests`/`StackRunTests`), then the two live scenarios
   against `lopi sail` — a **bare pane** (single-card regression check vs. the Forge
   baseline screenshot) and a **multi-card stack** (connectors, dock, run-until-goal
   stop-reason banner). Fix any compile drift the Linux authoring couldn't catch.
2. **`iOS-Research-1` is now cheaper.** `StackStore`/`StackGoal`/`StackRun` and the
   whole `Stacks/` layer are already pure Swift (zero SwiftUI/AppKit) — R-1's
   shared-package-boundary question is now "extract this module," not "rewrite it."
3. **Wire the eval backend through the existing seam** when A1–B1's evaluator lands
   server-side: `acceptance`/`budget_tokens` are carried in the pure payload +
   tested, but intentionally not on the live `CreateTaskBody` yet (backend gap).

## NEXT_SESSION_PROMPT (read this first)

**Unify-2 has landed (`CHANGELOG.md` `[0.3.0]`, `LEDGER.md`'s Unify-2 entry):
one pane primitive, the orb as the only status vocabulary, a read-only Overview,
and a four-item nav (Loop Stack · Scheduling · Overview · Configuration). Router
and Patterns (web panel) are fully removed; the old Forge component tree
(`AgentGrid`/`AgentPane`/`SessionSidebar`) and eight routes are gone.**

### ⛳ Immediate next action — Wes's post-merge live checklist (NOT an agent task)
Structural proof shipped in-sprint; the **live** half is owned by the operator
because live `sail`-spawned `claude` cannot authenticate in the CI sandbox (see
the standing constraint in `LEDGER.md` — settled, do not re-litigate). Run
`cargo run -- sail --port 3000 --max-agents 4 --repo .` and confirm, all real,
no `?demo=1`:
- [ ] A single-card pane (Forge-style) shows real **orb motion** through actual
      phase transitions — not just a static color.
- [ ] A single-card pane is visually indistinguishable from the pre-Unify-2
      Forge baseline (composer + card + orb, no dock/connector).
- [ ] A multi-card stack still runs and looks unchanged from current Stacks.
- [ ] Two concurrent real sessions both appear and update live in Overview
      (cost/phase/elapsed), not just correct at mount.
- [ ] The nav shows exactly four items: Loop Stack, Scheduling, Overview,
      Configuration.
- [ ] No console errors, no dead links to any of the ten cut routes.

### Then — macOS-Parity-1 (its own sprint) needs
Carry these Unify-1/2 decisions into the native port; none were decided for
macOS yet:
1. Orb-everywhere + the single pane-primitive decisions ported to the native UI.
2. Drop `ConstellationsView` / `.constellations` from macOS `NavSection` to match
   Router's full removal.
3. **Open call, decide before assuming either way:** macOS currently makes
   Patterns / Health / Audit / Dead-Letter first-class nav sections. Should they
   collapse to match web's four-item nav, or does macOS intentionally keep a
   richer admin surface? Not decided in Unify-1/2 — make a real call first.

---

**Track A *and* B1 have shipped: A1 + A2 + A3 + B1 are all built.** B1
(goal-directed stacks) is the newest — see `CHANGELOG.md` `[0.2.6]` and
`LEDGER.md`'s B1 entry. A stack now **runs the chain until its acceptance
passes, or a stack-level stop reason fires** (`goal_met > budget > no_progress >
max_chain_loops`, mirroring A3 at chain scope). It's **frontend-only, additive,
off by default**: the dock's new goal toggle drives it; a stack with no goal
behaves exactly as before.

**The two decisions B1 settled (don't re-litigate; carry the reasons):**
1. **Binary run-until-goal shipped; stack-level gain-gating was deferred** —
   there is **no clean whole-chain rollback** (each card's task rolls back its
   *own* loop, commits/PRs independently; nothing snapshots/restores the
   aggregate repo state). Gain-gating needs that rollback, so it's the top B1
   follow-up **only if** a real whole-chain snapshot/restore lands first. Don't
   fake it.
2. **The stack-scope eval seam is a dedicated eval task**, because stacks are
   100% client-only (no server "stack"). The sequencer launches a task carrying
   the compiled stack `Acceptance` and reads its terminal status as the verdict.
   The honest refinement is a **pure `POST /api/evaluate` endpoint** that runs
   A1's `TieredEvaluator` against a repo with *no agent work* — the same
   `EvalContext` A1 builds at finalize, exposed statelessly. That removes the
   "the eval task runs an agent" caveat and lets the client read a real
   `EvalOutcome` (score + critique), not just pass/fail — build it if B1's eval
   path needs to be tighter.

**Open items carried forward:**
- **The live A2 measurement is still owed.** Cross-run reflection stays gated off
  (`reflect_cross_run`, default off) pending a *live* three-arm run (blind /
  within-run / cross-run) clearing the pre-registered 15 pp margin. See the A2
  note below. If it comes back marginal, **better retrieval (semantic vs
  Jaccard)** is the lever before re-measuring.
- **Stack-level gain-gating** — deferred here (see decision 1); needs whole-chain
  rollback first.
- **Whole-chain scheduling remains stubbed** (since Stack-1) — the dock's stack
  schedule toggle shows an honest "not yet enforced" hint; closing it needs
  `ScheduleSpec.goal: String` → `Vec<String>` server-side. Don't downgrade it to
  the single-card `scheduleStack`.
- **`budget` at stack scope is unenforced** — B1 keeps it in the stop-reason
  precedence but never trips it client-side (no observable token meter), the same
  honesty stance as Stack-1's stack budget.

**Track C (autonomous decomposition / project autonomy) is the horizon — still
held.** A stack can now pursue a goal (B1), which is C's precondition: a *proposed*
stack is only useful once stacks pursue goals. But don't start C until the team
decides the goal-directed single-stack experience isn't already the product —
the roadmap explicitly reserves the right to stop at B1. C needs a *proven* A3
(runaway-loop safety) and the governance controls (audit, permissions,
reversibility) before it touches anything real.

---

## Superseded — B1's original prompt (kept for provenance)

_B1 is now shipped (`[0.2.6]`); the section below was the prompt that drove it._

**Track A is complete: A1 + A2 + A3 have all shipped.** A2 (Reflection) is the
newest — see `CHANGELOG.md` `[0.2.5]` and `LEDGER.md`'s A2 entry. A loop now
**captures durable, rollback-safe learnings** from every rejected attempt
(SQLite `learnings` table, no score gate — a rolled-back attempt still yields its
lesson) and can **retrieve relevance-filtered, bounded learnings** (Jaccard ≥
0.3, deduped, hard cap 3) into the next planning prompt. It's gated behind
`LoopConfig::reflect_cross_run` (**default off**) because the §2 measured
reflect-vs-blind comparison could not be run live here, and even the deterministic
mechanism harness (`lopi-agent::reflection_harness`) shows cross-run's *marginal*
lift over the within-run reflection lopi already had is ~0 pp at realistic
retrieval precision (its real win is speed, not pass-rate). **The honest verdict
is recorded, not papered over.** A2 extended A1/A3's existing critique routing —
reused, not rebuilt — and does **not** fight the gain gate (it informs the
prompt; A3 still decides what counts as a gain).

**If you pick up A2 again:** the one thing needed to justify flipping
`reflect_cross_run` on by default is the *live* three-arm run (blind / within-run
/ cross-run) on real tasks scored by A1's executor, clearing the pre-registered
15 pp margin against blind retry. The harness is the regression guard that makes
re-running it cheap; run it live in an API-enabled environment before changing
the default. Do not flip it on faith — a simulated lift is not a live one.

**B1 is the next layer: a *stack* runs until its goal-evals pass or termination
fires.** Now that a single loop can gain, terminate on no-progress, check a goal
(A1), and reflect (A2), the stack sequencer can keep looping/advancing the chain
until the stack's own `Acceptance` is satisfied or a `StopReason` fires. B1 is
two things:

1. **The sequencer change.** Attach a stack-level `Acceptance` (reuse A1's
   schema verbatim at stack scope — this was designed for it) and make "run
   stack" mean "pursue this outcome": loop/advance until the stack's evals pass
   or the no-progress/budget guards (A3, already built) stop it, recording the
   stop reason. Don't turn A3's per-loop gain gate into the stack controller —
   B1 owns stack-level control; A3 stays the per-loop mechanism it reads.
2. **The stack goal surface in the dock.** The purple stack-control dock needs
   one more facet — the *goal* — next to loop/schedule/limits. That's where a
   stack's `Acceptance` is authored (the same eval-checklist UI, at stack scope).

**A2 (Reflection) shipped** (`[0.2.5]`) — it extended the critique routing that
already existed (the verifier + A1's `EvalOutcome.critique` → next attempt's
constraints) into durable cross-run learnings on `MemoryStore` (**`kohaku`/a
vector store still does not exist**; the `learnings` table is the substrate, and
its no-score-gate design deliberately fixes the silent-0.6-drop hole
`lessons.rs` had, per CLAUDE.md "no silent failures"). The reflect-vs-blind A/B
is built as a repeatable harness but the *live* run is still owed (see above) —
so reflection is on-tap behind a flag, off by default. More reflection means the
loop has more to *gain* from, so A2 compounds A3 once the live measurement earns
the default-on flip.

**Carry this honest limit forward (a permanent design constraint):** the judge
catches only gaming *visible in its inputs*. A1 passes the full diff into
`EvalContext` and fails missing metric readings closed, but input-completeness
is the standing rule for anyone adding an eval — put the signal in the inputs,
or make the criterion objective (route it to a deterministic tier / `MetricGate`).
The A3 corollary: **gain-gate on objective metrics; a judge-only "improvement"
within judge noise must never lock** — the same discipline, applied to progress.

---

## Deferred UI gaps (from Stack-1, still open)

**Eval execution is now DONE** (A1) — a card's evals compile into a real
`Acceptance` and the backend scores against it. The remaining Stack-1 gaps:

**What's still open, in priority order:**

1. **Whole-chain scheduling has no backend to wire to.** The dock's
   schedule toggle stores `config.scheduled`/`config.cron` and shows an
   honest "not yet enforced" hint — closing this needs
   `ScheduleSpec.goal: String` → `Vec<String>` (or an equivalent
   multi-goal cron concept) server-side, the same gap Backend-1's own
   `scheduleStack` ledger entry already flagged for the per-card case.
   Until then, don't let a future sprint quietly wire the dock's toggle
   to the existing single-card `scheduleStack` — that would silently
   downgrade "schedule the whole chain" to "schedule the bottom card",
   which is worse than leaving it stubbed.
2. **Stack (and per-loop) budget enforcement.** Both stay unenforced by
   design (Backend-1's Phase 0 escalation); no scalar `budget_tokens`
   field exists anywhere server-side yet.
3. **Pane creation has no UI affordance.** `duplicateStack` is currently
   the only way to get a third+ pane; there's no "+ new empty stack"
   button. `deleteStack` deliberately refuses to empty the last pane
   because of this — worth revisiting together if pane creation ever
   gets built.
4. **`bumpCard` still has no UI affordance** (carried over from
   Backend-1, unrelated to Stack-1, not touched here).
5. **Chain on-fail's `'backoff'` policy re-attempts the *next repetition*
   immediately** (no actual backoff/delay — there's no client-side timer
   primitive worth inventing for this yet). If a real pacing need shows
   up, revisit alongside whatever brings real budget/rate-limiting to the
   client sequencer generally, rather than bolting a one-off `setTimeout`
   onto just this path.

**Unchanged (still out of scope):** eval execution/enforcement (see
above — now the standing top gap), budget enforcement, multi-card live
output, cross-pane card drag, `needs-you` derivation, effort→
thinking-budget mapping, ratchet/beats-best, severity.

## Prior sprint history

Shell-1 shipped: Loop Stacks (`/stacks`) became the app's default view, and
every nav destination moved from a horizontal top-tab bar into a left
sidebar that's closed (off-canvas) by default — opened by a hamburger,
closing on scrim-click/Esc/tab-select. Forge (the old `/`) moved to
`/forge`; no page's internal behavior changed (verified by an empty `git
diff --stat` outside the four touched route files). See `LEDGER.md`'s
Shell-1 entry for the load-bearing decisions and `CHANGELOG.md`'s `[0.2.1]`
entry for the full diff. Its own open items (`SIDEBAR_MODE` rail-flip, no
open-state persistence, exact-or-sub-route highlighting) are all still
exactly as it left them — untouched by Stack-1.

Backend-1 shipped: the `/stacks` UI actually executes. Task identity
(`client_ref` round-tripping through the store), run-stack execution
(`stores/stackRun.ts`), pause/drain/bump control signals, and per-card
`AgentEvent` isolation (proven by a concurrent-cross-talk test) are all
real, tested, and manually verified against a live `lopi sail` instance —
including catching and fixing a 100%-reproducible empty-repo bug that
only a real run could have surfaced. See `LEDGER.md`'s Backend-1 entry and
`CHANGELOG.md`'s `[0.2.0]` entry for the full detail.

UI-2 (PR #64) and its V&V audit (PR #65) shipped and merged: two
independent `/stacks` panes with per-card popovers (schedule/guardrails/
evals), an inline config drawer, drag-to-reorder, connector insert-between,
and a live-output attachment wired to the real per-`task_id` transcript
feed — verified against the settled mockup via Playwright screenshots (one
real popover-positioning bug found and fixed), then independently audited
(GO for the backend phase, 2 escalations, 3 real test-coverage gaps
closed). See `LEDGER.md`'s UI-2 entry and `docs/ui/UI-2-VV-report.md` for
the full detail — both escalations (budget badge honesty, CI soft-fail
policy) were resolved as part of Backend-1's own Phase 0, not carried
forward again here.

Sprint 5 (Expose Loop Fields on `CreateTaskRequest`) shipped: `POST /api/tasks`
now accepts `verifier_required`/`verifier_model`/`verifier_effort`, `report`,
`max_iterations` (`0` = infinite), and new `Task.model`/`Task.effort`
overrides — all optional, all round-trip-safe, no behavior change when
omitted. `select_model` and pool `build_runner` honor `Task.model`/
`max_iterations` as explicit overrides over the heuristic/repo default,
mirroring `verifier_model`'s precedent. See `LEDGER.md`'s Sprint 5 entry for
the `max_iterations: 0`-is-infinite one-way-door decision, why `Task.effort`
is stored but not yet folded into any prompt (cache-hit-rate risk on the
direct-API path), and the two Task-field additions this sprint required
beyond pure exposure. `web/src/lib/api.ts`'s `CreateTaskOptions` mirrors the
new fields — types only, no UI binds to them yet.

**Resolved since the above was written:** the worktree stash at `stash@{0}`
has been dropped (proven redundant against `origin/main`'s own
`WorktreeManager` file-by-file before the drop; the 3 unique `docs/ui/*.html`
mockups it held were extracted first). The committed-code DRY violations
`dry_check.py` was flagging (794 → 12 raw matches, 46 → 4 pairs) have also
been fixed. See `LEDGER.md` for both.

**Known flaky tests (not fixed here — each is its own separate task):**
- `constellation::tests::qlearned_favours_highest_reward_member`
  (`lopi-orchestrator`) — RNG-seed-dependent, ~20% flake rate observed across
  repeated `cargo test --workspace` runs.
- `health::tests::sweeper_runs_periodically`
  (`crates/lopi-orchestrator/src/health.rs:479`) — races a background sweeper
  tick against a hardcoded 350ms real-clock `sleep` with no margin; asserts
  `Degraded` when it expected `Dead` under scheduler contention. Confirmed
  orthogonal to any work in this file this session (`git log` shows its last
  touch was the original health-monitoring feature commit, 3 commits back).
  Fix is to drive the sweeper's clock via `tokio::time::pause`/`advance`
  instead of a real sleep, but that's out of scope here.

## What's next: UI-2 (card controls + guardrails/evals popovers) — now UNBLOCKED end-to-end

UI-1 shipped: the `/stacks` route, `stores/stack.ts` (pure ops + composer
grammar parser, unit-tested), static prompt-card rendering (preset pill,
spec line, read-only guardrails/evals summary lines), the fused creation
flow (type-first + suggested chip + preset grid + inline grammar), and the
model/effort/repo/autonomy selector row. See `LEDGER.md`'s UI-1 entry for
the `/stacks`-vs-`/loop` route decision, the stack store shape, and why
eval suites are client-side static config this slice.

Since then, the **Guardrails: Gate / Until / On-Fail** sprint landed the one
backend gap that blocked the guardrails popover: `LoopConfig`/`Task` now
carry `gate: Option<String>` (precondition), `until: Option<String>`
(exit-condition), and `on_fail: OnFail` (`Stop`/`Continue`/`Backoff`), all
exposed on `POST /api/tasks` and mirrored (types only) in `api.ts`'s
`CreateTaskOptions`. See `LEDGER.md`'s new entry for the `gate`-vs-`until`
shape decision, why `OnFail::Stop` had to become a no-op (a hard
kill-test-#1 constraint, not a design preference — `Stop`/`Backoff` are
currently behaviorally identical as a result), and the `sh -c` shell-exec
choice.

**UI-2 (card controls, popovers, config drawer, live output, pane chrome)
shipped.** `/stacks` now renders two independent panes side by side, each
with its own composer (prompts prepend to the top, flowing down to the
executing loop at the bottom), a run-stack footer, and per-card:
- **Iteration pill + guardrails max-iter stepper**, sharing one
  `StackCard.maxIterations` field (`0` = ∞, floor 2 below which the stepper
  wraps to infinite). `stores/stack.ts` gained the pane-keyed store
  (`panes`/`insertIntoPane`/`applyToPaneCards` — the pre-flight gate's
  `stack.insert(stackKey, index, loop)`) on top of the existing pure
  single-array ops, which are unchanged.
- **Schedule popover** — `cron.raw` is WIRED to the same shape as
  `ScheduleEntry.cron`; presets (every-minute/hourly/daily/weekly/custom)
  two-way-sync with it, and a real (bounded, minute-simulated)
  `computeNextRuns` drives the next-runs footer — no fabricated dates.
- **Guardrails popover** — `gate`/`until`/`onFail` are WIRED to
  `CreateTaskOptions`; `budget` stays client-only (no backend field exists
  anywhere, not even the scalar `budget_tokens` — see `LEDGER.md`).
- **Evals popover** — client-only checklist over the full `EVAL_CATALOG`
  plus suite shortcuts (KCQF/security/research), baseline locked-on. No
  pass/fail state rendered anywhere, per the brief's honesty rule.
- **Config drawer** (not a popover) — five `Dropdown.svelte` selectors
  (model/effort/repo/branch/autonomy) overriding pane defaults; model/
  effort/repo are WIRED, branch/autonomy are client-only.
- **`StackConnector`** — dotted + cyan cadence badge when the card above is
  scheduled, sun budget badge otherwise (if budget ≠ auto), hover-reveal
  insert-between block calling `insertCardIntoPane`.
- **`StackOutput`** — genuinely wired to `stores/transcript.ts`'s existing
  per-`task_id` block feed (thinking/tools/status/assistant_text → the
  mockup's thinking/actions/tools/output categories), collapsed by default,
  5s orange flash on the combined running card + output block
  (`prefers-reduced-motion` disables it). Renders only when a card has both
  `status === 'running'` and a real `taskId` — which no card gets this
  slice, since run-stack execution is still `RunMenu`'s stub (see below).
- The `cardToTaskPayload` pure function proves the WIRED fields' round-trip
  into the real `createTask(goal, repo, priority, options)` shape by unit
  test, independent of whether anything calls it yet.

See `LEDGER.md`'s new UI-2 entry for the `max_iterations`/iteration-pill
sharing decision, why the config drawer is five live selectors (not
read-only chips + a secondary menu, unlike the mockup), and the
`stores/transcript.ts` reuse discovery for `StackOutput`.

## What's next: the two backend signals that unblock UI-3/UI-4

Both were already flagged in UI-2's pre-flight and remain exactly as
described — nothing this sprint changed their status:

- **Pause/drain/bump signals** (blocks `RunMenu`'s four actions — Run now /
  Run once / Schedule stack / Dry run — and the `.runmain` "run stack"
  button, all still no-op stubs with a `// TODO(backend)`). Only `kill`
  (cancel) exists anywhere in the runner or web layer. This is the natural
  next backend sprint: invent the signal mechanism, then wire `RunMenu` for
  real (it already opens/closes correctly and just needs its four handlers
  to stop being stubs).
- **Per-card `AgentEvent` routing** (which card produced this event) — no
  card/stack-id tag exists on any event variant; every variant still keys
  on `task_id` alone. `StackOutput` is already built against
  `stores/transcript.ts`'s real per-`task_id` feed, so the moment a card is
  submitted as a task and carries a real `taskId`, its live output lights up
  with zero further UI work — the only missing piece is the execution path
  that would assign that `taskId` in the first place (folds into the
  pause/drain/bump work above, since "run this card" is the same signal
  that needs inventing).

**Remaining backend gaps, for UI-3/UI-4/overview — unaffected by this
sprint, flagging so they aren't assumed solved:**
- **Live-control signals** (pause/drain/bump) — confirmed only `kill`
  (cancel) exists anywhere in the runner or web layer. Pause/drain/bump need
  a signal mechanism invented from scratch (there is no partial version to
  extend), which blocks the live-controls row in UI-3 entirely except for
  its kill button.
- **Per-card event routing** (which card produced this `AgentEvent`) — no
  card/stack-id tag exists on any event variant. Blocks true
  multi-card-per-pane output attachment in UI-4; the documented fallback
  (one active card per pane, route by `task_id` alone) unblocks a
  single-active-card version without this.
- **Needs-you derivation** (verifier-fail / failing test-tier eval /
  `:escalate` severity → one triage signal) — nothing today derives this as
  a single state; the overview's one piece of real backend work (everything
  else there is aggregation over existing state).

Sprint 4 (Verifier as Explicit Gate) shipped: `LoopConfig`/`Task` gained
`verifier_required` / `verifier_model` / `verifier_effort`
(`#[serde(default)]`, round-trip-safe), `VerifierAgent::verify` is
parameterized (`model: &str, effort: Option<&str>`) instead of hardcoding
`MODEL_OPUS`, a pure `resolve_verifier` resolver enforces "never grade your
own homework" (defaults to a model that differs from the worker's), and pool
construction (`crates/lopi-orchestrator/src/pool/run_loop.rs`'s new
`build_runner`) now calls `.with_verifier()` — its first real call site ever
— when the gate is set. See `LEDGER.md`'s Sprint 4 entry for the
never-grade-your-own-homework default, why `verifier_effort` is a prompt hint
rather than a wire parameter, the pool-construction kill-test seam, and what
now exercises the previously-dead `.with_verifier()` path.

**All four recon capabilities from `PROMPTS_PLAN.md` are now landed:**
Prompt Templates (Sprint 1), Skill Arguments (Sprint 2), Report on Finish
(Sprint 3), and Verifier as Explicit Gate (Sprint 4). The recon punch list is
complete.

## What's next: the loop-stack UI, not more backend

The next body of work is **net-new frontend** — the Loop Engineering
cockpit surfaces (`web/src/lib/components/AgentPane.svelte` et al.) need to
expose the levers that now exist on the backend but have no UI:
`autonomy_level`, `verifier_required` / `verifier_model` / `verifier_effort`,
`report`, `promote_after` / `trust_ceiling`, `isolation`, and the skill/rule
enable lists on `LoopConfig`. `docs/LOOP_ENGINEERING.md`'s roadmap already
ranks "LoopConfig write path / editor" (`PATCH /api/loop-engineering` + a
config-editor UI) as the top impact-to-effort item — that's the natural
starting point.

This is explicitly **not** a backend sprint: no new `Task`/`LoopConfig`
fields, no new crate edges, no new gate-wiring. The schema surface this
sprint (and the three before it) built is the contract the UI now needs to
read and write against `web/src/lib/stores/`, `AgentPane.svelte`, and
whichever API handlers in `crates/lopi-ui/src/web/loop_handlers.rs` /
`schedule_handlers.rs` don't yet expose these fields. Audit those handlers
first — some of `LoopConfig`'s existing fields (e.g. `promote_after`,
`trust_ceiling`) may already lack API exposure, in which case the UI sprint
starts with closing that gap before it can build the editor.
