# Next — Loop-Stack UI (net-new frontend work)

## NEXT_SESSION_PROMPT (read this first)

UI-2 (PR #64) has been **audited and verified** — see `docs/ui/UI-2-VV-report.md`
for the full go/no-go and evidence. Verdict: **GO for the backend phase.**
Nothing in the shipped UI needed fixing; the audit closed three test-coverage
gaps (`stack.test.ts` 103→121) and found two things worth deciding before or
during the backend sprint, neither of which blocks starting it:

1. **Budget badge honesty** — `StackConnector.svelte` renders a
   `⏸ budget 200k`-style badge that visually reads as an enforced limit;
   nothing enforces it (confirmed absent from `cardToTaskPayload`'s output).
   Decide: hide it until a real budget field exists, or restyle it to read
   as unenforced intent (dashed border + explicit "(not enforced)", matching
   how the evals popover already avoids implying pass/fail).
2. **CI's Wall-2/Wall-3 gates are mostly soft-fail** (`continue-on-error`
   on cargo audit/deny/coverage/rustdoc, and even Wall-3's own "fail if
   BLOCKER" step) — pre-existing, repo-wide, unrelated to UI-2. Worth a
   deliberate decision on when to flip these to hard-fail, independent of
   this UI work.

The actual backend blockers UI-2 was built against (unchanged by the audit):

- **Pause/drain/bump signals** — unblocks `RunMenu`'s four actions (Run
  now/Run once/Schedule stack/Dry run) and the `.runmain` "run stack"
  button, all still stubs. Only `kill` (cancel) exists anywhere today.
- **Per-card `taskId` assignment** — the moment a card is actually submitted
  as a task (folds into the pause/drain/bump work above, since "run this
  card" is that same signal), `StackOutput` lights up for real with zero
  further UI work — it's already wired to the genuine per-`task_id`
  `stores/transcript.ts` feed, confirmed structurally unreachable today only
  because no card ever gets a real `taskId` (verified by grep: nothing
  outside `duplicateCard`'s explicit reset ever touches `card.taskId`).
- **Per-card `AgentEvent` routing** — same underlying gap as above, restated
  from the UI-2 brief's own §3: no card/stack-id tag exists on any event
  variant, every variant still keys on `task_id` alone.
- If "Schedule stack" gets wired as part of this sprint, it needs a
  `cardToTaskPayload`-equivalent mapping into the real `ScheduleBody` shape
  (`{name, cron, goal, repo, priority, enabled}`) — `StackCard` has no
  `name` field yet, so that's a small design decision to make first (see
  escalation §4.3 in the V&V report).

## Prior sprint history

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
