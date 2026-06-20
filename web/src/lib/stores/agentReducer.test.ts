/**
 * Agent reducer tests — run with `npx tsx src/lib/stores/agentReducer.test.ts`.
 *
 * The reducer is a pure `(Map, AgentEvent) → Map`, so every event kind and its
 * branches can be exercised directly. Covers the score composite, terminal
 * success/failure routing, stimulus flares, and snapshot-seed defaults.
 */
import { reduce, makeBlank } from './agentReducer';
import type { AgentEvent } from '$lib/types';

let pass = 0;
let fail = 0;

function ok(cond: boolean, name: string) {
  if (cond) pass++;
  else {
    fail++;
    console.error(`✗ ${name}`);
  }
}

function close(a: number, b: number, name: string, eps = 1e-9) {
  ok(Math.abs(a - b) < eps, `${name} (got ${a}, want ${b})`);
}

const empty = new Map();
const ID = 'task-1';

// Seed a map already containing a running agent, for events that need `cur`.
function seeded() {
  return reduce(reduce(empty, { type: 'task_queued', task_id: ID, goal: 'g', priority: 'Normal' }), {
    type: 'task_started',
    task_id: ID,
    attempt: 1,
    branch: 'feat/x',
    repo: './r'
  });
}

// ── task_queued ──────────────────────────────────────────────────────────────
{
  const m = reduce(empty, { type: 'task_queued', task_id: ID, goal: 'build it', priority: 'High' });
  const a = m.get(ID)!;
  ok(a !== undefined, 'task_queued creates an agent');
  ok(a.status === 'queued' && a.phase === 'Boot', 'task_queued is queued/Boot');
  ok(a.goal === 'build it', 'task_queued keeps goal');
  ok(a.stimulusKind === 'request', 'task_queued flags a request stimulus');
}

// ── task_started ─────────────────────────────────────────────────────────────
{
  // On an empty map it seeds via makeBlank, then marks running.
  const m = reduce(empty, { type: 'task_started', task_id: ID, attempt: 2, branch: 'b', repo: './r' });
  const a = m.get(ID)!;
  ok(a.status === 'running', 'task_started → running even without prior queue');
  ok(a.branch === 'b' && a.repo === './r' && a.attempt === 2, 'task_started carries branch/repo/attempt');
}
{
  // With an existing agent, startedAt is preserved.
  const s = seeded();
  const started = s.get(ID)!.startedAt;
  const m = reduce(s, { type: 'task_started', task_id: ID, attempt: 3, branch: 'b2' });
  ok(m.get(ID)!.startedAt === started, 'task_started preserves startedAt');
  ok(m.get(ID)!.repo === './r', 'task_started keeps prior repo when omitted');
}

// ── status_changed — terminal routing ───────────────────────────────────────
{
  const m = reduce(seeded(), { type: 'status_changed', task_id: ID, status: { Failed: { reason: 'boom' } }, attempt: 1 });
  ok(m.get(ID)!.status === 'failed', 'status_changed Failed → failed');
}
{
  const m = reduce(seeded(), {
    type: 'status_changed',
    task_id: ID,
    status: { Success: { branch: 'b', pr_url: null } },
    attempt: 1
  });
  ok(m.get(ID)!.status === 'completed', 'status_changed Success → completed');
}
{
  const m = reduce(seeded(), { type: 'status_changed', task_id: ID, status: 'Implementing', attempt: 1 });
  ok(m.get(ID)!.status === 'running', 'status_changed non-terminal → running');
}
{
  // No agent yet → the event is ignored, nothing created.
  const m = reduce(empty, { type: 'status_changed', task_id: ID, status: 'Planning', attempt: 1 });
  ok(!m.has(ID), 'status_changed with no agent is a no-op');
}

// ── log_line ─────────────────────────────────────────────────────────────────
{
  const m = reduce(seeded(), { type: 'log_line', task_id: ID, line: 'thinking…', level: 'info', ts: '2026-01-01T00:00:00Z' });
  ok(m.get(ID)!.thought === 'thinking…', 'log_line updates the thought preview');
}

// ── score_updated — composite + health drift ─────────────────────────────────
{
  const m = reduce(seeded(), { type: 'score_updated', task_id: ID, test_pass_rate: 1, lint_errors: 0, diff_lines: 10 });
  const a = m.get(ID)!;
  close(a.score!, 0.85, 'score composite for perfect pass / no lint');
  // health drifts: prior 0.85 * 0.7 + 0.85 * 0.3 = 0.85
  close(a.health, 0.85, 'health drifts toward score');
  ok(a.diffLines === 10, 'score_updated records diff lines');
}
{
  // Lint penalty caps at 0.15.
  const m = reduce(seeded(), { type: 'score_updated', task_id: ID, test_pass_rate: 1, lint_errors: 1000, diff_lines: 0 });
  close(m.get(ID)!.score!, 0.7, 'lint penalty is capped at 0.15');
}

// ── task_completed ───────────────────────────────────────────────────────────
{
  const m = reduce(seeded(), { type: 'task_completed', task_id: ID, outcome: { Success: { branch: 'b', pr_url: null } }, total_attempts: 2 });
  const a = m.get(ID)!;
  ok(a.status === 'completed' && a.phase === 'Conclusion', 'task_completed success → completed/Conclusion');
  ok(a.stimulusKind === 'success', 'task_completed success flares a success stimulus');
}
{
  const m = reduce(seeded(), { type: 'task_completed', task_id: ID, outcome: { Failed: { reason: 'x' } }, total_attempts: 5 });
  ok(m.get(ID)!.status === 'failed' && m.get(ID)!.stimulusKind === 'failure', 'task_completed failure → failed/failure');
}

// ── task_cancelled ───────────────────────────────────────────────────────────
{
  const m = reduce(seeded(), { type: 'task_cancelled', task_id: ID });
  ok(m.get(ID)!.status === 'cancelled' && m.get(ID)!.activity === 0, 'task_cancelled → cancelled, activity 0');
}

// ── turn_metrics ─────────────────────────────────────────────────────────────
{
  const m = reduce(seeded(), { type: 'turn_metrics', task_id: ID, pressure: 0.6, activity: 0.4, tokens_per_sec: 12, cost_usd: 0.5 });
  const a = m.get(ID)!;
  ok(a.pressure === 0.6 && a.activity === 0.4 && a.cost === 0.5, 'turn_metrics updates pressure/activity/cost');
}

// ── verifier_verdict ─────────────────────────────────────────────────────────
{
  const m = reduce(seeded(), { type: 'verifier_verdict', task_id: ID, passed: false, gaps: ['g1'], fix_hints: ['h1'] });
  const a = m.get(ID)!;
  ok(a.verifierPassed === false && a.stimulusKind === 'failure', 'failing verdict flares failure');
  ok(a.verifierGaps?.[0] === 'g1' && a.verifierFixHints?.[0] === 'h1', 'verdict records gaps + hints');
}

// ── budget_exceeded ──────────────────────────────────────────────────────────
{
  const before = seeded().get(ID)!.stimulus;
  const m = reduce(seeded(), { type: 'budget_exceeded', task_id: ID, scope: 'task', limit_usd: 1, burned_usd: 2 });
  ok(m.get(ID)!.stimulus >= before && m.get(ID)!.stimulusKind === 'failure', 'task-scoped breach flares the orb');
}
{
  // Fleet-wide breach (null task_id) must not throw or mutate any agent.
  const m = reduce(seeded(), { type: 'budget_exceeded', task_id: null, scope: 'fleet', limit_usd: 1, burned_usd: 2 });
  ok(m.has(ID), 'fleet breach leaves agents intact');
}

// ── immutability ─────────────────────────────────────────────────────────────
{
  const base = seeded();
  const out = reduce(base, { type: 'turn_metrics', task_id: ID, pressure: 0.9, activity: 0.9, tokens_per_sec: 1, cost_usd: 9 });
  ok(out !== base, 'reduce returns a new map');
  ok(base.get(ID)!.cost === 0, 'reduce does not mutate the input map');
}

// ── makeBlank ────────────────────────────────────────────────────────────────
{
  const a = makeBlank('z');
  ok(a.id === 'z' && a.status === 'queued' && a.phase === 'Boot', 'makeBlank seeds sane defaults');
  ok(a.health === 0.85 && a.pressure === 0.05, 'makeBlank seeds initial gauges');
}

// ── unknown event kinds are inert ────────────────────────────────────────────
{
  const m = reduce(seeded(), { type: 'pool_stats', running: 1, queued: 0, succeeded: 0, failed: 0, uptime_secs: 1 } as AgentEvent);
  ok(m.get(ID)!.status === 'running', 'pool_stats leaves agent state unchanged');
}

console.log(`\nagentReducer: ${pass} passed, ${fail} failed`);
if (fail > 0) process.exit(1);
