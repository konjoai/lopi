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
