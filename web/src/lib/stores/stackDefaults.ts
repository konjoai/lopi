/**
 * Stack-level defaults — per-stack (per-pane) baseline for every card's
 * config-drawer override: model, effort, repo, branch, autonomy,
 * permission_mode. Owned by each `StackPaneState.config.defaults`
 * (`stores/stack.ts`), not a single global store — Stack-1 made this
 * per-pane so two panes can carry two different default configs (was a
 * single app-wide `writable` through UI-2/Backend-1). `model`/`effort`/
 * `repo`/`permission_mode`/`autonomy` are all real `CreateTaskRequest`
 * fields as of the web-composer loop.toml sprint (`autonomy` reaches
 * `CreateTaskRequest.autonomy_level` via `autonomyToWire`, below — see
 * UI_PLAN.md's Backend Bindings table). `branch` is not inert despite
 * having no `CreateTaskRequest` field of its own: `paneSubmitPayload` turns
 * it into a "Target branch: …" planning constraint.
 */
import { MODEL_OPTIONS, type Option } from '$lib/stores/options';

export interface StackDefaults {
  model: string;
  effort: string;
  repo: string;
  branch: string;
  autonomy: string;
  permission_mode: string;
}

/** The sentinel `StackDefaults.autonomy`/`CardConfig.autonomy` value meaning
 *  "no live choice — inherit the repo's `.lopi/loop.toml` `autonomy_level`."
 *  Mirrors `options.ts::AUTO_MODEL`'s exact convention: a fresh pane must
 *  never default to a concrete rung (e.g. hardcoding `'L2'`) because that
 *  value would then be sent on *every* task, silently overriding a repo's
 *  real configured autonomy — the precedence inversion this whole sprint's
 *  precedence contract exists to prevent. Only a live, deliberate user pick
 *  of `L1..L4` produces a wire override; `autonomyToWire` omits everything
 *  else, this sentinel included. */
export const AUTO_AUTONOMY = 'auto';

/** The real `AutonomyLevel` ladder (`crates/lopi-core/src/loop_config.rs`) —
 *  PR-flow semantics, not the mockup's mismatched "leash" copy (see
 *  UI_PLAN.md's flagged label mismatch). Mirrors `loop/+page.svelte`'s
 *  `ladderHint()` wording so the two surfaces read the same. `auto` is
 *  first and is the cold-start default (see `DEFAULT_STACK_DEFAULTS`) — it
 *  reads as "inherit from the repo's `.lopi/loop.toml`," never a hidden L2. */
export const AUTONOMY_OPTIONS: Option[] = [
  { value: AUTO_AUTONOMY, label: 'Auto · from loop.toml', hint: "inherit the repo's .lopi/loop.toml autonomy_level" },
  { value: 'L1', label: 'L1 · Report only', hint: 'report only, no PR' },
  { value: 'L2', label: 'L2 · Draft PR', hint: 'draft PR, human approves' },
  { value: 'L3', label: 'L3 · Verified PR', hint: 'verify before PR' },
  { value: 'L4', label: 'L4 · Auto-merge', hint: 'auto-merge on pass' }
];

/** The wire tag `CreateTaskRequest.autonomy_level` (and `Task::autonomy_level`
 *  server-side) actually deserializes — mirrors
 *  `lopi_core::loop_config::AutonomyLevel::tag_snake` exactly. */
type AutonomyWireTag = 'report_only' | 'draft_pr' | 'verified_pr' | 'auto_merge';

const AUTONOMY_WIRE_TAGS: Record<string, AutonomyWireTag> = {
  L1: 'report_only',
  L2: 'draft_pr',
  L3: 'verified_pr',
  L4: 'auto_merge'
};

/** Map an `AUTONOMY_OPTIONS` UI value (`'L1'..'L4'`, or the `AUTO_AUTONOMY`
 *  sentinel) to the real `CreateTaskOptions.autonomy_level` wire tag.
 *  Returns `undefined` for `AUTO_AUTONOMY`, `undefined`/empty input, or
 *  anything else not a recognized `L1..L4` value — the caller must omit the
 *  field entirely in every one of those cases (never send a garbage string
 *  the server would 422 on), which is exactly the "inherit the repo's
 *  `.lopi/loop.toml`" case a card/pane that never touched autonomy resolves
 *  to. */
export function autonomyToWire(level: string | undefined): AutonomyWireTag | undefined {
  return level ? AUTONOMY_WIRE_TAGS[level] : undefined;
}

/** How much the `claude -p` worker session may act on tool calls without a
 *  human answering a prompt, passed to the CLI as `--permission-mode`.
 *  Mirrors `crates/lopi-core/src/permission_mode.rs::PermissionMode` — the
 *  wire value is the CLI's own literal string, unlike `autonomy`'s `L1..L4`
 *  UI value (mapped via `autonomyToWire`). Wired end to end: it reaches
 *  a real `CreateTaskRequest.permission_mode`. Only the four modes proven
 *  headless-safe by Permission-Modes-1's kill-tests are selectable — the
 *  CLI's own `plan`/`manual` need a live human relay every headless `-p` run
 *  has no channel for, so they're deliberately absent here. */
export const PERMISSION_MODE_OPTIONS: Option[] = [
  { value: 'bypassPermissions', label: 'Bypass', hint: 'no prompts, full autonomy (current default)' },
  { value: 'auto', label: 'Auto', hint: 'model reviews each action, blocks anything risky' },
  { value: 'acceptEdits', label: 'Accept edits', hint: 'file edits auto-approved, everything else needs an allow-list entry' },
  { value: 'dontAsk', label: 'Locked', hint: 'only pre-approved commands run, everything else denied' }
];

/** The `PERMISSION_MODE_OPTIONS` value reproducing lopi's pre-existing
 *  unconditional `--dangerously-skip-permissions` behavior — the wire
 *  default an absent `CreateTaskRequest.permission_mode` resolves to
 *  server-side. Never sent explicitly on the wire when a field resolves to
 *  this value untouched (see `cardToTaskPayload`/`paneSubmitPayload`). */
export const DEFAULT_PERMISSION_MODE = 'bypassPermissions';

/** The branch a fresh stack starts on, before any repo has been picked. The
 *  live dropdowns no longer read this — they derive their options from
 *  `stores/branches.ts`, which fetches the selected repo's real branches from
 *  `/api/branches`. This is only the cold-start seed for
 *  `DEFAULT_STACK_DEFAULTS`, which lives in the tsx-testable pure layer and so
 *  cannot reach the network. */
export const SEED_BRANCH = 'main';

/** Pick the branch to display for a repo, given that repo's real branches.
 *
 *  An empty `branches` means we have no knowledge of the repo — unfetched, or
 *  the fetch failed — so the caller's current value is returned untouched
 *  rather than being second-guessed away. Otherwise an explicit, still-valid
 *  choice always wins; only an unset or now-invalid branch falls back to the
 *  repo's HEAD. `branch` is not inert: it reaches the server as a planning
 *  constraint via `paneSubmitPayload`, so showing one branch while storing
 *  another would silently launch against the wrong target. */
export function resolveBranch(current: string, branches: string[], head: string): string {
  if (!branches.length) return current;
  if (current && branches.includes(current)) return current;
  return head && branches.includes(head) ? head : branches[0];
}

/** The app-wide `DEF` a stack's own defaults start from and are compared
 *  against (`stackDefaultsActive`) to decide whether the dock's "default"
 *  summary line has anything non-baseline to report. */
export const DEFAULT_STACK_DEFAULTS: StackDefaults = {
  model: MODEL_OPTIONS[0].value,
  effort: 'medium',
  repo: '',
  branch: SEED_BRANCH,
  autonomy: AUTO_AUTONOMY,
  permission_mode: DEFAULT_PERMISSION_MODE
};

/** Fresh defaults for a newly-created stack — every pane gets its own
 *  object (never a shared reference), matching `defaultGuardrails()`'s
 *  per-card convention in `stores/stack.ts`. */
export function defaultStackDefaults(): StackDefaults {
  return { ...DEFAULT_STACK_DEFAULTS };
}
