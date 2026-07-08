/**
 * Pure session-groups tests — `npx tsx src/lib/stores/session-groups.test.ts`.
 */
import { groupKeyFor, filterSessions, groupSessions } from './session-groups';
import { makeBlank } from './agentReducer';
import type { AgentState } from './agents';
import { eq, namedSummary } from '$lib/test-harness';

function mk(id: string, over: Partial<AgentState>): AgentState {
  return { ...makeBlank(id), ...over };
}

// ── groupKeyFor ──────────────────────────────────────────────────────────────
eq(groupKeyFor('running'), 'active', 'running → active');
eq(groupKeyFor('queued'), 'active', 'queued → active');
eq(groupKeyFor('completed'), 'done', 'completed → done');
eq(groupKeyFor('failed'), 'failed', 'failed → failed');
eq(groupKeyFor('cancelled'), 'failed', 'cancelled → failed');

// ── filterSessions ───────────────────────────────────────────────────────────
const fixtures: AgentState[] = [
  mk('a', { goal: 'Wire OTel export', repo: '~/kairu', branch: 'feat/otel', status: 'running' }),
  mk('b', { goal: 'Refactor encoder', repo: '~/vectro', branch: 'perf/neon', status: 'queued' }),
  mk('c', { goal: 'Add Redis cache', repo: '~/rag', branch: 'feat/redis', status: 'completed' }),
  mk('d', { goal: 'Migrate cache', repo: '~/rag', branch: 'feat/streams', status: 'failed' })
];

eq(filterSessions(fixtures, '').length, 4, 'empty query matches all');
eq(filterSessions(fixtures, '   ').length, 4, 'whitespace query matches all');
eq(
  filterSessions(fixtures, 'redis').map((s) => s.id),
  ['c'],
  'matches on goal'
);
eq(
  filterSessions(fixtures, '~/rag').map((s) => s.id),
  ['c', 'd'],
  'matches on repo'
);
eq(
  filterSessions(fixtures, 'OTEL').map((s) => s.id),
  ['a'],
  'case-insensitive on branch'
);
eq(filterSessions(fixtures, 'nomatch').length, 0, 'no match → empty');

// ── groupSessions ────────────────────────────────────────────────────────────
{
  const groups = groupSessions(fixtures);
  eq(
    groups.map((g) => g.key),
    ['active', 'done', 'failed'],
    'ordered active, done, failed; empties dropped'
  );
  eq(
    groups[0].sessions.map((s) => s.id),
    ['a', 'b'],
    'active bucket holds running + queued'
  );
  eq(groups[2].sessions.map((s) => s.id), ['d'], 'failed bucket');
}
{
  // newest-first within a bucket
  const two = [mk('old', { status: 'running', startedAt: 100 }), mk('new', { status: 'running', startedAt: 900 })];
  eq(groupSessions(two)[0].sessions.map((s) => s.id), ['new', 'old'], 'newest first within bucket');
}
{
  // empty input → no groups
  eq(groupSessions([] as AgentState[]).length, 0, 'no sessions → no groups');
}

namedSummary('session-groups');
