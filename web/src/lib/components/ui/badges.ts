/**
 * Shared color helpers for status + log-level chips across the data tabs.
 * Returns CSS color values from the Konjo palette.
 */

/** Task status string (serde TaskStatus rendering) → chip color. */
export function statusColor(status: string): string {
  const s = status.toLowerCase();
  if (s.includes('success') || s.includes('succeeded')) return 'var(--konjo-jade)';
  if (s.includes('failed') || s.includes('rolledback') || s.includes('dead'))
    return 'var(--konjo-rose)';
  if (s.includes('queued')) return 'var(--konjo-sun)';
  if (s.includes('retry')) return 'var(--konjo-flame)';
  return 'var(--konjo-ice)';
}

/** Log level → color. */
export function levelColor(level: string): string {
  switch (level) {
    case 'error':
      return 'var(--konjo-rose)';
    case 'warn':
      return 'var(--konjo-flame)';
    case 'debug':
      return 'rgba(245,245,245,0.4)';
    default:
      return 'var(--konjo-ice)';
  }
}

/** Compact status label from a serde TaskStatus JSON value (string or object). */
export function statusLabel(status: unknown): string {
  if (typeof status === 'string') {
    // May be pre-rendered JSON like {"Failed":{...}} from the history table.
    if (status.startsWith('{')) {
      try {
        return statusLabel(JSON.parse(status));
      } catch {
        return status;
      }
    }
    return status;
  }
  if (status && typeof status === 'object') {
    const key = Object.keys(status)[0];
    return key ?? 'Unknown';
  }
  return 'Unknown';
}
