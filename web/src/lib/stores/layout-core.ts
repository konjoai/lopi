/**
 * Pure layout algorithms — no Svelte, no browser, no `$app/environment`.
 *
 * Everything stateful about the pane grid lives in `layout.ts`; the
 * non-trivial *decisions* (how to tile N panes, where a fresh session lands,
 * how the grid grows/shrinks) live here so they can be unit-tested directly.
 */

/** Default number of panes — four concurrent agents, as a 2×2 grid. */
export const DEFAULT_PANE_COUNT = 4;
/** Hard ceiling on simultaneous panes. */
export const MAX_PANES = 12;
/** Floor — at least one pane is always mounted. */
export const MIN_PANES = 1;

/** Slot value: a session id, or `null` for an empty pane. */
export type Slot = string | null;

/**
 * Rows/cols for a given pane count. Favours wide splits at small counts so
 * 2 = halves, 3 = thirds, 4 = quarters — then grows three-wide.
 */
export function tileDims(n: number): [cols: number, rows: number] {
  if (n <= 1) return [1, 1];
  if (n === 2) return [2, 1];
  if (n === 3) return [3, 1];
  if (n === 4) return [2, 2];
  if (n <= 6) return [3, 2];
  if (n <= 9) return [3, 3];
  return [4, Math.ceil(n / 4)];
}

/** Clamp a requested pane count into the supported range. */
export function clampPaneCount(n: number): number {
  return Math.max(MIN_PANES, Math.min(MAX_PANES, n));
}

/** Grow or shrink a slot array to exactly `target` panes, preserving order. */
export function resizeSlots(slots: Slot[], target: number): Slot[] {
  const n = clampPaneCount(target);
  if (slots.length === n) return slots;
  if (slots.length < n) {
    return [...slots, ...Array.from({ length: n - slots.length }, () => null as Slot)];
  }
  return slots.slice(0, n);
}

/** Place a session into the first empty slot, append, or take the last slot
 *  when the grid is full. Never duplicates an already-mounted session. */
export function placeSession(slots: Slot[], id: string): Slot[] {
  if (slots.includes(id)) return slots;
  const empty = slots.indexOf(null);
  if (empty !== -1) {
    const next = [...slots];
    next[empty] = id;
    return next;
  }
  if (slots.length < MAX_PANES) return [...slots, id];
  const next = [...slots];
  next[next.length - 1] = id;
  return next;
}

/** Swap two slots; out-of-range or equal indices are no-ops. */
export function swapSlots(slots: Slot[], a: number, b: number): Slot[] {
  if (a === b || a < 0 || b < 0 || a >= slots.length || b >= slots.length) return slots;
  const next = [...slots];
  [next[a], next[b]] = [next[b], next[a]];
  return next;
}

/** Result of reconciling live sessions against the grid. */
export interface Reconciled {
  slots: Slot[];
  placed: string[];
  newlyKnown: string[];
}

/**
 * Decide which of `ids` are genuinely new and auto-place them into free panes.
 *
 * A session is *fresh* when it has never been seen (`known`) and was not
 * tombstoned (`deleted`). Fresh sessions that aren't parked (`closed`) drop
 * into the first empty slot. Everything already known keeps the operator's
 * layout untouched — the core guarantee behind "closed sessions don't pop
 * back open on reload".
 */
export function reconcile(
  slots: Slot[],
  ids: Iterable<string>,
  known: ReadonlySet<string>,
  closed: ReadonlySet<string>,
  deleted: ReadonlySet<string>
): Reconciled {
  const fresh: string[] = [];
  for (const id of ids) {
    if (known.has(id) || deleted.has(id)) continue;
    if (!fresh.includes(id)) fresh.push(id);
  }
  if (fresh.length === 0) return { slots, placed: [], newlyKnown: [] };

  let next = slots;
  const placed: string[] = [];
  for (const id of fresh) {
    if (closed.has(id) || next.includes(id)) continue;
    if (next.indexOf(null) === -1) break; // grid full — leave it in the sidebar
    next = placeSession(next, id);
    placed.push(id);
  }
  return { slots: next, placed, newlyKnown: fresh };
}
