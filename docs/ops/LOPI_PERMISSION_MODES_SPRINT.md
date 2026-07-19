# Sprint: Permission Modes — Web (Phase 1)

## Objective

Replace the unconditional `--dangerously-skip-permissions` on every `claude -p`
spawn with a per-task `permission_mode` the operator picks from a dropdown in
the web config popover, matching Claude Code's own mode selector. Default
value preserves current behavior exactly — this is an opt-in loosening of
autonomy, not a silent behavior change.

## Grounding (read before touching code)

Current hardcoded call sites, all with `.arg("--dangerously-skip-permissions")`,
all with a comment explaining why it's unconditional today:

- `crates/lopi-agent/src/claude.rs:145` — `run_streamed`
- `crates/lopi-agent/src/claude.rs:356` — `run` (backs `fix()`/`implement_step()`)
- `crates/lopi-agent/src/claude_stream.rs:71` — `plan_streaming` (speculative path)

Shared cap-injection point already exists for `--model`/`--effort`/`--max-turns`/
`--max-budget-usd`/`--allowedTools`/`--disallowedTools`:
`crates/lopi-agent/src/claude_support.rs:101` (`apply_cli_caps`). Its own doc
comment says `--dangerously-skip-permissions` was kept per-site "their
positions/doc comments differ enough not to share" — revisit that call for
this flag rather than assuming it still holds.

`with_*` builders live in `crates/lopi-agent/src/claude_builders.rs`
(`with_model`, `with_effort`, `with_max_turns`, etc.) — same file the new
`with_permission_mode` belongs in, per the file-size-gate convention noted at
the top of `claude.rs`.

The builder chain that assembles a session lives in
`crates/lopi-agent/src/runner/run_loop.rs:102-121`, right where `with_effort`
reads `self.task.effort`. `permission_mode` follows the same read.

Wire format: `CreateTaskRequest` in `crates/lopi-ui/src/web/types.rs` — add a
field the same way `effort`/`model` are declared (`#[serde(default)]`,
doc comment mirroring the `lopi_core::Task` field). `apply_loop_fields` in
`crates/lopi-ui/src/web/handlers.rs` is where it gets copied onto `Task`.

Frontend wire type: `CreateTaskOptions` in `web/src/lib/api.ts:91`. Per-loop
override type: `CardConfig` in `web/src/lib/stores/stack.ts:158`. Both already
carry `model`/`effort` with the exact optional-field-omit-when-unset pattern
this new field should copy (`stack.ts:1303` `cardToTaskPayload`,
`stack.ts:1388` `paneSubmitPayload`).

UI surfaces to extend, both using `Dropdown.svelte` on the same
icon/dense/accent-color pattern already used for model/effort/repo/branch/
autonomy:

- `web/src/lib/components/stacks/StackConfigPopover.svelte` — stack-level
  default, "sliders" button popover.
- `web/src/lib/components/stacks/ConfigDrawer.svelte` — per-loop override,
  inline drawer under a card.

Option catalogs live in `web/src/lib/stores/options.ts` (`MODEL_OPTIONS`) and
`web/src/lib/stores/stackDefaults.ts` (`AUTONOMY_OPTIONS`) — same `Option`
shape (`value`/`label`/`hint`) the new `PERMISSION_MODE_OPTIONS` should use.

## Two known landmines, already in the repo, not part of this sprint

1. **`autonomy` is decorative today.** `CardConfig`'s doc comment
   (`stack.ts:154`) says outright: `"autonomy is client-only — backend gap,
   not yet exposed."` The dropdown exists in both popovers, edits state, and
   goes nowhere. Don't let `permission_mode` accidentally follow the same
   fate — it must round-trip through `cardToTaskPayload` into a real
   `CreateTaskRequest` field, not just live in `CardConfig`.

2. **`require_plan_approval` already exists server-side and is fully
   unwired on the web.** `Task.require_plan_approval` (`lopi-core/src/task.rs:215`)
   and `CreateTaskRequest.require_plan_approval` (`types.rs:29`) both exist;
   `crates/lopi-agent/src/runner/plan_gate.rs` implements a real
   channel-based human-approval gate on the first attempt's plan
   (`AgentEvent::PlanProposed` → `TaskStatus::AwaitingPlanApproval` → 1-hour
   wait → auto-reject on timeout). None of this reaches `CreateTaskOptions`
   or any Svelte component. This is architecturally the closest thing lopi
   has to Claude Code's own `plan` mode — but it's lopi's own gate, not the
   CLI's `--permission-mode plan`, and it needs its own approve/reject UI
   before it's safe to expose (see Out of Scope). Don't build a "plan" entry
   in this sprint's dropdown that silently routes to this field — it isn't
   ready.

## Pre-flight kill-tests (hard gates, live proof required — M3 machine, real
Max subscription, not CI-sandboxed)

Run these before writing implementation code. Any failure changes scope, not
just implementation detail.

**KT1 — `auto`/`dontAsk`/`acceptEdits` don't stall headless.**
Docs claim `auto` mode aborts cleanly under `-p` on repeated classifier block
rather than stalling, and `dontAsk` denies-not-stalls. Verify live: run
`claude -p "<goal>" --permission-mode auto` and `--permission-mode dontAsk`
against a real lopi-managed repo with a task that needs at least one Bash
command outside the read-only/filesystem set. Confirm the process exits
(success or clean failure) within `CLI_SESSION_TIMEOUT_SECS`, never hangs to
the `--max-turns` ceiling the way an unanswerable prompt would.

**KT2 — `acceptEdits` + existing `permission_allow` avoids stalling on a real
task.** Pick a repo with a populated `permission_allow` list
(`.lopi/loop.toml`). Run a task under `--permission-mode acceptEdits` that
needs a command matching that allow list (e.g. `cargo test`). Confirm it
completes without a permission prompt. This is the one mode whose headless
safety depends on repo config, not just the mode itself — if it stalls even
with allow rules present, `acceptEdits` isn't safe to ship as a selectable
option yet.

**KT3 — `--permission-mode bypassPermissions` is a true drop-in for
`--dangerously-skip-permissions` on the pinned CLI version.** Confirm both
flags produce identical behavior (including the root/sudo refusal check) on
whatever `claude` CLI version is actually installed where lopi runs — don't
trust the docs' "equivalent" claim without checking the pinned version's
changelog for drift.

**KT4 — `auto` mode account eligibility.** Confirm the account lopi's CLI
authenticates as actually meets `auto` mode's requirements (model, provider,
plan, and on Team/Enterprise an Owner toggle) *before* wiring UI around it.
If ineligible, decide now whether the option is hidden, disabled with a
tooltip, or shown and left to fail at spawn time with a surfaced error — pick
one, don't leave it implicit.

**KT5 — container root check.** `Dockerfile:74` sets `USER lopi`, so
`bypassPermissions`'s root/sudo refusal shouldn't fire in the deployed
container — but confirm this at runtime (`whoami` inside the actual running
container/fly.toml deploy), not just by reading the Dockerfile, since a
compose override or fly.toml directive could change the runtime user without
touching this file.

## Phase 1 — Backend: thread `permission_mode` end to end

1. Add a `PermissionMode` type to `lopi-core` (new file or alongside
   `autonomy.rs`), exposing exactly the four values validated safe by the
   kill-tests above. Serialize to the literal strings the CLI's
   `--permission-mode` flag accepts (`"bypassPermissions"`, `"dontAsk"`,
   `"auto"`, `"acceptEdits"`) — not a snake_case translation that then needs
   a lookup table at the spawn site. Default: `BypassPermissions`, so an
   absent field reproduces today's behavior exactly.
2. Add `permission_mode: PermissionMode` to `Task` (`lopi-core/src/task.rs`),
   defaulted the same way, doc-commented the way `require_plan_approval` is
   at line 215 — including a note that this is a genuinely different axis
   from `require_plan_approval` (CLI tool-execution permission vs. lopi's own
   plan-approval gate) and from `autonomy_level` (PR/merge behavior, not
   execution-time permission at all).
3. Add `permission_mode: Option<String>` to `CreateTaskRequest`
   (`crates/lopi-ui/src/web/types.rs`), validated against the four accepted
   values at request time — reject unknown values with 422, the same
   pattern `ReportChannel::parse` uses for `report` (see the doc comment at
   `types.rs:43`), never silently drop or coerce.
4. Wire it onto `Task` in `apply_loop_fields`
   (`crates/lopi-ui/src/web/handlers.rs`), alongside where
   `require_plan_approval` is set at handler line 380.
5. Add `permission_mode: Option<String>` field to `ClaudeCode`
   (`crates/lopi-agent/src/claude.rs`) and a `with_permission_mode` builder
   in `claude_builders.rs`, following `with_effort`'s validate-and-drop
   pattern exactly (`claude_builders.rs:24`).
6. Replace the three hardcoded `--dangerously-skip-permissions` args
   (`claude.rs:145`, `claude.rs:356`, `claude_stream.rs:71`) with a
   conditional emission of `--permission-mode <value>`, defaulting to
   `bypassPermissions` when unset. Decide during implementation whether this
   folds into `apply_cli_caps` (one injection point, matches the flag's
   nature as a shared cap) or stays per-site (matches the existing stated
   rationale) — document whichever is chosen and why in the same doc comment
   style already used at `claude_support.rs:93-100`.
7. Thread `self.task.permission_mode` into the `ClaudeCode` builder chain in
   `run_loop.rs`, next to the existing `.with_effort(...)` call
   (`run_loop.rs:111`).
8. Verify: unit test each of the three spawn sites emits the correct
   `--permission-mode` value (or the correct default) for each enum variant,
   mirroring the existing `plan_streaming_forwards_all_caps_to_the_subprocess_argv`
   test pattern in `claude_stream.rs:159`. Verify a request with an
   unrecognized `permission_mode` string is rejected with 422, not silently
   defaulted.

## Phase 2 — Web: surface the dropdown

1. Add `PERMISSION_MODE_OPTIONS: Option[]` to `web/src/lib/stores/options.ts`
   (or `stackDefaults.ts`, matching wherever `AUTONOMY_OPTIONS` lives),
   labeled in plain operator language, not CLI jargon:
   - `bypassPermissions` — "Bypass · no prompts, full autonomy (current
     default)"
   - `auto` — "Auto · model reviews each action, blocks anything risky"
   - `acceptEdits` — "Accept edits · file edits auto-approved, everything
     else needs an allow-list entry"
   - `dontAsk` — "Locked · only pre-approved commands run, everything else
     denied"
2. Add `permission_mode?: string` to `CardConfig` (`stack.ts:158`) and to
   `CreateTaskOptions` (`api.ts:91`), doc-commented the same way `effort` is,
   noting explicitly (unlike `autonomy`) that this one is wired end to end.
3. Wire `cardToTaskPayload`, `cardToTaskPayloadForRunOnce`, and
   `paneSubmitPayload` (`stack.ts:1303`, `1352`, `1388`) to include
   `permission_mode` when set, using the same
   `if (x && x !== default) options.permission_mode = x` omit-when-default
   pattern `effort`/`model` already use — never send the literal default
   string on the wire when the field wasn't touched.
4. Add a `Dropdown` row to `StackConfigPopover.svelte` and `ConfigDrawer.svelte`,
   matching the existing model/effort/repo/branch/autonomy rows exactly
   (icon, `dense`, its own `--konjo-accent-rgb`). Add an icon to `ICONS` if
   one doesn't already fit (`./icons`).
5. Update the "config active" / summary-string helpers
   (`stack.ts` — `configActive` around line 1095, the summary builder around
   line 1235) so a non-default `permission_mode` shows up in the same
   "what's overridden" indicator `model`/`effort`/`autonomy` already
   populate.
6. Verify: extend `stack.test.ts`'s existing round-trip proof (the one
   `cardToTaskPayload`/`paneSubmitPayload` shape-equality test the doc
   comment at `stack.ts:1387` references) to cover `permission_mode`.

## Out of scope / non-goals for this sprint

- **Interactive relay for `default`/manual and `plan` modes.** These need
  every tool call to round-trip through a live human decision, which
  headless `-p` has no channel for today. `plan_gate.rs` proves lopi *can*
  build this kind of relay, but generalizing it to every tool call (not just
  the first-attempt plan) is a distinct, larger feature — not a dropdown
  addition. Do not add these as selectable options that quietly degrade to
  something else.
- **Wiring `require_plan_approval` into the frontend.** Real, already
  server-side, genuinely close to what "plan mode" means to an operator —
  but exposing it before confirming (or building) an approve/reject UI for
  `AwaitingPlanApproval`/`PlanProposed` would let an operator flip a toggle
  that silently strands their task for an hour and auto-rejects it. Flagging
  this as the most likely next sprint, not doing it here.
- **Fixing `autonomy`'s existing backend gap.** Pre-existing, unrelated to
  this feature, shouldn't be bundled into the same diff.
- **`.lopi/loop.toml` repo-level `default_permission_mode`.** `LoopConfig`
  already carries `permission_allow`/`permission_deny`; a repo-level default
  mode is a reasonable follow-up but touches TOML schema and the
  `/api/loop-engineering` read surface, which has no write path today either.
  Out of scope for "start with web."
- **macOS/SwiftUI surface.** Explicitly web-first per the ask.
- **Any change to `permission_allow`/`permission_deny` semantics.** This
  sprint consumes them (KT2) but doesn't modify them.

## Post-flight

- `CHANGELOG.md`: new entry for the `permission_mode` field, both crates and
  web, noting the default preserves existing behavior.
- `LEDGER.md`: log the one-way-door decisions — enum value strings chosen to
  match the CLI's own literal flag values verbatim; default variant
  `BypassPermissions`; which of the two options (fold into `apply_cli_caps`
  vs. per-site) was chosen for the flag emission and why; the four-mode
  subset chosen as headless-safe and the kill-test evidence for each.
- `NEXT_SESSION_PROMPT.md`: stub the `require_plan_approval` frontend-wiring
  sprint (approve/reject UI first) and the `.lopi/loop.toml`
  `default_permission_mode` repo-level sprint as the two natural follow-ups.
- `VERSION` bump per the one-way-door convention.
