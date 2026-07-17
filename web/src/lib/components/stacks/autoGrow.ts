/**
 * Grows a `<textarea>` to fit its content (no inner scrollbar, no sideways
 * clipping) — called on mount, so a field that starts with existing text
 * renders at full height immediately, and on every keystroke thereafter.
 * Shared by every free-text field in the stacks UI (the composer, the
 * command bar, the committed-card edit box, and the guardrails/schedule
 * popover inputs) so a long value always wraps and stays visible instead of
 * scrolling off sideways in a single line.
 */
export function autoGrow(node: HTMLTextAreaElement) {
  const resize = () => {
    node.style.height = 'auto';
    node.style.height = `${node.scrollHeight}px`;
  };
  resize();
  node.addEventListener('input', resize);
  return {
    destroy() {
      node.removeEventListener('input', resize);
    }
  };
}
