/**
 * Launch controls — the model / effort / repo / branch / priority selections
 * that drive new task submissions. Persisted to localStorage so the cockpit
 * remembers the operator's last setup across reloads.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';

/** A selectable option with a stable value and a human label. */
export interface Option {
  value: string;
  label: string;
  hint?: string;
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

/** Resolve a value to its display label within an option set. */
export function labelFor(options: Option[], value: string): string {
  return options.find((o) => o.value === value)?.label ?? value;
}
