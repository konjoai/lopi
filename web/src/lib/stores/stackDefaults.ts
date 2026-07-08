/**
 * Stack-level defaults for the /stacks composer's selector row and every
 * card's config-drawer baseline — model, effort, repo, branch, autonomy.
 * Distinct from `launchControls` (which drives single-task launches
 * elsewhere): these are in-memory only this slice. `model`/`effort`/`repo`
 * are real `CreateTaskRequest` fields; `branch`/`autonomy` are client-only —
 * see UI_PLAN.md's Backend Bindings table.
 */
import { writable } from 'svelte/store';
import { MODEL_OPTIONS, type Option } from '$lib/stores/controls';

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

const DEFAULTS: StackDefaults = {
  model: MODEL_OPTIONS[0].value,
  effort: 'medium',
  repo: '',
  branch: BRANCH_OPTIONS[0].value,
  autonomy: 'L2'
};

export const stackDefaults = writable<StackDefaults>({ ...DEFAULTS });
