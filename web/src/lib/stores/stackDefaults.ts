/**
 * Stack-level defaults — per-stack (per-pane) baseline for every card's
 * config-drawer override: model, effort, repo, branch, autonomy. Owned by
 * each `StackPaneState.config.defaults` (`stores/stack.ts`), not a single
 * global store — Stack-1 made this per-pane so two panes can carry two
 * different default configs (was a single app-wide `writable` through
 * UI-2/Backend-1). `model`/`effort`/`repo` are real `CreateTaskRequest`
 * fields; `autonomy` is client-only — see UI_PLAN.md's Backend Bindings table.
 * `branch` is not inert despite having no `CreateTaskRequest` field of its own:
 * `paneSubmitPayload` turns it into a "Target branch: …" planning constraint.
 */
import { MODEL_OPTIONS, type Option } from '$lib/stores/options';

export interface StackDefaults {
  model: string;
  effort: string;
  repo: string;
  branch: string;
  autonomy: string;
}

/** The real `AutonomyLevel` ladder (`crates/lopi-core/src/loop_config.rs`) —
 *  PR-flow semantics, not the mockup's mismatched "leash" copy (see
 *  UI_PLAN.md's flagged label mismatch). Mirrors `loop/+page.svelte`'s
 *  `ladderHint()` wording so the two surfaces read the same. */
export const AUTONOMY_OPTIONS: Option[] = [
  { value: 'L1', label: 'L1 · Report only', hint: 'report only, no PR' },
  { value: 'L2', label: 'L2 · Draft PR', hint: 'draft PR, human approves' },
  { value: 'L3', label: 'L3 · Verified PR', hint: 'verify before PR' },
  { value: 'L4', label: 'L4 · Auto-merge', hint: 'auto-merge on pass' }
];

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
  autonomy: 'L2'
};

/** Fresh defaults for a newly-created stack — every pane gets its own
 *  object (never a shared reference), matching `defaultGuardrails()`'s
 *  per-card convention in `stores/stack.ts`. */
export function defaultStackDefaults(): StackDefaults {
  return { ...DEFAULT_STACK_DEFAULTS };
}
