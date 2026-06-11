/**
 * connections.ts tests — runs as standalone script.
 * Usage: npx tsx src/lib/forge/connections.test.ts
 */
import { computeConnections, connectionsFor } from './connections';
import type { AgentState } from '$lib/stores/agents';

let pass = 0;
let fail = 0;

function expect(cond: boolean, name: string) {
  if (cond) {
    pass++;
    console.log(`  ✓ ${name}`);
  } else {
    fail++;
    console.error(`  ✗ ${name}`);
  }
}

function makeAgent(over: Partial<AgentState>): AgentState {
  return {
    id: over.id ?? 'a',
    goal: over.goal ?? 'unnamed task',
    repo: over.repo ?? '',
    branch: '',
    status: over.status ?? 'running',
    taskStatus: 'Planning',
    phase: over.phase ?? 'Planning',
    attempt: 0,
    startedAt: 0,
    elapsedMs: 0,
    pressure: 0.3,
    activity: 0.4,
    health: 0.85,
    cost: 0,
    stimulus: 0,
    stimulusKind: 'request',
    ...over
  };
}

console.log('\n── computeConnections ─────────────────────────────────');

// Single agent → no connections possible
{
  const m = new Map([['a', makeAgent({ id: 'a', repo: '~/x' })]]);
  expect(computeConnections(m).length === 0, 'one agent → 0 connections');
}

// Two agents, same repo → connected
{
  const m = new Map([
    ['a', makeAgent({ id: 'a', repo: '~/kyro' })],
    ['b', makeAgent({ id: 'b', repo: '~/kyro' })]
  ]);
  const conns = computeConnections(m);
  expect(conns.length === 1, 'shared repo → 1 connection');
  expect(conns[0].strength >= 0.55, 'shared repo strength ≥ 0.55');
  expect(conns[0].reasons.some((r) => r.includes('same repo')), 'reason mentions same repo');
}

// Two agents, different repos, no goal overlap → no connection
{
  const m = new Map([
    ['a', makeAgent({ id: 'a', repo: '~/kyro', goal: 'add cache' })],
    ['b', makeAgent({ id: 'b', repo: '~/vectro', goal: 'fix encoder' })]
  ]);
  expect(computeConnections(m).length === 0, 'different repo + no overlap → 0 connections');
}

// Same repo + same phase → strength bonus + phaseSync
{
  const m = new Map([
    ['a', makeAgent({ id: 'a', repo: '~/k', phase: 'Implementation' })],
    ['b', makeAgent({ id: 'b', repo: '~/k', phase: 'Implementation' })]
  ]);
  const conns = computeConnections(m);
  expect(conns[0].strength >= 0.8, 'same repo + same phase → strength ≥ 0.8');
  expect(conns[0].phaseSync === true, 'phaseSync flag set');
  expect(conns[0].reasons.some((r) => r.includes('Implementation')), 'reason names the phase');
}

// Goal keyword overlap (≥2 keywords, no shared repo)
{
  const m = new Map([
    [
      'a',
      makeAgent({
        id: 'a',
        repo: '~/r1',
        goal: 'Add Redis-backed semantic cache layer'
      })
    ],
    [
      'b',
      makeAgent({
        id: 'b',
        repo: '~/r2',
        goal: 'Refactor semantic cache into Redis-aware module'
      })
    ]
  ]);
  const conns = computeConnections(m);
  expect(conns.length === 1, '≥2 shared keywords → connection without shared repo');
  expect(
    conns[0].reasons.some((r) => r.includes('shared goal keywords')),
    'reason mentions shared keywords'
  );
}

// Stop words alone do NOT trigger a connection
{
  const m = new Map([
    ['a', makeAgent({ id: 'a', repo: '~/r1', goal: 'Add a fix to the new file' })],
    ['b', makeAgent({ id: 'b', repo: '~/r2', goal: 'Use this in all the new tests' })]
  ]);
  // Intersection: {add, the, new, a, an, ...} — all stop words
  expect(computeConnections(m).length === 0, 'stop-word-only overlap → 0 connections');
}

// Three agents, all same repo → 3 connections (3 choose 2)
{
  const m = new Map([
    ['a', makeAgent({ id: 'a', repo: '~/k' })],
    ['b', makeAgent({ id: 'b', repo: '~/k' })],
    ['c', makeAgent({ id: 'c', repo: '~/k' })]
  ]);
  expect(computeConnections(m).length === 3, '3 same-repo agents → 3 pairs');
}

// Canonical ids: pair ordering is independent of insertion order
{
  const ab = new Map([
    ['a', makeAgent({ id: 'a', repo: '~/k' })],
    ['b', makeAgent({ id: 'b', repo: '~/k' })]
  ]);
  const ba = new Map([
    ['b', makeAgent({ id: 'b', repo: '~/k' })],
    ['a', makeAgent({ id: 'a', repo: '~/k' })]
  ]);
  expect(
    computeConnections(ab)[0].id === computeConnections(ba)[0].id,
    'pair id is canonical regardless of map order'
  );
}

// Completed agents are excluded
{
  const m = new Map([
    ['a', makeAgent({ id: 'a', repo: '~/k', status: 'completed' })],
    ['b', makeAgent({ id: 'b', repo: '~/k', status: 'running' })]
  ]);
  expect(computeConnections(m).length === 0, 'completed agent excluded from connections');
}

// Strength clamped to 1.0
{
  const m = new Map([
    [
      'a',
      makeAgent({
        id: 'a',
        repo: '~/k',
        phase: 'Implementation',
        goal: 'redis cache semantic vectoring quantization implementation logic'
      })
    ],
    [
      'b',
      makeAgent({
        id: 'b',
        repo: '~/k',
        phase: 'Implementation',
        goal: 'redis cache semantic vectoring quantization implementation logic'
      })
    ]
  ]);
  expect(computeConnections(m)[0].strength <= 1.0, 'strength clamped to 1.0');
}

console.log('\n── connectionsFor ─────────────────────────────────────');
{
  const m = new Map([
    ['a', makeAgent({ id: 'a', repo: '~/k' })],
    ['b', makeAgent({ id: 'b', repo: '~/k' })],
    ['c', makeAgent({ id: 'c', repo: '~/k' })]
  ]);
  const all = computeConnections(m);
  expect(connectionsFor(all, 'a').length === 2, 'agent a has 2 connections');
  expect(connectionsFor(all, 'b').length === 2, 'agent b has 2 connections');
  expect(connectionsFor(all, 'unknown').length === 0, 'unknown agent has 0 connections');
}

console.log(`\n── Result: ${pass} passed, ${fail} failed ──`);
if (fail > 0) process.exit(1);
