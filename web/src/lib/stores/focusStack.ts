/**
 * focusStack — the one-shot "scroll me into view" signal from `/overview`'s
 * board to `/stacks`'s `TileGrid`. Every pane already renders side-by-side
 * on `/stacks` (there's no per-stack detail route to push onto), so tapping
 * a stack card can't navigate *to* it the way the iOS handoff describes —
 * this is the web equivalent: set the target pane's key, `StackPane.svelte`
 * scrolls itself into view and flashes, then clears the key back to `null`.
 */
import { writable } from 'svelte/store';

export const focusedStackKey = writable<string | null>(null);

/** Ask `/stacks` to scroll to and flash the pane with this key. */
export function focusStack(key: string): void {
  focusedStackKey.set(key);
}
