# UI_PLAN.md — loop-stack UI recon (UI-0)

**Scope:** reconnaissance only, no code. Written after discovering and fixing a
repo-sync problem — see "Preflight" below before reading anything else.

## Preflight — read this first

The task brief assumed a frontend and backend state that **did not exist on
local `main`** at session start. Local `main` was 176 commits behind
`origin/main`. Every file the brief named (`web/src/routes/loop/+page.svelte`,
`Composer.svelte`, `LaunchControls.svelte`, `transcript/*`, `LoopConfig`,
verifier gate fields) is real — but only on `origin/main`. This plan is
written against `origin/main`, which is now also local `main` (synced during
this session via fast-forward merge).

**Also found:** local `main` had uncommitted/staged work implementing git
worktree isolation (`crates/lopi-git/src/worktree.rs`, `src/worktree_command.rs`,
etc.) that duplicates a *different, more complete* worktree isolation feature
already merged into `origin/main` (RAII `WorktreeManager`, slug-based naming,
`pool/mod.rs` + `pool/worktree.rs` split). Reapplying the stash after the sync
produced 10 conflicted files (1 add/add, 1 modify/delete, 8 content). Per your
choice, the sync was completed and the local worktree stash was preserved
unresolved at `stash@{0}` — **reconciling it is a separate, unrelated piece of
work from the loop-stack UI and is not addressed further in this doc.**

**Also found:** the `docs/ui/lopi-scope-and-test-plan.html` mockup's own
"Backend done/partial/todo" table is *stale relative to origin/main* — it
undersells what's shipped (e.g. it lists "Sprint 4 verifier gate" as "in
flight"; it's merged, with `verifier_model`/`verifier_effort` fully
configurable) and never mentions concepts origin/main already has that the
mockup could reuse (`AutonomyLevel` L1–L4, `IsolationMode`, `ScheduleEntry`
cron+report, a real `/api/repos` listing endpoint). Sections 2 and 3 below
supersede that table.

---

## 1. REUSE MAP

| Capability the loop-stack UI needs | Existing module (origin/main) | Verdict |
|---|---|---|
| Composer that adds to a stack | `web/src/lib/components/Composer.svelte` | **Extend** — currently bottom-pinned, single-shot submit (`onSubmit(text)`), no "add to top of stack" or card-creation concept. Reuse the input/submit mechanics; the "adds a card" behavior is new. |
| Model/effort/repo selector row | `web/src/lib/components/LaunchControls.svelte` + `stores/controls.ts` (`MODEL_OPTIONS`/`EFFORT_OPTIONS`/`PRIORITY_OPTIONS`, localStorage-persisted) | **Reuse as-is**, restyle. This is almost exactly the mockup's selector row already. No verifier-model/effort array exists — extend `controls.ts` for that one. |
| Generic dropdown widget | `web/src/lib/components/ui/Dropdown.svelte` | **Reuse as-is** — full keyboard nav, `dense` mode, outside-click close. Every mockup selector/popover option list can bind to this. |
| Cron scheduling (backend + API) | `crates/lopi-core/src/config.rs::ScheduleEntry` (`cron: String`), `crates/lopi-orchestrator/src/scheduler.rs` (`next_run_times`, `tokio-cron-scheduler`), `web/src/lib/api.ts` `Schedule`/`ScheduleBody` (enable/disable/run-now, `next_runs`) | **Reuse as-is** on the backend/API side. The Schedules route only exposes a raw cron text field today (`bind:value={form.cron}`) — the frequency-preset popover UI (min/hourly/daily/weekly/custom ⇄ raw sync) is net-new UI, but it writes to a field that already round-trips end to end. |
| Report-on-finish | `crates/lopi-core/src/report.rs::ReportChannel`, `Task.report`/`ScheduleEntry.report: Option<String>`, `AgentEvent::ReportReady` | **Reuse as-is** on backend. Not exposed on `CreateTaskRequest` (web API) or in any UI control yet — see Gap Map. |
| Verifier gate (toggle + model/effort) | `crates/lopi-core/src/loop_config.rs::LoopConfig{verifier_required, verifier_model, verifier_effort}`, mirrored on `Task`; `crates/lopi-agent/src/verifier.rs::resolve_verifier` ("never grade your own homework") | **Reuse as-is** on backend — this is a complete, non-hardcoded implementation. No UI control exists for it anywhere (LaunchControls has no verifier picker). Pure frontend gap. |
| Loop iteration count | `LoopConfig.max_iterations: u8` | **Extend** — exists but finite-only (`u8`, no ∞ sentinel). The mockup's "×∞" needs either a sentinel value (e.g. `0` or `u8::MAX` reinterpreted) or a new `Option<u8>` — a backend decision, not just UI. |
| Budget selector (auto/200k/none) | `LoopConfig.budget_tokens: u64` (0 = inherit), `web/src/lib/stores/budget.ts` `fleetBudget`/`PRESETS` | **Extend** — the budget *system* exists but is USD/hour fleet-wide, not a per-loop token-count enum. No `auto`/`200k`/`none` preset vocabulary exists at either layer. |
| Gate / until / on-fail | *(none found)* | **Missing entirely**, backend and frontend both. See Gap Map. |
| Prompt alias / template resolution | `crates/lopi-core/src/template.rs::resolve` (re-exported `resolve_template`), `Task::from_template` | **Reuse the function**, but **wire net-new** — CLI-only today (`src/run_command.rs`); zero web-layer consumer. |
| Skill invocation args (`:kcqf vectro`) | `crates/lopi-skill/src/invocation.rs::parse_invocation`, `Skill::render_body` | **Reuse the function**, **wire net-new** — same story, CLI-only. |
| Chain (`:a > :b`) | *(none found)* | **Missing entirely.** |
| Live output stream / event routing | `web/src/lib/stores/agentReducer.ts::reduce()` (20-variant `AgentEvent` fold → `activity`/`pressure`/`stimulus`), `stores/events.ts` (Pulse ring buffer + `describe(ev)` tier/summary) | **Reuse the fold**, **extend the routing**. This is the correct backbone for a running card's state, but no event carries a card/stack id — see Gap Map. |
| Tool-call / output accordion | `web/src/lib/components/transcript/{Transcript,ToolCall,StatusChip,CodeBlock,Markdown}.svelte` | **Extend** — `ToolCall.svelte` is already a collapsed-by-default accordion per call; `Transcript.svelte` already has a `thinkingOpen` toggle with a scroll-capped think body (unbounded height today, no `max-height`). The mockup's single 4-section (thinking/actions/tools/output) aggregate accordion with filter chips has no direct precedent — assemble from these primitives, don't build from scratch. |
| Orb / running-pulse visual language | `web/src/lib/forge/{Forge.svelte, orbState.ts, excitement.ts}` | **Reuse as-is**, orthogonal — no card has an orb in the mockup, but the "running pulse outline" visual idiom is exactly what `orbState.ts`'s phase-color + `excitement.ts`'s stimulus-decay envelope already drive. Consider borrowing the animation language, not the component. |
| Pane layout / add-pane | `web/src/lib/stores/layout.ts` + `layout-core.ts` (pane slots, drag-swap, tombstone, close-vs-delete, `window.dispatchEvent('lopi:add-pane')`), `TileGrid.svelte` (auto-tiling, drag-resizable gutters) | **Reuse the store**, **extend the affordance** — add/remove-pane logic exists and works; the mockup's hover-reveal edge tab does not exist (`TileGrid.svelte` only has resize gutters, no hover-add zones). |
| Repo selector | `crates/lopi-ui/src/web/repos_handlers.rs` (`GET /api/repos`, `GET /api/branches?repo=`) | **Reuse as-is** on backend — a real listing endpoint exists. `CreateTaskRequest.repo` is still a raw free-text string not validated against it, and no frontend dropdown consumes it yet (LaunchControls has repo as a text input). |
| Generic card chrome (bordered panel, stat tiles, sparkline) | `web/src/lib/components/ui/{Panel,StatCard,Sparkline,EmptyState}.svelte`, `ui/badges.ts` | **Reuse as-is** for any static chrome the stack cards need. |

**Invented-but-already-exists flags:** the mockup's "cron popover" invents a
frequency-preset UI, but the underlying field, parser, and API are already
real (`api.ts` Schedule model) — this is a pure UI slice, not a backend
capability gap, contrary to how the scope doc framed it. Likewise "repo
selector" backend (`/api/repos`) already exists and the scope doc doesn't
mention it at all.

---

## 2. GAP MAP

| Mockup requirement | Smallest new component | Binds to |
|---|---|---|
| Loop-stack card model (spec + modifiers, ordered array, drag/dup/delete/insert) | New `StackCard` type + a `stores/stack.ts` (array of cards, index-based reorder) | No existing store models an ordered list of *pending* prompts — `agentReducer.ts` models *running* agents, not a queue-to-be-run. This is genuinely new state, not an extension. |
| Composer-adds-to-top | Extend `Composer.svelte` with an `onAddCard(text) => prependToStack()` mode, or a new `StackComposer.svelte` | `stores/stack.ts` (new) |
| Model/effort/repo selector row, icon+dropdown, colored | Restyle `LaunchControls.svelte` (cyan/ember/sun icon treatment) + swap repo text input for a `Dropdown` bound to `GET /api/repos` | `controls.ts` (extend) + `repos_handlers.rs` (reuse) |
| Verifier check button + slide-out model/effort picker | New `VerifierToggle.svelte` (a `Dropdown`-based slide-out, same shape as `LaunchControls`' existing pickers) | `Task.verifier_required/verifier_model/verifier_effort` (reuse, unexposed on `CreateTaskRequest` today) |
| Limits popover (gate/until/on-fail/budget) | New `LimitsPopover.svelte` | **Blocked** — no backend field for gate/until/on-fail exists at all (see Backend Bindings). Budget half-binds to `budget_tokens: u64`, needs a preset enum either client-side (map auto/200k/none → number, cheapest) or server-side. |
| Cron popover with frequency presets ⇄ raw sync | New `CronPopover.svelte` (pure client-side parser, same shape as the mockup's `cronToHuman`/`recompute`) | `ScheduleEntry.cron` (reuse, already round-trips via `api.ts`) |
| Run split (run now / once / schedule / dry run) | New `RunSplitButton.svelte` | "Run now" → existing task-create path. "Schedule" → existing `ScheduleEntry` create. "Dry run" → **blocked**, see below. "Run once" has no existing analog (a schedule that fires once — `ScheduleEntry` implies recurring cron) — needs a design decision, not just UI. |
| Live controls (pause/drain/bump/kill) | New `LiveControls.svelte` | Kill/cancel reuses `AgentPool::cancel`/`cancel_by_prefix`. **Pause/drain/bump have no backend signal at all** — blocked, largest single backend gap (see Backend Bindings). |
| Collapsed→expand output attachment (2-line strip → 4-section accordion, thinking height-capped) | New `OutputAttachment.svelte`, composed from `ToolCall.svelte` (tool section) + a capped variant of `Transcript.svelte`'s think-body (thinking section) + `events.ts`'s `describe(ev)` (collapsed-strip summary line) | `agentReducer.ts`'s per-event fold (reuse) + **a card/stack-id tag on `AgentEvent`, which does not exist** — blocked for the *per-card* routing specifically; a single-agent-per-pane version could ship without it. |
| Hover-add-pane (slide-out tab L/R) | New hover-zone affordance in `TileGrid.svelte` or a wrapping component | `layout.ts`'s `addPane()`/`window.dispatchEvent('lopi:add-pane')` (reuse — the action already exists, only the hover-reveal UI is missing) |
| Chain (`:a > :b`) | New parser function, no component | **Missing entirely**, backend and frontend. Smallest version: a client-side split on `>` that enqueues two ordered stack cards — doesn't need a backend "chain" concept if the stack itself already sequences cards. |
| Per-card model/effort/repo override (shown only when ≠ stack default) | Extend the `StackCard` type with optional override fields; a conditional row under the spec line | `Task`'s existing model/effort/repo fields (reuse) — `CreateTaskRequest` already needs to gain these regardless of stack UI, see Backend Bindings |
| ×∞ infinite loop | Backend decision first (see Backend Bindings), then a UI-only rendering change (`∞` glyph vs. number) | `LoopConfig.max_iterations` (extend) |

---

## 3. BACKEND BINDINGS

Confirmed against `origin/main` (now local `main`), not the stale table in
`docs/ui/lopi-scope-and-test-plan.html`.

| UI control | Backend field / endpoint | State |
|---|---|---|
| Prompt spec (alias/literal/template) | `lopi_core::resolve_template`, `Task::from_template` | **Done**, CLI-only. Needs a web-layer call site (e.g. in the task-create handler, resolve `goal` server-side before persisting). |
| Skill arg (`:kcqf vectro`) | `lopi_skill::parse_invocation`, `Skill::render_body` | **Done**, CLI-only (`src/run_command.rs::resolve_skill_invocation`). Same gap as templates. |
| Loop ×N | `LoopConfig.max_iterations: u8` | **Partial** — finite only, no ∞. Needs a design decision: sentinel value vs. `Option<u8>`. |
| Cron schedule | `ScheduleEntry.cron`, `next_run_times`, full CRUD in `api.ts` | **Done**, fully wired end to end already. |
| Report on finish | `Task.report`/`ScheduleEntry.report`, `ReportChannel::parse`, `AgentEvent::ReportReady` | **Done** on backend (Telegram only; WhatsApp explicitly rejected, not silently dropped). **Not exposed** on `CreateTaskRequest` (web API) — small gap. |
| Verifier gate + model + effort | `LoopConfig`/`Task.verifier_required/verifier_model/verifier_effort`, `verifier.rs::resolve_verifier` | **Done**, fully configurable, "never grade your own homework" default. **Not exposed** on `CreateTaskRequest` — small gap. |
| Gate (must-pass-before-start shell command) | *(none)* | **Missing.** No `gate_cmd`/precondition concept anywhere in the codebase. |
| Until (loop-until-exit-0 shell command) | *(none)* | **Missing.** Nothing named until/exit-condition found. |
| On-fail (stop/continue/backoff) | *(none)* | **Missing** as a configurable policy. "Backoff" exists only as a fixed internal retry-timing constant (`runner/mod.rs`), not a user-selectable enum. |
| Budget (auto/200k/none) | `LoopConfig.budget_tokens: u64` (0=inherit), fleet-wide USD budget in `budget.ts` | **Partial** — a scalar token ceiling exists; the three-preset vocabulary (auto/200k/none) doesn't exist at either layer. Cheapest fix: client-side enum → number mapping, no backend change needed if `0` already means "inherit/auto." |
| Per-card model/effort/repo override | `Task.model`/`effort`/`repo_path` presumably exist on `Task` (verifier work proves the pattern) but **`CreateTaskRequest` (web API) doesn't expose them** | **Partial** — needs `CreateTaskRequest` extended; not a `Task`-level gap. |
| Duplicate / drag reorder / delete / insert (stack ops) | *(none — no stack concept exists server-side at all)* | **Missing entirely** — this is pure client-side stack-array manipulation until/unless a stack needs to persist server-side (e.g. surviving a page reload). Recommend starting client-only. |
| Run split · dry run | `AgentRunner.dry_run: bool`, wired through `run_loop.rs` | **Partial** — per-task dry-run exists (produces a plan, no execution). No stack-wide expansion/preview (resolve all aliases+templates+cost estimate across N cards without running any) — that's new. |
| Run split · run once / schedule stack | `ScheduleEntry` (recurring only) | **Partial** — "run now" and "schedule" (recurring) map cleanly; "run once" (a single future/immediate fire, not recurring) has no existing analog and needs a design decision. |
| Live controls · kill | `AgentPool::cancel`/`cancel_by_prefix` | **Done.** |
| Live controls · pause / drain / bump | *(none)* | **Missing entirely** — confirmed via targeted search of `crates/lopi-agent/src/runner/*` and `crates/lopi-ui/src/web/*`; no pause/resume/drain/iteration-bump signal exists in any form. This is the single largest backend gap blocking the live-controls row. |
| Per-card event routing (which card produced this event) | `AgentEvent` (21 variants, `crates/lopi-core/src/event.rs`) — every variant keys only on `task_id` | **Missing** — no card/stack-id tag exists on any event. Confirmed independently by both recon passes. Blocks true per-card output-attachment routing in a multi-card-per-pane layout; a single-card-per-pane version doesn't need this. |
| Repo picker | `GET /api/repos`, `GET /api/branches?repo=` (`crates/lopi-ui/src/web/repos_handlers.rs`) | **Done.** Not yet consumed by any frontend dropdown. |

---

## 4. TEST TOOLING REALITY

The frontend has **no test runner configured** — `web/package.json` defines
only `dev`/`build`/`preview`/`check` scripts. Existing tests (`parser.test.ts`,
`forge/connections.test.ts`, `forge/excitement.test.ts`, `forge/orbState.test.ts`,
`stores/agentReducer.test.ts`, `stores/events.test.ts`,
`stores/layout-core.test.ts`, `stores/session-groups.test.ts`,
`stores/transcript.test.ts`, `ui/badges.test.ts`, `render/markdown.test.ts`,
`api.test.ts`) are all **plain hand-rolled assertion scripts run via
`npx tsx <file>.test.ts`** (confirmed by `parser.test.ts`'s own header
comment) — no Playwright, no Vitest, no jsdom, nothing that mounts a Svelte
component or drives a DOM interaction. Every existing test is pure-logic:
given input, assert output, no rendering.

**What it would take to test interactive Svelte components:** none of the
current tooling can render a `.svelte` file, dispatch a click, or assert on
computed styles/DOM state — that requires either component-test tooling
(`@testing-library/svelte` + jsdom/happy-dom under Vitest) or full-browser
tooling (Playwright). Both are net-new dependencies; this repo currently has
zero.

**Recommendation, lightest path first:** do **not** default to Playwright.
Given the existing pattern (pure-function tests as plain `tsx` scripts,
zero framework), the lightest extension consistent with what's already here
is **Vitest + `@testing-library/svelte` + happy-dom** — it's a single
devDependency addition (`vitest` bundles the runner+assertions, replacing the
current pattern of hand-rolled `pass`/`fail` counters with the same spirit),
runs in Node (no browser install), and can mount/interact with a single
component in isolation (the mockup's per-component test list — Loop pill,
Cron popover, Limits popover, etc. — is exactly this shape: isolated
component, assert render + interaction + emitted event/store write).
Playwright would additionally be justified **only** for the drag-reorder and
cross-pane drag-to-mount interactions (`SessionSidebar.svelte` →
`AgentGrid.svelte` drag), which happy-dom cannot faithfully simulate
(`dragstart`/`dataTransfer` in a real browser). If drag interactions turn out
to be a small fraction of the surface, they can be smoke-tested manually
instead of paying for a second test runner. **This is a dependency decision —
flag it for approval before adding either.**

---

## 5. BUILD SLICES (refined from the scope doc's UI-1..UI-6)

The scope doc's sequence is directionally right but under-weights how much of
UI-1/UI-2 is pure client-side state (no backend blocker) versus how much of
UI-3/UI-4 is hard-blocked on missing backend signals. Refined:

- **UI-1 · Static stack + selector row.** Prompt card rendering (spec +
  modifier badges), the connector/next-tag, composer-at-top, model/effort/
  repo selector row. **Reuses:** `LaunchControls.svelte` styling,
  `Dropdown.svelte`, `ui/Panel.svelte`. **New:** `stores/stack.ts` (client-only
  ordered array — no backend needed yet). **Not blocked.**
- **UI-2 · Card controls, client-side.** Loop pill + steppers, cron popover
  (reuses the already-working `ScheduleEntry.cron` + `api.ts` Schedule CRUD),
  duplicate/drag/delete/insert (pure array ops on `stores/stack.ts`).
  **Not blocked**, except: the Limits popover's gate/until/on-fail fields have
  nothing to write to — ship the popover UI against local-only state (or hide
  it) until the backend fields exist. The verifier button **can** be built now
  — `Task.verifier_required/model/effort` exist — but needs
  `CreateTaskRequest` extended first (small, unblocks immediately).
- **UI-3 · Run + live controls.** Run split's "run now"/"schedule" modes are
  **not blocked** (existing task-create + schedule-create paths). "Run once"
  needs a design decision (no recurring-vs-once distinction exists in
  `ScheduleEntry` today). "Dry run" is **partially blocked** — per-task
  dry-run exists, stack-wide expansion preview doesn't. Live controls: kill
  is **not blocked**; pause/drain/bump are **fully blocked** on backend
  signals that don't exist anywhere — this is the sequence's biggest single
  dependency and should be scoped as its own backend sprint before UI-3 can
  ship complete (kill-only is a valid partial slice).
- **UI-4 · Output attachment.** **Blocked** on per-card event routing (no
  card/stack-id on `AgentEvent`) for the true multi-card-per-pane case.
  Unblocked fallback: if a pane runs one card at a time (matches the mockup's
  actual behavior — only the "next" card runs), route by `task_id` alone,
  since a pane only ever has one active task_id at once. Reuses `ToolCall.svelte`
  + `events.ts`'s `describe()` for the collapsed strip. The 4-section
  aggregate accordion and thinking-height-cap are new but assemble from
  existing transcript primitives — moderate effort, not blocked.
- **UI-5 · Multi-pane.** **Not blocked** — `layout.ts`/`layout-core.ts`/
  `TileGrid.svelte` already implement pane slots, drag-swap, add/remove. Only
  the hover-reveal edge-tab affordance is new UI.
- **UI-6 · Polish.** Unchanged from the scope doc — not blocked, no
  dependencies.

**Net change from the scope doc:** UI-1/UI-2 collapse to "mostly unblocked,
client-only" (the scope doc treated everything as needing backend work — most
doesn't). UI-3's live-controls row is a harder, more isolated blocker than the
scope doc implied (it's not "wire an existing signal," it's "invent the
signal"). UI-4 has a real fallback path (single-active-card-per-pane) that
avoids its stated blocker entirely, which the scope doc didn't consider.

---

## 6. KEEP/REPLACE VERDICT — `web/src/routes/loop/+page.svelte`

**Recommendation: stand up as a new route; do not replace in place.**

The existing `/loop` route (~380 lines) is not a stack-composer UI at all —
it's a read-only "Loop Engineering" cockpit: loop health stats/sparklines,
recent-runs drill-down, effective `.lopi/loop.toml` display, the Autonomy
Ladder (L1–L4), Self-Prompting Strategy picker (S1–S4 + escalation), a
schedules list (trust-level dropdown only), and quality gates. This is
**config/observability for the loop-as-code system** (`LoopConfig`,
`AutonomyLevel`, `SelfPromptStrategy`) — a genuinely different surface from
the mockup's interactive stack-of-prompts composer, which is closer in spirit
to the existing `AgentPane`/`Composer`/`LaunchControls` per-pane model than to
anything currently at `/loop`.

Two real routes would coexist post-launch: the loop-stack composer (probably
still living at `/loop`, or a new `/stacks` if `/loop` needs to keep its
current config/health meaning) and this existing cockpit page. Given the
existing page's content (health telemetry, autonomy ladder, gates) is
genuinely useful and unrelated to "compose and run a stack of prompts," the
cleanest path is: build the new stack UI as its own route, keep the current
`/loop` page's content available (rename the route if `/loop` is wanted for
the new UI — e.g. move today's page to `/loop/config` or fold its telemetry
into a tab within the new stack page later, once the new UI has parity on
what people actually use from it). Do not delete or overwrite the existing
380-line page as a side effect of building the new one.
