/**
 * Pure session filtering + grouping for the sessions sidebar. No Svelte, no
 * stores — just `AgentState[] → buckets`, so it is unit-tested directly
 * (`session-groups.test.ts`) and the component stays a thin renderer.
 */
import type { AgentState, Status } from './agents-model';

/** Lifecycle bucket a session belongs to in the sidebar. */
export type GroupKey = 'active' | 'done' | 'failed';

export interface SessionGroup {
  key: GroupKey;
  label: string;
  sessions: AgentState[];
}

const GROUP_ORDER: { key: GroupKey; label: string }[] = [
  { key: 'active', label: 'active' },
  { key: 'done', label: 'done' },
  { key: 'failed', label: 'failed' }
];

/** Which bucket a status maps to. Running/queued are live work; cancelled is
 *  filed with failures (it didn't complete); completed is done. */
export function groupKeyFor(status: Status): GroupKey {
  if (status === 'running' || status === 'queued') return 'active';
  if (status === 'failed' || status === 'cancelled') return 'failed';
  return 'done';
}

/** Case-insensitive substring match across goal, repo and branch. An empty or
 *  whitespace query matches everything. */
export function filterSessions(sessions: Iterable<AgentState>, query: string): AgentState[] {
  const q = query.trim().toLowerCase();
  const all = [...sessions];
  if (!q) return all;
  return all.filter(
    (s) =>
      s.goal.toLowerCase().includes(q) ||
      s.repo.toLowerCase().includes(q) ||
      s.branch.toLowerCase().includes(q)
  );
}

/**
 * Sort newest-first, split into active/done/failed, and drop empty buckets.
 * The order is stable: active, then done, then failed.
 */
export function groupSessions(sessions: Iterable<AgentState>): SessionGroup[] {
  const sorted = [...sessions].sort((a, b) => b.startedAt - a.startedAt);
  return GROUP_ORDER.map(({ key, label }) => ({
    key,
    label,
    sessions: sorted.filter((s) => groupKeyFor(s.status) === key)
  })).filter((g) => g.sessions.length > 0);
}
