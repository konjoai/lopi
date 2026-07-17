/**
 * Global keyboard shortcut store.
 *
 * Bindings:
 *   j / ↓        — next agent
 *   k / ↑        — previous agent
 *   Esc          — clear active agent (return to overview)
 *   ⌘K / Ctrl+K  — toggle Loop Stacks ↔ Overview
 *   ?            — show shortcut help overlay
 *   /            — focus task submit (when implemented)
 *
 * Avoids hijacking when user is typing in an input/textarea.
 */
import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import { writable, get } from 'svelte/store';
import { agents, activeAgentId, selectAgent } from './agents';
import { page } from '$app/stores';

export const helpVisible = writable(false);

function isTextInput(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === 'INPUT' ||
    tag === 'TEXTAREA' ||
    tag === 'SELECT' ||
    target.isContentEditable
  );
}

function cycleAgent(direction: 1 | -1) {
  const map = get(agents);
  if (map.size === 0) return;
  const ids = [...map.keys()];
  const current = get(activeAgentId);
  const idx = current ? ids.indexOf(current) : -1;
  const next = (idx + direction + ids.length) % ids.length;
  selectAgent(ids[next]);
}

function toggleView() {
  const current = get(page).url.pathname;
  // The whole-fleet glance now lives on /overview; the working surface is the
  // Loop Stacks. ⌘K flips between them (Constellation was cut in Unify-2).
  goto(current.startsWith('/overview') ? '/stacks' : '/overview');
}

function handler(e: KeyboardEvent) {
  if (isTextInput(e.target)) return;

  // ⌘K or Ctrl+K — toggle view
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    toggleView();
    return;
  }

  // Plain keys
  switch (e.key) {
    case 'j':
    case 'ArrowDown':
      e.preventDefault();
      cycleAgent(1);
      return;
    case 'k':
    case 'ArrowUp':
      e.preventDefault();
      cycleAgent(-1);
      return;
    case 'Escape':
      activeAgentId.set(null);
      helpVisible.set(false);
      return;
    case '?':
      helpVisible.update((v) => !v);
      return;
  }
}

let installed = false;
export function installKeyboardShortcuts() {
  if (!browser || installed) return;
  installed = true;
  window.addEventListener('keydown', handler);
}
