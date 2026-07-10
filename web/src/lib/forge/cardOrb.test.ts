/**
 * cardOrb parity tests — `npx tsx src/lib/forge/cardOrb.test.ts`.
 *
 * Phase 2 proof (Unify-2 §2): a stack card, given a live `AgentState` keyed by
 * its `taskId`, resolves the SAME `OrbState` that `AgentPane` would compute for
 * the identical agent — same pure function, same input, same output, different
 * host component. Also covers the launch lifecycle: no taskId / unknown taskId
 * fall back to the idle orb, and the `permissionWaiting` flag is threaded so an
 * awaiting agent lights the attention orb.
 */
import { orbStateForCard } from './cardOrb';
import { computeOrbState, IDLE_ORB } from './orbState';
import { makeBlank } from '$lib/stores/agentReducer';
import type { AgentState } from '$lib/stores/agents';
import { eq, ok, summary } from '$lib/test-harness';

const agent = (id: string, patch: Partial<AgentState>): AgentState => ({
  ...makeBlank(id),
  ...patch
});

// ── no task / unknown task → the calm idle orb (an unlaunched card) ───────────
{
  const map = new Map<string, AgentState>();
  eq(orbStateForCard(undefined, map, new Set()), IDLE_ORB, 'no taskId → idle orb');
  eq(orbStateForCard('ghost', map, new Set()), IDLE_ORB, 'taskId not in store → idle orb');
}

// ── parity: card orb === AgentPane orb for the identical agent ────────────────
// The agents map is keyed by task id (== AgentState.id), so a card's taskId is
// the store key. For every representative phase/terminal state, the card must
// resolve exactly what a pane would.
{
  const states: Array<Partial<AgentState>> = [
    { status: 'running', phase: 'Implementation', activity: 1 },
    { status: 'running', phase: 'Testing' },
    { status: 'running', phase: 'Planning' },
    { status: 'running', phase: 'Conclusion' },
    { status: 'completed', phase: 'Testing' },
    { status: 'failed', phase: 'Implementation' },
    { status: 'cancelled' },
    { status: 'queued' },
    { status: 'running', phase: 'Implementation', throttled: true },
    { status: 'running', phase: 'Implementation', taskStatus: 'RolledBack' }
  ];
  states.forEach((patch, i) => {
    const id = `t${i}`;
    const a = agent(id, patch);
    const map = new Map([[id, a]]);
    eq(
      orbStateForCard(id, map, new Set()),
      computeOrbState(a, false),
      `card orb matches pane orb for ${JSON.stringify(patch)}`
    );
  });
}

// ── permissionWaiting is threaded through by agent id ─────────────────────────
{
  const a = agent('w1', { status: 'running', phase: 'Implementation' });
  const map = new Map([['w1', a]]);
  eq(
    orbStateForCard('w1', map, new Set(['w1'])),
    computeOrbState(a, true),
    'waiting card resolves the awaiting orb (attentionPulse), same as the pane'
  );
  ok(orbStateForCard('w1', map, new Set(['w1'])).special === 'attentionPulse', 'awaiting → attentionPulse');
}

summary();
