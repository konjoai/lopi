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

## What's next: UI-1 (static stack + selector row) — client-side, unblocked

Per `UI_PLAN.md`'s refined build-slice sequence, UI-1 (prompt-card rendering,
composer-at-top, model/effort/repo selector row) needs **no further backend
work** — it's a new client-side `stores/stack.ts` (an ordered array, no
server round-trip yet) plus restyled reuse of `LaunchControls.svelte` and
`ui/Dropdown.svelte`. Start there next, not with another backend sprint.

**Two backend gaps remain out of this sprint's scope, each blocking a
specific later UI-2/UI-3 control — flagging so they aren't assumed solved by
this sprint's field exposure:**
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
