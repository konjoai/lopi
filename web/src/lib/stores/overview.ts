/**
 * overview — the app-wide rollup that backs the `/overview` route.
 *
 * The `agents` store is already the single source of truth for every live
 * session app-wide (every task — Forge pane or Loop-Stack card — flows through
 * `createTask` and lands here off the one `AgentEvent` stream). So the Overview
 * is a pure projection of that map: one row per agent, orb-colored, sortable,
 * filterable by lifecycle. Kept free of `$app`/store subscriptions so it's unit
 * testable against a seeded map — the row it builds is exactly what the route
 * renders.
 *
 * This is the sole replacement for Fleet + Dashboard + Pulse's *information*
 * (per-agent metrics, whole-fleet glance, live status). It deliberately does
 * NOT absorb Constellation's 3D orbital rendering — that surface is cut, not
 * folded in here.
 */
import type { AgentState, Status } from './agents';
import { computeOrbState } from '$lib/forge/orbState';

/** One dense Overview row — a flattened, display-ready view of one agent. */
export interface OverviewRow {
  /** Agent/task id — the `agents` map key and the click-through target. */
  id: string;
  goal: string;
  repo: string;
  branch: string;
  phase: string;
  status: Status;
  elapsedMs: number;
  cost: number;
  /** Synthetic 0..1 composite, when scored. */
  score?: number;
  attempt: number;
  /** `computeOrbState(agent).glowColor` — the status dot color, one vocabulary
   *  with the pane/card orb. */
  orbColor: string;
  /** The orb's motion flourish (`hardStop`/`kryptonite`/…), for the dot's class. */
  special: string;
  /** True when paused for a human (plan gate / CLI prompt). */
  awaiting: boolean;
}

/** The lifecycle buckets the Overview can filter to. `'dead-letter'` is the
 *  folded-in Tasks dead-letter view (failed/cancelled), now a filter here
 *  rather than its own page. */
export type StatusFilter = 'all' | 'running' | 'queued' | 'done' | 'dead-letter';

/** Sort rank — active work first, terminal last, so the eye lands on what's
 *  live. Within a rank, `overviewRows` keeps most-recently-started first. */
function statusRank(status: Status): number {
  switch (status) {
    case 'running':
      return 0;
    case 'queued':
      return 1;
    case 'completed':
      return 2;
    case 'failed':
      return 3;
    case 'cancelled':
      return 4;
    default:
      return 5;
  }
}

/** Project the live `agents` map into ordered Overview rows. `waiting` is the
 *  `permissionWaiting` set (agent ids paused for a human). */
export function overviewRows(
  agents: Map<string, AgentState>,
  waiting: ReadonlySet<string>
): OverviewRow[] {
  const rows: OverviewRow[] = [];
  for (const a of agents.values()) {
    const isWaiting = waiting.has(a.id);
    const orb = computeOrbState(a, isWaiting);
    rows.push({
      id: a.id,
      goal: a.goal,
      repo: a.repo,
      branch: a.branch,
      phase: a.phase,
      status: a.status,
      elapsedMs: a.elapsedMs,
      cost: a.cost,
      score: a.score,
      attempt: a.attempt,
      orbColor: orb.glowColor,
      special: orb.special,
      awaiting: isWaiting
    });
  }
  rows.sort((x, y) => {
    const r = statusRank(x.status) - statusRank(y.status);
    return r !== 0 ? r : y.elapsedMs - x.elapsedMs;
  });
  return rows;
}

/** True when a row belongs in the given lifecycle filter. */
export function rowMatchesFilter(row: OverviewRow, filter: StatusFilter): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'running':
      return row.status === 'running';
    case 'queued':
      return row.status === 'queued';
    case 'done':
      return row.status === 'completed';
    case 'dead-letter':
      return row.status === 'failed' || row.status === 'cancelled';
  }
}

/** Apply a lifecycle filter to a row list (pure; preserves order). */
export function filterRows(rows: OverviewRow[], filter: StatusFilter): OverviewRow[] {
  return filter === 'all' ? rows : rows.filter((r) => rowMatchesFilter(r, filter));
}

/** Per-filter counts for the filter chips, computed in one pass. */
export function filterCounts(rows: OverviewRow[]): Record<StatusFilter, number> {
  const counts: Record<StatusFilter, number> = {
    all: rows.length,
    running: 0,
    queued: 0,
    done: 0,
    'dead-letter': 0
  };
  for (const r of rows) {
    if (r.status === 'running') counts.running++;
    else if (r.status === 'queued') counts.queued++;
    else if (r.status === 'completed') counts.done++;
    else if (r.status === 'failed' || r.status === 'cancelled') counts['dead-letter']++;
  }
  return counts;
}

/** Compact elapsed formatter shared by the Overview rows (e.g. `2m 5s`). */
export function formatElapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(s / 60);
  return m > 0 ? `${m}m ${s % 60}s` : `${s}s`;
}
