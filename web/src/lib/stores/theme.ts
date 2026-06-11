/**
 * Theme store — Konjo accent variants, persisted in localStorage.
 *
 * Mirrors the OpenClaw Control UI's theme picker: browser-local only,
 * applied as a `data-theme` attribute on <html> which the CSS variable
 * overrides in app.css pick up.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';

export type Theme = 'ice' | 'ember' | 'jade';

export const THEMES: { id: Theme; label: string; swatch: string }[] = [
  { id: 'ice', label: 'Ice', swatch: '#00d4ff' },
  { id: 'ember', label: 'Ember', swatch: '#ff9500' },
  { id: 'jade', label: 'Jade', swatch: '#00ff9d' }
];

const STORAGE_KEY = 'lopi-theme';

function load(): Theme {
  if (!browser) return 'ice';
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored === 'ember' || stored === 'jade' ? stored : 'ice';
}

export const theme = writable<Theme>(load());

/** Set + persist + apply the theme. */
export function setTheme(t: Theme) {
  theme.set(t);
  if (!browser) return;
  localStorage.setItem(STORAGE_KEY, t);
  applyTheme(t);
}

/** Apply the current theme to <html> — call once on app mount. */
export function applyTheme(t?: Theme) {
  if (!browser) return;
  const value = t ?? load();
  if (value === 'ice') document.documentElement.removeAttribute('data-theme');
  else document.documentElement.setAttribute('data-theme', value);
}
