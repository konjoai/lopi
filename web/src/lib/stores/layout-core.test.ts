/**
 * Pure layout-core tests — run with `npx tsx src/lib/stores/layout-core.test.ts`.
 * No browser, no Svelte: just the tiling/placement/reconcile algorithms.
 */
import {
  tileDims,
  clampPaneCount,
  resizeSlots,
  placeSession,
  swapSlots,
  reconcile,
  type Slot,
  MIN_PANES,
  MAX_PANES
} from './layout-core';
import { eq, namedSummary } from '$lib/test-harness';

// ── tileDims — the halves/thirds/quarters contract ───────────────────────────
eq(tileDims(1), [1, 1], 'tileDims 1 → full');
eq(tileDims(2), [2, 1], 'tileDims 2 → halves');
eq(tileDims(3), [3, 1], 'tileDims 3 → thirds');
eq(tileDims(4), [2, 2], 'tileDims 4 → quarters');
eq(tileDims(6), [3, 2], 'tileDims 6 → 3×2');
eq(tileDims(9), [3, 3], 'tileDims 9 → 3×3');
eq(tileDims(12), [4, 3], 'tileDims 12 → 4×3');

// ── clampPaneCount ───────────────────────────────────────────────────────────
eq(clampPaneCount(0), MIN_PANES, 'clamp below floor');
eq(clampPaneCount(99), MAX_PANES, 'clamp above ceiling');
eq(clampPaneCount(4), 4, 'clamp in range');

// ── resizeSlots — grow keeps order, shrink truncates ─────────────────────────
eq(resizeSlots(['a', 'b'], 4), ['a', 'b', null, null], 'grow appends nulls');
eq(resizeSlots(['a', 'b', 'c', 'd'], 2), ['a', 'b'], 'shrink truncates');
eq(resizeSlots(['a'], 1), ['a'], 'no-op when equal');

// ── placeSession — first empty, then append, no duplicates ───────────────────
eq(placeSession([null, 'b'], 'a'), ['a', 'b'], 'fills first empty');
eq(placeSession(['a', 'b'], 'a'), ['a', 'b'], 'never duplicates');
eq(placeSession(['a', 'b'], 'c'), ['a', 'b', 'c'], 'appends when full but under cap');

// ── swapSlots ────────────────────────────────────────────────────────────────
eq(swapSlots(['a', 'b', 'c'], 0, 2), ['c', 'b', 'a'], 'swaps two slots');
eq(swapSlots(['a', 'b'], 0, 0), ['a', 'b'], 'self-swap is a no-op');
eq(swapSlots(['a', 'b'], 0, 9), ['a', 'b'], 'out-of-range is a no-op');

// ── reconcile — the resurrection-prevention core ─────────────────────────────
const empty: ReadonlySet<string> = new Set();

// A brand-new session auto-places into a free slot and becomes known.
{
  const r = reconcile([null, null], ['x'], empty, empty, empty);
  eq(r.slots, ['x', null], 'fresh session auto-placed');
  eq(r.placed, ['x'], 'fresh session reported as placed');
  eq(r.newlyKnown, ['x'], 'fresh session marked newly-known');
}

// A KNOWN session returning via snapshot must NOT reopen.
{
  const r = reconcile([null, null], ['x'], new Set(['x']), empty, empty);
  eq(r.slots, [null, null], 'known session does not reopen');
  eq(r.placed, [], 'known session not placed');
}

// A CLOSED (parked) fresh session is marked known but stays out of the grid.
{
  const r = reconcile([null, null], ['x'], empty, new Set(['x']), empty);
  eq(r.slots, [null, null], 'closed session stays parked');
  eq(r.placed, [], 'closed session not placed');
  eq(r.newlyKnown, ['x'], 'closed session still recorded as known');
}

// A DELETED (tombstoned) session is ignored entirely — the bug fix.
{
  const r = reconcile([null, null], ['x'], empty, empty, new Set(['x']));
  eq(r.slots, [null, null], 'deleted session never re-hydrates');
  eq(r.newlyKnown, [], 'deleted session not recorded');
}

// Full grid leaves the overflow session in the sidebar.
{
  const full: Slot[] = ['a', 'b'];
  const r = reconcile(full, ['c'], empty, empty, empty);
  eq(r.slots, ['a', 'b'], 'full grid does not displace existing panes');
  eq(r.placed, [], 'overflow session not placed');
  eq(r.newlyKnown, ['c'], 'overflow session still becomes known');
}

namedSummary('layout-core');
