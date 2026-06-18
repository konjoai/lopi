/**
 * Pane/session layout store — the source of truth for *which* sessions are
 * mounted as panes, independent of which sessions exist.
 *
 * This is the fix for the "deleted sessions reappear" bug. The old code
 * conflated two distinct ideas:
 *
 *   • a **session** — a task the server knows about (lives in the `agents`
 *     store, hydrated from the WebSocket snapshot on every load), and
 *   • a **pane** — a slot in the Forge grid that happens to show a session.
 *
 * Closing a pane must NOT delete the session, and deleting a session must
 * survive a reload. We model three persisted sets, keyed in localStorage:
 *
 *   - `slots`    — the ordered grid (session id or `null` per slot).
 *   - `closed`   — sessions dismissed from the grid but still listed in the
 *                  sidebar; they never auto-reopen into a pane.
 *   - `deleted`  — tombstones: permanently removed sessions. The snapshot
 *                  reducer filters these out, so a slow/failed server DELETE
 *                  can no longer resurrect them on the next connect.
 *   - `known`    — every session id ever seen, so a *genuinely new* task can
 *                  be told apart from an old one returning via the snapshot
 *                  and auto-placed into a free pane.
 */
import { writable, derived, get, type Readable } from 'svelte/store';
import { browser } from '$app/environment';
import {
  DEFAULT_PANE_COUNT,
  type Slot,
  clampPaneCount,
  placeSession,
  reconcile,
  resizeSlots,
  swapSlots
} from './layout-core';

export { DEFAULT_PANE_COUNT, MAX_PANES, MIN_PANES } from './layout-core';

const SLOTS_KEY = 'lopi-pane-slots-v1';
const CLOSED_KEY = 'lopi-closed-sessions-v1';
const DELETED_KEY = 'lopi-deleted-sessions-v1';
const KNOWN_KEY = 'lopi-known-sessions-v1';

// ── localStorage helpers (no-ops during SSR) ─────────────────────────────────
function loadArray(key: string): (string | null)[] | null {
  if (!browser) return null;
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function loadSet(key: string): Set<string> {
  if (!browser) return new Set();
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? new Set(parsed.filter((v) => typeof v === 'string')) : new Set();
  } catch {
    return new Set();
  }
}

function persist(key: string, value: unknown) {
  if (!browser) return;
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch (err) {
    console.warn('[lopi] layout persist failed:', err);
  }
}

// ── Stores ────────────────────────────────────────────────────────────────────
const initialSlots: Slot[] =
  loadArray(SLOTS_KEY) ?? Array.from({ length: DEFAULT_PANE_COUNT }, () => null);

/** Ordered grid of pane slots; each holds a session id or `null` (empty). */
export const paneSlots = writable<Slot[]>(initialSlots);
/** Sessions dismissed from the grid but kept in the sidebar. */
export const closedSessions = writable<Set<string>>(loadSet(CLOSED_KEY));
/** Tombstoned sessions — permanently deleted, filtered from every snapshot. */
export const deletedSessions = writable<Set<string>>(loadSet(DELETED_KEY));
/** Every session id ever observed — used to detect genuinely new tasks. */
export const knownSessions = writable<Set<string>>(loadSet(KNOWN_KEY));

/** Number of mounted panes. */
export const paneCount: Readable<number> = derived(paneSlots, ($slots) => $slots.length);

// Mirror every mutation back to localStorage so a reload restores the layout.
if (browser) {
  paneSlots.subscribe((v) => persist(SLOTS_KEY, v));
  closedSessions.subscribe((v) => persist(CLOSED_KEY, [...v]));
  deletedSessions.subscribe((v) => persist(DELETED_KEY, [...v]));
  knownSessions.subscribe((v) => persist(KNOWN_KEY, [...v]));
}

// ── Tombstone query (consumed by the agents reducer) ─────────────────────────
/** True when `id` was permanently deleted and must never be re-hydrated. */
export function isDeleted(id: string): boolean {
  return get(deletedSessions).has(id);
}

function unpark(id: string): void {
  closedSessions.update((s) => {
    if (!s.has(id)) return s;
    const next = new Set(s);
    next.delete(id);
    return next;
  });
}

// ── Pane mutations ────────────────────────────────────────────────────────────
/** Place a session into the first empty slot, appending if the grid is full. */
export function openSession(id: string): void {
  unpark(id);
  paneSlots.update((slots) => placeSession(slots, id));
}

/** Mount a session into a *specific* pane slot — the drop target of a drag
 *  from the sessions sidebar. Unparks it, removes it from any slot it already
 *  occupied (so dragging never duplicates a pane), then drops it into `index`.
 *  Whatever sat in `index` is displaced back to the sidebar. */
export function mountInPane(id: string, index: number): void {
  unpark(id);
  paneSlots.update((slots) => {
    if (index < 0 || index >= slots.length) return slots;
    const next = slots.map((s) => (s === id ? null : s));
    next[index] = id;
    return next;
  });
}

/** Close the pane at `slot`: empties the slot and parks the session in the
 *  sidebar. The session itself is untouched (no server DELETE). */
export function closePane(slot: number): void {
  const slots = get(paneSlots);
  const id = slots[slot];
  if (id) closedSessions.update((s) => new Set(s).add(id));
  paneSlots.update((cur) => {
    const next = [...cur];
    if (slot >= 0 && slot < next.length) next[slot] = null;
    return next;
  });
}

/** Permanently delete a session: tombstone it, drop it from every pane and the
 *  sidebar. The tombstone is what stops the snapshot from resurrecting it. */
export function tombstoneSession(id: string): void {
  deletedSessions.update((s) => new Set(s).add(id));
  unpark(id);
  paneSlots.update((slots) => slots.map((s) => (s === id ? null : s)));
}

/** Grow or shrink the grid to exactly `n` panes (clamped to [MIN, MAX]). */
export function setPaneCount(n: number): void {
  paneSlots.update((slots) => resizeSlots(slots, clampPaneCount(n)));
}

/** Add one pane (up to MAX_PANES). */
export function addPane(): void {
  setPaneCount(get(paneSlots).length + 1);
}

/** Remove one pane (down to MIN_PANES); the displaced session stays in the
 *  sidebar. */
export function removePane(): void {
  setPaneCount(get(paneSlots).length - 1);
}

/** Swap the contents of two pane slots — drives drag-to-reorder. */
export function swapPanes(a: number, b: number): void {
  paneSlots.update((slots) => swapSlots(slots, a, b));
}

/** Reconcile the live session list against the grid: auto-place genuinely new
 *  sessions, leave known/closed ones exactly where the operator left them.
 *  Returns the ids that were auto-placed (for logging/tests). */
export function reconcileSessions(ids: Iterable<string>): string[] {
  const result = reconcile(
    get(paneSlots),
    ids,
    get(knownSessions),
    get(closedSessions),
    get(deletedSessions)
  );
  if (result.newlyKnown.length === 0) return [];
  paneSlots.set(result.slots);
  knownSessions.update((k) => {
    const out = new Set(k);
    for (const id of result.newlyKnown) out.add(id);
    return out;
  });
  return result.placed;
}
