/**
 * toastStore — a minimal, generic undo-toast queue (round 2, item 1). No
 * existing shared toast component/store predates this; Loop Stacks' delete
 * flows are its first (and, this slice, only) callers, but nothing here is
 * stacks-specific — any future "instant action, recoverable for a few
 * seconds" flow can reuse it as-is.
 */
import { writable } from 'svelte/store';

/** An optional recovery action a toast offers — e.g. "Undo". */
export interface ToastAction {
  label: string;
  onClick: () => void;
}

/** One live toast. `id` is its removal key, generated internally. */
export interface ToastItem {
  id: string;
  message: string;
  action?: ToastAction;
}

export const toasts = writable<ToastItem[]>([]);

let counter = 0;

/** Queue a toast for `durationMs` (default ~5.5s, matching the "5-6s" delete
 *  window), then auto-dismiss. Clicking `action` (if any) runs it and
 *  dismisses immediately — after that point the underlying action (e.g. a
 *  delete) is final, exactly as if the toast had simply timed out. */
export function showToast(message: string, action?: ToastAction, durationMs = 5500): void {
  counter += 1;
  const id = `toast-${counter}`;
  toasts.update((list) => [...list, { id, message, action }]);
  setTimeout(() => dismissToast(id), durationMs);
}

/** Remove a toast by id — called on timeout, action click, or manual close.
 *  No-op if it's already gone (e.g. timeout firing after an action click
 *  already dismissed it). */
export function dismissToast(id: string): void {
  toasts.update((list) => list.filter((t) => t.id !== id));
}
