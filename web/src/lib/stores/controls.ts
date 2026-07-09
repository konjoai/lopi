/**
 * Launch controls — the model / effort / repo / branch / priority selections
 * that drive new task submissions. Persisted to localStorage so the cockpit
 * remembers the operator's last setup across reloads.
 *
 * The option catalogs (`Option`/`MODEL_OPTIONS`/`EFFORT_OPTIONS`/
 * `PRIORITY_OPTIONS`/`labelFor`) live in `./options` and are re-exported
 * here verbatim (Stack-1) — this file's own `$app/environment` import
 * (needed for the `browser` check below) would otherwise drag every pure
 * consumer of those catalogs into a SvelteKit-virtual-module dependency it
 * doesn't need, breaking anything that must stay tsx-testable outside a
 * Vite build (see `stores/stackDefaults.ts`, imported by `stores/stack.ts`).
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';
import { type Option, MODEL_OPTIONS, EFFORT_OPTIONS, PRIORITY_OPTIONS, labelFor } from './options';

export { type Option, MODEL_OPTIONS, EFFORT_OPTIONS, PRIORITY_OPTIONS, labelFor };

/** Mutable launch configuration shared by every empty pane. */
export interface LaunchControls {
  model: string;
  effort: string;
  priority: string;
  repo: string;
  branch: string;
}

const KEY = 'lopi-launch-controls-v1';

const DEFAULTS: LaunchControls = {
  model: MODEL_OPTIONS[0].value,
  effort: 'medium',
  priority: 'normal',
  repo: '',
  branch: ''
};

function load(): LaunchControls {
  if (!browser) return { ...DEFAULTS };
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULTS };
    const parsed = JSON.parse(raw);
    return { ...DEFAULTS, ...parsed };
  } catch {
    return { ...DEFAULTS };
  }
}

export const launchControls = writable<LaunchControls>(load());

if (browser) {
  launchControls.subscribe((v) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(v));
    } catch (err) {
      console.warn('[lopi] launch-controls persist failed:', err);
    }
  });
}
