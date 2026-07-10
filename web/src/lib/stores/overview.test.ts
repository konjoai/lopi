/**
 * Overview rollup tests — `npx tsx src/lib/stores/overview.test.ts`.
 *
 * Phase 4 proof (Unify-2 §4): with the `agents` store seeded with N synthetic
 * agents across different phases, `overviewRows` produces one row per agent with
 * the correct cost/phase/elapsed and an orb-matching status color; filters and
 * counts partition the fleet (including the folded-in dead-letter view).
 */
import {
  overviewRows,
  filterRows,
  filterCounts,
  formatElapsed,
  type OverviewRow
} from './overview';
import { computeOrbState } from '$lib/forge/orbState';
import { makeBlank } from './agentReducer';
import type { AgentState } from './agents';
import { eq, ok, summary } from '$lib/test-harness';

const agent = (id: string, patch: Partial<AgentState>): AgentState => ({
  ...makeBlank(id),
  ...patch
});

// A realistic mixed fleet: two running (different phases), one queued, one done,
// one failed, one cancelled.
const fleet = new Map<string, AgentState>(
  [
    agent('a', { goal: 'impl feature', repo: 'r1', status: 'running', phase: 'Implementation', cost: 0.12, elapsedMs: 90_000, activity: 1 }),
    agent('b', { goal: 'run tests', repo: 'r2', status: 'running', phase: 'Testing', cost: 0.04, elapsedMs: 30_000, score: 0.9 }),
    agent('c', { goal: 'queued job', repo: 'r3', status: 'queued', phase: 'Boot', cost: 0 }),
    agent('d', { goal: 'shipped', repo: 'r4', status: 'completed', phase: 'Conclusion', cost: 0.5, score: 0.95 }),
    agent('e', { goal: 'broke', repo: 'r5', status: 'failed', phase: 'Implementation', cost: 0.2 }),
    agent('f', { goal: 'stopped', repo: 'r6', status: 'cancelled', phase: 'Planning', cost: 0.01 })
  ].map((a) => [a.id, a] as const)
);

// ── one row per agent, fields carried through faithfully ──────────────────────
{
  const rows = overviewRows(fleet, new Set());
  ok(rows.length === fleet.size, 'one row per live agent');
  const a = rows.find((r) => r.id === 'a') as OverviewRow;
  eq(a.goal, 'impl feature', 'goal carried through');
  eq(a.repo, 'r1', 'repo carried through');
  eq(a.phase, 'Implementation', 'phase carried through');
  eq(a.cost, 0.12, 'cost carried through');
  eq(a.elapsedMs, 90_000, 'elapsed carried through');
  eq(rows.find((r) => r.id === 'b')?.score, 0.9, 'score carried through when present');
}

// ── orb color matches what the pane/card would render (one vocabulary) ────────
{
  const rows = overviewRows(fleet, new Set());
  for (const r of rows) {
    const a = fleet.get(r.id) as AgentState;
    eq(r.orbColor, computeOrbState(a, false).glowColor, `row ${r.id} orb color == pane orb color`);
  }
  // failed → rose hardStop; completed → jade kryptonite.
  eq(rows.find((r) => r.id === 'e')?.special, 'hardStop', 'failed row → hardStop');
  eq(rows.find((r) => r.id === 'd')?.special, 'kryptonite', 'completed row → kryptonite');
}

// ── sort: running first, terminal last ────────────────────────────────────────
{
  const rows = overviewRows(fleet, new Set());
  eq(rows[0].status, 'running', 'running sorts first');
  ok(rows[rows.length - 1].status === 'cancelled' || rows[rows.length - 1].status === 'failed', 'terminal sorts last');
  // within running, larger elapsed first (a: 90s before b: 30s)
  const running = rows.filter((r) => r.status === 'running').map((r) => r.id);
  eq(running, ['a', 'b'], 'running ordered by elapsed desc');
}

// ── awaiting flag threaded from the permissionWaiting set ─────────────────────
{
  const rows = overviewRows(fleet, new Set(['a']));
  ok(rows.find((r) => r.id === 'a')?.awaiting === true, 'awaiting agent flagged');
  ok(rows.find((r) => r.id === 'b')?.awaiting === false, 'non-waiting agent not flagged');
}

// ── filters partition the fleet; dead-letter folds failed + cancelled ─────────
{
  const rows = overviewRows(fleet, new Set());
  eq(filterRows(rows, 'running').map((r) => r.id).sort(), ['a', 'b'], 'running filter');
  eq(filterRows(rows, 'queued').map((r) => r.id), ['c'], 'queued filter');
  eq(filterRows(rows, 'done').map((r) => r.id), ['d'], 'done filter');
  eq(filterRows(rows, 'dead-letter').map((r) => r.id).sort(), ['e', 'f'], 'dead-letter folds failed + cancelled');
  eq(filterRows(rows, 'all').length, rows.length, 'all filter is identity');
}

// ── counts match the filters in one pass ──────────────────────────────────────
{
  const rows = overviewRows(fleet, new Set());
  const counts = filterCounts(rows);
  eq(counts, { all: 6, running: 2, queued: 1, done: 1, 'dead-letter': 2 }, 'filter counts');
}

// ── elapsed formatter ─────────────────────────────────────────────────────────
{
  eq(formatElapsed(0), '0s', '0ms → 0s');
  eq(formatElapsed(45_000), '45s', 'sub-minute stays in seconds');
  eq(formatElapsed(90_000), '1m 30s', 'minute + seconds');
  eq(formatElapsed(-5), '0s', 'negative clamps to 0s');
}

summary();
