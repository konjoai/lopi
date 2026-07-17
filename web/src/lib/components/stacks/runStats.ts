/**
 * Pure formatters for `RunStatsPill.svelte` — split out so they're testable
 * with `tsx` (no Svelte compiler needed), matching this repo's convention of
 * keeping component logic in a plain `.ts` module behind a thin wrapper.
 */

/** "4m 27s" / "1h 03m" / "42s". */
export function formatElapsed(ms: number): string {
  const totalSec = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`;
  if (m > 0) return `${m}m ${String(s).padStart(2, '0')}s`;
  return `${s}s`;
}

/** "3.4k" / "1.2m" / "840" — compact token count. */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}m`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return `${Math.max(0, Math.round(n))}`;
}

/** "$0.42" / "<$0.01" / "$0.00" — matches the run-complete cost line's
 *  precision without the noise of 4 decimal places inline on a card. */
export function formatCost(usd: number): string {
  if (usd <= 0) return '$0.00';
  if (usd < 0.01) return '<$0.01';
  return `$${usd.toFixed(2)}`;
}
