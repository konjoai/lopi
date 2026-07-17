/**
 * Shared drag state for within-pane card reordering. A single module-scope
 * store (rather than component-local state) is what lets a drop target know
 * which card and index is being dragged without threading callbacks through
 * every `StackCard` instance. Cross-pane drag is out of scope this slice —
 * every consumer checks `paneKey` matches before acting.
 */
import { writable } from 'svelte/store';

export interface DragState {
  paneKey: string;
  cardId: string;
  index: number;
}

export const dragging = writable<DragState | null>(null);

/** Stack-1: the pane-level twin of `dragging` — whole-stack reordering via
 *  the purple control dock's drag handle. The draggable element is the
 *  pane's own root container (`StackPane.svelte`), one component up from
 *  the dock the handle lives in, so this is module-scope state rather than
 *  a prop threaded back up through a callback. */
export interface PaneDragState {
  paneKey: string;
  index: number;
}

export const draggingPane = writable<PaneDragState | null>(null);

/** Which pane's whole-stack drag handle is currently held down (`null` when
 *  none) — the pane-level twin of `StackCard.svelte`'s local `armDrag`/
 *  `disarmDrag` boolean. Has to be module-scope rather than local component
 *  state, unlike the card version: the handle lives in
 *  `StackControlDock.svelte`, but the element it needs to arm — the pane's
 *  own root — is one component up, in `StackPane.svelte`. Before this, the
 *  handle button itself carried a static `draggable="true"`, so only that
 *  ~14px icon was ever the actual drag source; real drags routinely lost
 *  the cursor off something that small and snapped back on drop. */
export const armedPaneKey = writable<string | null>(null);
