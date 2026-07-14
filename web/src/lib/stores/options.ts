/**
 * Pure, static option catalogs — split out of `controls.ts` (Stack-1) so
 * modules that must stay tsx-testable (no `$app/environment`, which only
 * resolves inside a Vite build) can depend on just the data, not
 * `controls.ts`'s browser-coupled `launchControls` localStorage
 * persistence. `controls.ts` re-exports everything here verbatim for every
 * pre-existing call site — nothing changes for them. Reach for this module
 * directly only where avoiding the `$app/environment` import chain matters,
 * e.g. `stores/stackDefaults.ts` (imported by `stores/stack.ts`, which
 * `stack.test.ts` runs under plain `tsx`, same reasoning as
 * `stackRun.ts`'s own doc comment on why it takes `statusSource` as a
 * parameter instead of importing `./agents` directly).
 */

/** A selectable option with a stable value and a human label. */
export interface Option {
  value: string;
  label: string;
  hint?: string;
  /** Section this option belongs to, or absent to pin it above every section.
   *  Only the repo catalog groups; every other field leaves this unset and so
   *  renders as one flat list — see `stores/optionMenu.ts`. */
  group?: string;
}

/** Claude models lopi can drive, newest first. */
export const MODEL_OPTIONS: Option[] = [
  { value: 'claude-opus-4-8', label: 'Opus 4.8', hint: 'deepest reasoning' },
  { value: 'claude-sonnet-4-6', label: 'Sonnet 4.6', hint: 'balanced' },
  { value: 'claude-haiku-4-5', label: 'Haiku 4.5', hint: 'fastest' }
];

/** Reasoning-effort presets. */
export const EFFORT_OPTIONS: Option[] = [
  { value: 'low', label: 'Low', hint: 'quick passes' },
  { value: 'medium', label: 'Medium', hint: 'default' },
  { value: 'high', label: 'High', hint: 'thorough' },
  { value: 'max', label: 'Max', hint: 'exhaustive' }
];

/** Scheduling priority presets. */
export const PRIORITY_OPTIONS: Option[] = [
  { value: 'low', label: 'Low' },
  { value: 'normal', label: 'Normal' },
  { value: 'high', label: 'High' },
  { value: 'critical', label: 'Critical' }
];

/** Resolve a value to its display label within an option set. */
export function labelFor(options: Option[], value: string): string {
  return options.find((o) => o.value === value)?.label ?? value;
}
