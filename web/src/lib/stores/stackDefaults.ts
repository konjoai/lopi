/**
 * Stack-level defaults for the /stacks composer's selector row — model,
 * effort, repo, and autonomy. Distinct from `launchControls` (which drives
 * single-task launches elsewhere): these are in-memory only this slice, not
 * persisted, and not yet wired to any backend field (`CreateTaskRequest`
 * doesn't expose autonomy yet — see UI_PLAN.md's Backend Bindings table).
 */
import { writable } from 'svelte/store';
import { MODEL_OPTIONS, type Option } from '$lib/stores/controls';

export interface StackDefaults {
  model: string;
  effort: string;
  repo: string;
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

const DEFAULTS: StackDefaults = {
  model: MODEL_OPTIONS[0].value,
  effort: 'medium',
  repo: '',
  autonomy: 'L2'
};

export const stackDefaults = writable<StackDefaults>({ ...DEFAULTS });
