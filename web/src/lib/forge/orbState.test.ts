/**
 * Pure orb-state tests — `npx tsx src/lib/forge/orbState.test.ts`.
 * Verifies the ORB STATE MAP: terminal/blocked overrides win over phase color,
 * the only fully-stopped states are hardStop, and every other state keeps motion.
 */
import { computeOrbState, IDLE_ORB } from './orbState';
import { makeBlank } from '$lib/stores/agentReducer';
import type { AgentState } from '$lib/stores/agents';

let pass = 0;
let fail = 0;
function ok(cond: boolean, name: string) {
  if (cond) pass++;
  else {
    fail++;
    console.error(`✗ ${name}`);
  }
}
function eq(a: unknown, b: unknown, name: string) {
  ok(JSON.stringify(a) === JSON.stringify(b), `${name} (got ${JSON.stringify(a)})`);
}

const agent = (patch: Partial<AgentState>): AgentState => ({ ...makeBlank('t'), ...patch });

// ── idle (no session) → the calm launcher orb ─────────────────────────────────
eq(computeOrbState(null), IDLE_ORB, 'null agent → idle orb');
ok(IDLE_ORB.spinSpeed > 0, 'idle still drifts (never fully stopped)');

// ── phase coloring while running ──────────────────────────────────────────────
{
  const o = computeOrbState(agent({ status: 'running', phase: 'Implementation', activity: 1 }));
  eq(o.glowColor, '#5ee6ff', 'Implementing → plasma cyan');
  ok(o.spinSpeed > 1.5 && o.turbulence >= 0.9, 'Implementing is fast + turbulent');
}
{
  const o = computeOrbState(agent({ status: 'running', phase: 'Testing' }));
  eq(o.glowColor, '#7c3aed', 'Testing → violet (not yellow)');
}
{
  const o = computeOrbState(agent({ status: 'running', phase: 'Planning' }));
  eq(o.glowColor, '#00d4ff', 'Planning → ice');
}
{
  const o = computeOrbState(agent({ status: 'running', phase: 'Conclusion' }));
  eq(o.glowColor, '#9d5cff', 'running + Conclusion → bright violet (scoring)');
}

// ── overrides win over phase ──────────────────────────────────────────────────
{
  const o = computeOrbState(agent({ status: 'completed', phase: 'Testing' }));
  eq(o.glowColor, '#00ff9d', 'completed → jade despite Testing phase');
  eq(o.special, 'kryptonite', 'completed → kryptonite');
  ok(o.spinSpeed > 0 && o.spinSpeed < 0.6, 'completed slows to a drift, not a stop');
}
{
  const o = computeOrbState(agent({ status: 'failed', phase: 'Implementation' }));
  eq(o.glowColor, '#ff0066', 'failed → rose/pink');
  eq(o.special, 'hardStop', 'failed → hardStop');
  eq(o.spinSpeed, 0, 'failed fully stops');
}
{
  const o = computeOrbState(agent({ status: 'cancelled' }));
  eq(o.special, 'hardStop', 'cancelled → hardStop');
  eq(o.spinSpeed, 0, 'cancelled fully stops');
}
{
  const o = computeOrbState(agent({ status: 'running', phase: 'Implementation' }), true);
  eq(o.glowColor, '#ffcc00', 'awaiting → yellow/orange');
  eq(o.special, 'attentionPulse', 'awaiting → attentionPulse');
  ok(o.spinSpeed > 0, 'awaiting keeps spinning (slow)');
}
{
  const o = computeOrbState(agent({ status: 'running', awaitingApproval: true }));
  eq(o.special, 'attentionPulse', 'plan-gate awaitingApproval → attentionPulse');
}
{
  const o = computeOrbState(agent({ status: 'running', throttled: true }));
  eq(o.glowColor, '#ff9500', 'throttled → flame');
  eq(o.special, 'stutter', 'throttled → stutter');
}
{
  const o = computeOrbState(agent({ status: 'running', taskStatus: 'RolledBack' }));
  eq(o.glowColor, '#ff4500', 'RolledBack → ember');
  eq(o.special, 'reverseSpin', 'RolledBack → reverseSpin');
}
{
  const o = computeOrbState(agent({ status: 'queued' }));
  eq(o.glowColor, '#0088aa', 'queued → iceDeep');
  ok(o.spinSpeed > 0, 'queued still turns slowly');
}

// ── invariant: only hardStop fully stops ──────────────────────────────────────
{
  const states: AgentState[] = [
    agent({ status: 'running', phase: 'Planning' }),
    agent({ status: 'running', phase: 'Testing' }),
    agent({ status: 'completed' }),
    agent({ status: 'running', throttled: true }),
    agent({ status: 'queued' }),
    agent({ status: 'running', taskStatus: 'RolledBack' })
  ];
  ok(states.every((a) => computeOrbState(a).spinSpeed > 0), 'non-hardStop states all keep some spin');
}

console.log(`\norbState: ${pass} passed, ${fail} failed`);
if (fail > 0) process.exit(1);
