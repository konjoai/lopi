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
