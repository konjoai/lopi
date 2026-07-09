/**
 * Stack-level defaults — per-stack (per-pane) baseline for every card's
 * config-drawer override: model, effort, repo, branch, autonomy. Owned by
 * each `StackPaneState.config.defaults` (`stores/stack.ts`), not a single
 * global store — Stack-1 made this per-pane so two panes can carry two
 * different default configs (was a single app-wide `writable` through
 * UI-2/Backend-1). `model`/`effort`/`repo` are real `CreateTaskRequest`
 * fields; `branch`/`autonomy` are client-only — see UI_PLAN.md's Backend
 * Bindings table.
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

/** Placeholder branch list — there is no `/api/branches` endpoint yet, so
 *  this is a static, client-only convenience rather than a repo-derived
 *  list (same honesty caveat as `autonomy`). */
export const BRANCH_OPTIONS: Option[] = [
  { value: 'main', label: 'main' },
  { value: 'dev', label: 'dev' }
];

/** The app-wide `DEF` a stack's own defaults start from and are compared
 *  against (`stackDefaultsActive`) to decide whether the dock's "default"
 *  summary line has anything non-baseline to report. */
export const DEFAULT_STACK_DEFAULTS: StackDefaults = {
  model: MODEL_OPTIONS[0].value,
  effort: 'medium',
  repo: '',
  branch: BRANCH_OPTIONS[0].value,
  autonomy: 'L2'
};

/** Fresh defaults for a newly-created stack — every pane gets its own
 *  object (never a shared reference), matching `defaultGuardrails()`'s
 *  per-card convention in `stores/stack.ts`. */
export function defaultStackDefaults(): StackDefaults {
  return { ...DEFAULT_STACK_DEFAULTS };
}
