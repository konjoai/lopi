/**
 * Pure agent-reducer tests — run with `npx tsx src/lib/stores/agentReducer.test.ts`.
 * No browser, no Svelte: just `AgentEvent → AgentState` folds. The reducer was
 * extracted from the store precisely so it could be tested in isolation.
 */
import { reduce, makeBlank } from './agentReducer';
import type { AgentState } from './agents';
import type { AgentEvent } from '$lib/types';
import { eq, ok, namedSummary } from '$lib/test-harness';

function approx(actual: number, expected: number, name: string, eps = 1e-9) {
  ok(Math.abs(actual - expected) < eps, `${name} (got ${actual}, want ${expected})`);
}

const empty = () => new Map<string, AgentState>();
const ev = (e: Record<string, unknown>) => e as unknown as AgentEvent;
const ID = 'task-1';

// ── task_queued seeds a queued agent ─────────────────────────────────────────
{
  const m = reduce(empty(), ev({ type: 'task_queued', task_id: ID, goal: 'do a thing' }));
  const a = m.get(ID)!;
  eq(a.status, 'queued', 'task_queued → queued');
  eq(a.goal, 'do a thing', 'task_queued keeps goal');
  eq(a.phase, 'Boot', 'task_queued → Boot phase');
}

// ── task_started promotes to running with branch/repo ────────────────────────
{
  let m = reduce(empty(), ev({ type: 'task_queued', task_id: ID, goal: 'g' }));
  m = reduce(m, ev({ type: 'task_started', task_id: ID, attempt: 1, branch: 'feat/x', repo: '~/r' }));
  const a = m.get(ID)!;
  eq(a.status, 'running', 'task_started → running');
  eq(a.branch, 'feat/x', 'task_started sets branch');
  eq(a.repo, '~/r', 'task_started sets repo');
  eq(a.attempt, 1, 'task_started sets attempt');
}

// ── turn_metrics drives the Forge inputs ─────────────────────────────────────
{
  const seed = new Map([[ID, makeBlank(ID)]]);
  const m = reduce(seed, ev({ type: 'turn_metrics', task_id: ID, pressure: 0.7, activity: 0.4, cost_usd: 0.012 }));
  const a = m.get(ID)!;
  approx(a.pressure, 0.7, 'turn_metrics pressure');
  approx(a.activity, 0.4, 'turn_metrics activity');
  approx(a.cost, 0.012, 'turn_metrics cost');
}

// ── status_changed maps to a phase and stays running ─────────────────────────
{
  const seed = new Map([[ID, makeBlank(ID)]]);
  const m = reduce(seed, ev({ type: 'status_changed', task_id: ID, status: 'Implementing', attempt: 2 }));
  const a = m.get(ID)!;
  eq(a.status, 'running', 'status_changed mid-run → running');
  eq(a.phase, 'Implementation', 'status_changed Implementing → Implementation phase');
  eq(a.attempt, 2, 'status_changed updates attempt');
}

// ── task_completed: success vs failure terminal flash ────────────────────────
{
  const seed = new Map([[ID, makeBlank(ID)]]);
  const ok2 = reduce(seed, ev({ type: 'task_completed', task_id: ID, outcome: 'Success', total_attempts: 1 }));
  eq(ok2.get(ID)!.status, 'completed', 'task_completed Success → completed');
  eq(ok2.get(ID)!.phase, 'Conclusion', 'task_completed → Conclusion');
  eq(ok2.get(ID)!.stimulusKind, 'success', 'success flash');

  const bad = reduce(seed, ev({ type: 'task_completed', task_id: ID, outcome: { Failed: 'boom' }, total_attempts: 3 }));
  eq(bad.get(ID)!.status, 'failed', 'task_completed Failed → failed');
  eq(bad.get(ID)!.stimulusKind, 'failure', 'failure flare');
}

// ── score_updated composite + health drift, with lint penalty clamp ──────────
{
  const seed = new Map([[ID, { ...makeBlank(ID), health: 0.85 }]]);
  const m = reduce(seed, ev({ type: 'score_updated', task_id: ID, test_pass_rate: 0.9, lint_errors: 0, diff_lines: 40 }));
  const a = m.get(ID)!;
  approx(a.score!, 0.765, 'score = 0.9*0.85');
  approx(a.health, 0.85 * 0.7 + 0.765 * 0.3, 'health drifts toward score');

  // lint penalty caps at 0.15 even with many errors
  const penal = reduce(seed, ev({ type: 'score_updated', task_id: ID, test_pass_rate: 1, lint_errors: 999, diff_lines: 5 }));
  approx(penal.get(ID)!.score!, 0.7, 'lint penalty clamped at 0.15 (0.85 − 0.15)');
}

// ── verifier_verdict pulses the orb in the verdict color ─────────────────────
{
  const seed = new Map([[ID, makeBlank(ID)]]);
  const p = reduce(seed, ev({ type: 'verifier_verdict', task_id: ID, passed: true, gaps: [], fix_hints: [] }));
  eq(p.get(ID)!.stimulusKind, 'success', 'verifier pass → success');
  const f = reduce(seed, ev({ type: 'verifier_verdict', task_id: ID, passed: false, gaps: ['x'], fix_hints: ['y'] }));
  eq(f.get(ID)!.stimulusKind, 'failure', 'verifier fail → failure');
  eq(f.get(ID)!.verifierGaps, ['x'], 'verifier gaps recorded');
}

// ── events for an unknown task are ignored ───────────────────────────────────
{
  const m = reduce(empty(), ev({ type: 'turn_metrics', task_id: 'ghost', pressure: 1, activity: 1, cost_usd: 9 }));
  eq(m.size, 0, 'metrics for unknown task → no-op');
}

// ── immutability: input map and agent are never mutated ──────────────────────
{
  const before = makeBlank(ID);
  const seed = new Map([[ID, before]]);
  const after = reduce(seed, ev({ type: 'turn_metrics', task_id: ID, pressure: 0.9, activity: 0.9, cost_usd: 1 }));
  ok(after !== seed, 'returns a new map');
  ok(seed.get(ID) === before, 'input map entry untouched');
  approx(before.pressure, 0.05, 'original agent object not mutated');
}

// ── stream-json pane events (Phase 1 event spine) ────────────────────────────
{
  // tool_call seeds an agent if absent and counts/labels the tool.
  const m = reduce(empty(), ev({ type: 'tool_call', task_id: ID, tool: 'Bash', summary: 'ls -la' }));
  const a = m.get(ID)!;
  eq(a.lastTool, 'Bash', 'tool_call sets lastTool');
  eq(a.toolCalls, 1, 'tool_call increments toolCalls');
  eq(a.thought, '🔧 Bash(ls -la)', 'tool_call sets thought label');
}
{
  // token_delta accumulates output tokens and tracks the current turn.
  const seed = new Map([[ID, makeBlank(ID)]]);
  let m = reduce(seed, ev({ type: 'token_delta', task_id: ID, output_tokens: 50, input_tokens: 3, cache_read_tokens: 100 }));
  m = reduce(m, ev({ type: 'token_delta', task_id: ID, output_tokens: 70, input_tokens: 4, cache_read_tokens: 120 }));
  const a = m.get(ID)!;
  eq(a.outputTokens, 120, 'token_delta accumulates output tokens');
  eq(a.cacheReadTokens, 120, 'token_delta tracks latest cache reads');
}
{
  // api_retry records throttle + utilization.
  const seed = new Map([[ID, makeBlank(ID)]]);
  const m = reduce(seed, ev({ type: 'api_retry', task_id: ID, status: 'allowed_warning', limit_type: 'seven_day', utilization: 0.92 }));
  const a = m.get(ID)!;
  ok(a.throttled === true, 'api_retry sets throttled');
  approx(a.utilization!, 0.92, 'api_retry records utilization');
}
{
  // cost sets accumulated cost, turns, and the resumable session id.
  const seed = new Map([[ID, makeBlank(ID)]]);
  const m = reduce(seed, ev({ type: 'cost', task_id: ID, cost_usd: 0.048, num_turns: 3, session_id: 'sess-1' }));
  const a = m.get(ID)!;
  approx(a.cost, 0.048, 'cost sets accumulated cost');
  eq(a.numTurns, 3, 'cost sets num_turns');
  eq(a.sessionId, 'sess-1', 'cost threads session_id');
}
{
  // phase records Claude's own phase label.
  const seed = new Map([[ID, makeBlank(ID)]]);
  const m = reduce(seed, ev({ type: 'phase', task_id: ID, phase: 'review_ready' }));
  eq(m.get(ID)!.claudePhase, 'review_ready', 'phase sets claudePhase');
}
{
  // events for an unknown task are a no-op (except tool_call, which seeds).
  eq(reduce(empty(), ev({ type: 'cost', task_id: 'ghost', cost_usd: 1, num_turns: 1, session_id: 's' })).size, 0, 'cost for unknown task → no-op');
  eq(reduce(empty(), ev({ type: 'phase', task_id: 'ghost', phase: 'x' })).size, 0, 'phase for unknown task → no-op');
}

namedSummary('agentReducer');
