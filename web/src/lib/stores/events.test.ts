/**
 * Event-feed `describe` tests — runs as a standalone Node script.
 * Usage: `npx tsx src/lib/stores/events.test.ts` from web/
 *
 * `describe` is pure (no store side-effects), so we can exercise the
 * summary + tier mapping for every AgentEvent kind directly.
 */
import { describe } from './events';
import type { AgentEvent } from '$lib/types';

let pass = 0;
let fail = 0;

function eq(actual: unknown, expected: unknown, name: string) {
  if (Object.is(actual, expected)) {
    pass++;
  } else {
    fail++;
    console.error(`✗ ${name}: expected ${expected}, got ${actual}`);
  }
}

function truthy(v: unknown, name: string) {
  if (v) pass++;
  else {
    fail++;
    console.error(`✗ ${name}`);
  }
}

// ── tier mapping ──────────────────────────────────────────────────────────────
eq(describe({ type: 'task_queued', task_id: 't', goal: 'g', priority: 'Normal' }).tier, 'info', 'queued is info');

eq(
  describe({ type: 'log_line', task_id: 't', line: 'boom', level: 'error', ts: '' }).tier,
  'bad',
  'error log is bad'
);
eq(
  describe({ type: 'log_line', task_id: 't', line: 'hmm', level: 'warn', ts: '' }).tier,
  'warn',
  'warn log is warn'
);

eq(
  describe({ type: 'score_updated', task_id: 't', test_pass_rate: 0.95, lint_errors: 0, diff_lines: 10 }).tier,
  'good',
  'high score is good'
);
eq(
  describe({ type: 'score_updated', task_id: 't', test_pass_rate: 0.5, lint_errors: 3, diff_lines: 10 }).tier,
  'warn',
  'low score with lint is warn'
);

const okDone: AgentEvent = { type: 'task_completed', task_id: 't', outcome: { Success: { branch: 'b', pr_url: null } }, total_attempts: 2 };
eq(describe(okDone).tier, 'good', 'success completion is good');
const failDone: AgentEvent = { type: 'task_completed', task_id: 't', outcome: { Failed: { reason: 'x' } }, total_attempts: 3 };
eq(describe(failDone).tier, 'bad', 'failed completion is bad');

eq(
  describe({ type: 'verifier_verdict', task_id: 't', passed: true, gaps: [], fix_hints: [] }).tier,
  'good',
  'verifier pass is good'
);
eq(
  describe({ type: 'verifier_verdict', task_id: 't', passed: false, gaps: ['a'], fix_hints: [] }).tier,
  'warn',
  'verifier fail is warn'
);

eq(
  describe({ type: 'budget_exceeded', task_id: null, scope: 'fleet', limit_usd: 5, burned_usd: 6 }).tier,
  'bad',
  'budget breach is bad'
);

eq(
  describe({ type: 'turn_metrics', task_id: 't', pressure: 0.9, activity: 0.5, tokens_per_sec: 40, cost_usd: 0.01 }).tier,
  'warn',
  'high pressure turn is warn'
);

// ── summary content ───────────────────────────────────────────────────────────
truthy(
  describe({ type: 'task_queued', task_id: 't', goal: 'fix the bug', priority: 'High' }).summary.includes('fix the bug'),
  'queued summary names the goal'
);
truthy(
  describe({ type: 'budget_exceeded', task_id: null, scope: 'agent', limit_usd: 2, burned_usd: 2.5 }).summary.includes('agent'),
  'budget summary names the scope'
);
truthy(
  describe({ type: 'verifier_verdict', task_id: 't', passed: false, gaps: ['untested path'], fix_hints: [] }).summary.includes('untested path'),
  'verifier summary surfaces the first gap'
);

console.log(`\n── Result: ${pass} passed, ${fail} failed ──`);
if (fail > 0) process.exit(1);
