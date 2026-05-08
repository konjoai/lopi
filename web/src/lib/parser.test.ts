/**
 * Parser tests — runs as a standalone Node script (no test runner needed).
 * Usage: `npx tsx src/lib/parser.test.ts` from web/
 *
 * Covers:
 *   - All AgentEvent variants round-trip
 *   - TaskStatus unit + struct variants
 *   - Snapshot parsing
 *   - Malformed inputs return null (defence-in-depth)
 *   - taskStatusToPhase mapping for every TaskStatus
 *   - isTerminalStatus boundary cases
 */
import {
  parseWireMessage,
  parseTaskStatus,
  parseAgentEvent,
  parseSnapshot,
  taskStatusToPhase,
  isTerminalStatus
} from './parser';
import type { Phase } from './types';

let pass = 0;
let fail = 0;

function eq(got: unknown, want: unknown, name: string) {
  const a = JSON.stringify(got);
  const b = JSON.stringify(want);
  if (a === b) {
    pass++;
    console.log(`  ✓ ${name}`);
  } else {
    fail++;
    console.error(`  ✗ ${name}\n      got:  ${a}\n      want: ${b}`);
  }
}

function assertNotNull(got: unknown, name: string) {
  if (got !== null) {
    pass++;
    console.log(`  ✓ ${name}`);
  } else {
    fail++;
    console.error(`  ✗ ${name} — got null`);
  }
}

function assertNull(got: unknown, name: string) {
  if (got === null) {
    pass++;
    console.log(`  ✓ ${name}`);
  } else {
    fail++;
    console.error(`  ✗ ${name} — expected null, got ${JSON.stringify(got)}`);
  }
}

console.log('\n── parseTaskStatus ─────────────────────────────────────');
eq(parseTaskStatus('Queued'), 'Queued', 'unit variant Queued');
eq(parseTaskStatus('Planning'), 'Planning', 'unit variant Planning');
eq(parseTaskStatus('Implementing'), 'Implementing', 'unit variant Implementing');
eq(parseTaskStatus('RolledBack'), 'RolledBack', 'unit variant RolledBack');
eq(
  parseTaskStatus({ Retrying: { attempt: 2 } }),
  { Retrying: { attempt: 2 } },
  'struct variant Retrying'
);
eq(
  parseTaskStatus({ Success: { branch: 'feat/x', pr_url: 'https://github.com/o/r/pull/1' } }),
  { Success: { branch: 'feat/x', pr_url: 'https://github.com/o/r/pull/1' } },
  'struct variant Success with pr_url'
);
eq(
  parseTaskStatus({ Success: { branch: 'feat/x', pr_url: null } }),
  { Success: { branch: 'feat/x', pr_url: null } },
  'struct variant Success with null pr_url'
);
eq(
  parseTaskStatus({ Failed: { reason: 'TurnLimitExceeded' } }),
  { Failed: { reason: 'TurnLimitExceeded' } },
  'struct variant Failed'
);
assertNull(parseTaskStatus('NotAStatus'), 'unknown unit variant rejected');
assertNull(parseTaskStatus({ Retrying: { attempt: 'two' } }), 'wrong-type Retrying rejected');
assertNull(parseTaskStatus({ Failed: {} }), 'missing reason rejected');
assertNull(parseTaskStatus(null), 'null input rejected');
assertNull(parseTaskStatus(42), 'number input rejected');

console.log('\n── taskStatusToPhase ──────────────────────────────────');
const phaseFor: [string | object, Phase][] = [
  ['Queued', 'Boot'],
  ['Planning', 'Planning'],
  ['Implementing', 'Implementation'],
  ['Testing', 'Testing'],
  ['Scoring', 'Conclusion'],
  ['RolledBack', 'Conclusion'],
  [{ Retrying: { attempt: 1 } }, 'Discovery'],
  [{ Success: { branch: 'x', pr_url: null } }, 'Conclusion'],
  [{ Failed: { reason: 'x' } }, 'Conclusion']
];
for (const [s, expected] of phaseFor) {
  eq(taskStatusToPhase(s as any), expected, `${JSON.stringify(s)} → ${expected}`);
}
eq(taskStatusToPhase(null), 'Boot', 'null → Boot');
eq(taskStatusToPhase(undefined), 'Boot', 'undefined → Boot');

console.log('\n── isTerminalStatus ───────────────────────────────────');
eq(isTerminalStatus('Queued'), false, 'Queued not terminal');
eq(isTerminalStatus('Planning'), false, 'Planning not terminal');
eq(isTerminalStatus('RolledBack'), true, 'RolledBack terminal');
eq(isTerminalStatus({ Success: { branch: 'x', pr_url: null } }), true, 'Success terminal');
eq(isTerminalStatus({ Failed: { reason: 'x' } }), true, 'Failed terminal');
eq(isTerminalStatus({ Retrying: { attempt: 2 } }), false, 'Retrying not terminal');
eq(isTerminalStatus(null), false, 'null not terminal');

console.log('\n── parseAgentEvent — all variants ──────────────────────');
assertNotNull(
  parseAgentEvent({
    type: 'task_queued',
    task_id: 'abc',
    goal: 'fix the bug',
    priority: 'High'
  }),
  'task_queued valid'
);
assertNotNull(
  parseAgentEvent({ type: 'task_started', task_id: 'abc', attempt: 1, branch: 'feat/x' }),
  'task_started valid'
);
assertNotNull(
  parseAgentEvent({ type: 'status_changed', task_id: 'abc', status: 'Planning', attempt: 1 }),
  'status_changed valid'
);
assertNotNull(
  parseAgentEvent({
    type: 'log_line',
    task_id: 'abc',
    line: 'cargo check ok',
    level: 'info',
    ts: '2026-05-06T12:00:00Z'
  }),
  'log_line valid'
);
assertNotNull(
  parseAgentEvent({
    type: 'score_updated',
    task_id: 'abc',
    test_pass_rate: 0.95,
    lint_errors: 0,
    diff_lines: 32
  }),
  'score_updated valid'
);
assertNotNull(
  parseAgentEvent({
    type: 'task_completed',
    task_id: 'abc',
    outcome: { Success: { branch: 'feat/x', pr_url: null } },
    total_attempts: 2
  }),
  'task_completed valid'
);
assertNotNull(
  parseAgentEvent({ type: 'task_cancelled', task_id: 'abc' }),
  'task_cancelled valid'
);
assertNotNull(
  parseAgentEvent({
    type: 'pool_stats',
    running: 3,
    queued: 2,
    succeeded: 12,
    failed: 1,
    uptime_secs: 1820
  }),
  'pool_stats valid'
);
assertNotNull(
  parseAgentEvent({
    type: 'turn_metrics',
    task_id: 'abc',
    pressure: 0.42,
    activity: 0.65,
    tokens_per_sec: 52.4,
    cost_usd: 0.0124
  }),
  'turn_metrics valid'
);

console.log('\n── parseAgentEvent — malformed rejected ───────────────');
assertNull(parseAgentEvent({}), 'empty object');
assertNull(parseAgentEvent({ type: 'unknown_kind', task_id: 'abc' }), 'unknown type');
assertNull(parseAgentEvent({ type: 'task_queued', task_id: 'abc' }), 'missing goal');
assertNull(
  parseAgentEvent({ type: 'task_queued', task_id: 'abc', goal: 'g', priority: 'Bogus' }),
  'invalid priority'
);
assertNull(
  parseAgentEvent({ type: 'log_line', task_id: 'abc', line: 'x', level: 'fatal', ts: '2026' }),
  'invalid log level'
);
assertNull(
  parseAgentEvent({ type: 'turn_metrics', task_id: 'abc', pressure: 'high' }),
  'turn_metrics non-numeric pressure'
);
assertNull(
  parseAgentEvent({ type: 'status_changed', task_id: 'abc', status: 'Bogus', attempt: 1 }),
  'status_changed bad status'
);

console.log('\n── turn_metrics clamps to [0, 1] ──────────────────────');
const overflow = parseAgentEvent({
  type: 'turn_metrics',
  task_id: 'abc',
  pressure: 1.5, // intentionally over
  activity: -0.2, // intentionally under
  tokens_per_sec: 100,
  cost_usd: 0.05
});
if (overflow && overflow.type === 'turn_metrics') {
  eq(overflow.pressure, 1, 'pressure clamped to 1');
  eq(overflow.activity, 0, 'activity clamped to 0');
}

console.log('\n── parseSnapshot ──────────────────────────────────────');
assertNotNull(
  parseSnapshot({
    type: 'snapshot',
    tasks: [
      { id: 't1', goal: 'fix bug', status: 'Planning', created_at: '2026-05-06T12:00:00Z' }
    ],
    stats: { running: 1, queued: 0, succeeded: 5, failed: 0, uptime_secs: 60 }
  }),
  'snapshot valid'
);
assertNull(
  parseSnapshot({ type: 'snapshot', tasks: 'nope', stats: {} }),
  'snapshot non-array tasks rejected'
);

console.log('\n── parseWireMessage dispatch ──────────────────────────');
const dispatched = parseWireMessage({
  type: 'log_line',
  task_id: 'abc',
  line: 'hi',
  level: 'info',
  ts: '2026-05-06T12:00:00Z'
});
eq((dispatched as any)?.type, 'log_line', 'dispatch routes log_line to event parser');
const snap = parseWireMessage({
  type: 'snapshot',
  tasks: [],
  stats: { running: 0, queued: 0, succeeded: 0, failed: 0, uptime_secs: 0 }
});
eq((snap as any)?.type, 'snapshot', 'dispatch routes snapshot');

console.log(`\n── Result: ${pass} passed, ${fail} failed ──`);
if (fail > 0) process.exit(1);
