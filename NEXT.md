# Next — Loop-Stack UI (net-new frontend work)

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

## What's next: UI-2 (card controls + guardrails/evals popovers)

UI-1 shipped: the `/stacks` route, `stores/stack.ts` (pure ops + composer
grammar parser, unit-tested), static prompt-card rendering (preset pill,
spec line, read-only guardrails/evals summary lines), the fused creation
flow (type-first + suggested chip + preset grid + inline grammar), and the
model/effort/repo/autonomy selector row. See `LEDGER.md`'s UI-1 entry for
the `/stacks`-vs-`/loop` route decision, the stack store shape, and why
eval suites are client-side static config this slice.

**UI-2 is card controls**, wiring the buttons UI-1 shipped disabled:
- **Loop pill + steppers** — toggle/adjust `StackCard.loopN` (×N / ∞), backed
  by the already-tested `reorderCard`/array-position logic; the field itself
  (`loopN`) already exists on `StackCard`.
- **Cron popover** — reuses `ScheduleEntry.cron` + the existing `api.ts`
  Schedule CRUD (`createSchedule`/`updateSchedule`, already round-trips) and
  the freq-pill ⇄ raw-cron two-way sync pattern from
  `docs/ui/lopi-loop-stacks-4-evals.html`'s `openSched`/`cronToHuman`/
  `recompute`. Not blocked.
- **Duplicate / drag reorder / delete / insert** — wire the card-bar buttons
  to `duplicateInStack`/`reorderInStack`/`removeFromStack`/`insertIntoStack`,
  already implemented and unit-tested in `stores/stack.ts`. Drag itself
  (HTML5 `dragstart`/`drop`) is new UI work; the array ops it calls are not.
- **Guardrails popover** (shield button) — budget/max-iterations/on-fail/
  schedule editor. **Needs backend first**: `gate`/`until`/`on-fail` have
  *zero* backend representation anywhere (confirmed by `UI_PLAN.md`'s direct
  source search — not on `Task`, not on `LoopConfig`). Per the scope doc's
  flagged reconciliation, model these as an `EvalDef` (test-tier), not a
  revived `gate_cmd`/`until_cmd` scalar pair — `on-fail` and `budget` presets
  are the only guardrail fields that could bind to something real today
  (`LoopConfig.budget_tokens`, though the 3-preset vocabulary doesn't exist
  yet either). Ship the popover against local-only card state (or hide the
  fields with no backend) until these land.
- **Evals popover** (check button) — flat-checklist editor over
  `StackCard.evals`. **Client-only until eval execution exists**: no
  `EvalDef`/`EvalSuite` backend concept exists, so toggling a check can only
  ever mutate the card's static list this slice — there is no run to attach
  a pass/fail/running state to. Build the popover UI now (toggle tiers,
  "add a suite" row, baseline locked-on) against `StackCard.evals` directly;
  wire real eval-run status when the backend eval ladder lands.

**Two backend gaps carried over from before UI-1, still blocking specific
UI-2/UI-3 controls — flagging so they aren't assumed solved by this
sprint's field exposure:**
- **Gate / until / on-fail** (the Limits popover's shell-command precondition,
  loop-until-exit-0, and stop/continue/backoff policy) — confirmed to have
  *zero* backend representation anywhere (not on `Task`, not on `LoopConfig`).
  Needs its own design + sprint before the Limits popover can bind to
  anything real; ship it against local-only client state (or hide it) until
  then.
- **Live-control signals** (pause/drain/bump) — confirmed only `kill`
  (cancel) exists anywhere in the runner or web layer. Pause/drain/bump need
  a signal mechanism invented from scratch (there is no partial version to
  extend), which blocks the live-controls row in UI-3 entirely except for
  its kill button.

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
