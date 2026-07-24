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
  isTerminalStatus,
  dbStatusToUiStatus
} from './parser';
import type { Phase } from './types';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

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

console.log('\n── verifier_verdict + budget_exceeded ─────────────────');
{
  const v = parseAgentEvent({
    type: 'verifier_verdict',
    task_id: 'abc',
    passed: false,
    gaps: ['no test for empty input'],
    fix_hints: ['add a unit test']
  });
  eq(v?.type, 'verifier_verdict', 'verifier_verdict parses');
  if (v && v.type === 'verifier_verdict') {
    eq(v.passed, false, 'passed preserved');
    eq(v.gaps.length, 1, 'gaps preserved');
  }
  assertNull(
    parseAgentEvent({ type: 'verifier_verdict', task_id: 'abc', passed: 'no', gaps: [], fix_hints: [] }),
    'non-boolean passed rejected'
  );
  assertNull(
    parseAgentEvent({ type: 'verifier_verdict', task_id: 'abc', passed: true, gaps: [1], fix_hints: [] }),
    'non-string gap rejected'
  );

  const bFleet = parseAgentEvent({
    type: 'budget_exceeded',
    task_id: null,
    scope: 'fleet',
    limit_usd: 5,
    burned_usd: 5.4
  });
  eq(bFleet?.type, 'budget_exceeded', 'fleet budget_exceeded parses with null task');
  if (bFleet && bFleet.type === 'budget_exceeded') {
    eq(bFleet.task_id, null, 'null task_id preserved');
    eq(bFleet.scope, 'fleet', 'scope preserved');
  }

  const bTask = parseAgentEvent({
    type: 'budget_exceeded',
    task_id: 'abc',
    scope: 'task',
    limit_usd: 1,
    burned_usd: 1.2
  });
  if (bTask && bTask.type === 'budget_exceeded') {
    eq(bTask.task_id, 'abc', 'task-scoped budget keeps id');
  }
  assertNull(
    parseAgentEvent({ type: 'budget_exceeded', task_id: null, scope: 'galaxy', limit_usd: 1, burned_usd: 2 }),
    'unknown scope rejected'
  );
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
// Verify-1 F6 — the defensive parser must carry per-task cost through, else
// /budget "spent" and the Overview COST column hydrate $0 from the snapshot.
const snapWithCost = parseSnapshot({
  type: 'snapshot',
  tasks: [
    { id: 't1', goal: 'g', status: 'success', created_at: '2026-05-06T12:00:00Z', cost: 0.1234 }
  ],
  stats: { running: 0, queued: 0, succeeded: 1, failed: 0, uptime_secs: 1 }
});
eq((snapWithCost as any)?.tasks?.[0]?.cost, 0.1234, 'snapshot preserves per-task cost (F6)');
const snapNoCost = parseSnapshot({
  type: 'snapshot',
  tasks: [{ id: 't2', goal: 'g', status: 'queued', created_at: '2026-05-06T12:00:00Z' }],
  stats: { running: 0, queued: 1, succeeded: 0, failed: 0, uptime_secs: 1 }
});
eq(
  (snapNoCost as any)?.tasks?.[0]?.cost,
  undefined,
  'snapshot without cost stays undefined (older servers)'
);
// macOS-Web-Parity-5 — same F6 lesson applied to `repo`: a new server field
// is invisible to the client until this whitelist is taught to keep it.
const snapWithRepo = parseSnapshot({
  type: 'snapshot',
  tasks: [
    {
      id: 't1',
      goal: 'g',
      status: 'success',
      created_at: '2026-05-06T12:00:00Z',
      repo: '/Users/dev/lopi'
    }
  ],
  stats: { running: 0, queued: 0, succeeded: 1, failed: 0, uptime_secs: 1 }
});
eq((snapWithRepo as any)?.tasks?.[0]?.repo, '/Users/dev/lopi', 'snapshot preserves per-task repo');
const snapNoRepo = parseSnapshot({
  type: 'snapshot',
  tasks: [{ id: 't2', goal: 'g', status: 'queued', created_at: '2026-05-06T12:00:00Z' }],
  stats: { running: 0, queued: 1, succeeded: 0, failed: 0, uptime_secs: 1 }
});
eq(
  (snapNoRepo as any)?.tasks?.[0]?.repo,
  undefined,
  'snapshot without repo stays undefined (task never started, or older servers)'
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

// ── Golden fixture (G3 — contract parity) ──────────────────────────────────
// The SAME file is decoded by the Rust test (crates/lopi-core/tests/
// agent_event_golden.rs) and the Swift test. All three must agree on fields.
console.log('\n── golden AgentEvent fixture ──────────────────────────');
const here = dirname(fileURLToPath(import.meta.url));
const goldenPath = resolve(here, '../../../crates/lopi-core/tests/fixtures/agent_event_golden.json');
const golden = JSON.parse(readFileSync(goldenPath, 'utf8')) as Record<string, unknown>[];
eq(golden.length, 6, 'golden fixture has six events');

const decoded = golden.map((g) => parseAgentEvent(g));
decoded.forEach((d, i) => assertNotNull(d, `golden[${i}] decodes`));

const TID = '11111111-1111-4111-8111-111111111111';
eq(decoded[0], { type: 'tool_call', task_id: TID, tool: 'Bash', summary: 'ls -la' }, 'golden tool_call');
eq(
  decoded[1],
  { type: 'tool_result', task_id: TID, tool: 'Bash', is_error: false, preview: 'README.md\nnotes.txt' },
  'golden tool_result'
);
eq(
  decoded[2],
  { type: 'token_delta', task_id: TID, output_tokens: 118, input_tokens: 3, cache_read_tokens: 16312 },
  'golden token_delta'
);
eq((decoded[3] as any)?.type, 'api_retry', 'golden api_retry type');
eq((decoded[3] as any)?.utilization, 0.92, 'golden api_retry utilization');
eq((decoded[4] as any)?.session_id, '4fa68a55-05cf-4878-aa2f-d0edaec6b8a6', 'golden cost session_id');
eq((decoded[4] as any)?.num_turns, 3, 'golden cost num_turns');
eq(decoded[5], { type: 'phase', task_id: TID, phase: 'review_ready' }, 'golden phase');

// ── dbStatusToUiStatus (Ops-2 bug #1 regression) ───────────────────────────
// The backend persists canonical LOWERCASE tokens (`TaskStatus::db_status`).
// A fresh page load buckets terminal tasks off the snapshot alone, so every
// one of these must land in its real lifecycle bucket — not fall through to
// `running` the way the old capitalized-only match did.
console.log('\n── dbStatusToUiStatus ─────────────────────────────────');
eq(dbStatusToUiStatus('queued'), 'queued', 'db queued → queued');
eq(dbStatusToUiStatus('running'), 'running', 'db running → running');
eq(dbStatusToUiStatus('planning'), 'running', 'db planning → running');
eq(dbStatusToUiStatus('implementing'), 'running', 'db implementing → running');
eq(dbStatusToUiStatus('testing'), 'running', 'db testing → running');
eq(dbStatusToUiStatus('scoring'), 'running', 'db scoring → running');
eq(dbStatusToUiStatus('retrying'), 'running', 'db retrying → running');
eq(dbStatusToUiStatus('awaiting_plan_approval'), 'running', 'db awaiting → running');
eq(dbStatusToUiStatus('success'), 'completed', 'db success → completed');
eq(dbStatusToUiStatus('failed'), 'failed', 'db failed → failed');
eq(dbStatusToUiStatus('rolled_back'), 'failed', 'db rolled_back → failed');
eq(dbStatusToUiStatus('conflict'), 'failed', 'db conflict → failed');
eq(dbStatusToUiStatus('cancelled'), 'cancelled', 'db cancelled → cancelled');
// Serde enum shapes from live task_completed events map identically.
eq(dbStatusToUiStatus({ Success: { branch: 'b', pr_url: null } }), 'completed', 'enum Success → completed');
eq(dbStatusToUiStatus({ Failed: { reason: 'boom' } }), 'failed', 'enum Failed → failed');
eq(dbStatusToUiStatus('RolledBack'), 'failed', 'enum RolledBack → failed');
eq(dbStatusToUiStatus('Queued'), 'queued', 'enum Queued → queued');
// The compound legacy artifact that broke bucketing must NOT read as running.
eq(dbStatusToUiStatus('unknown'), 'failed', 'legacy unknown → failed');

// A snapshot of terminal DB rows buckets correctly end-to-end: parse then map.
const termSnap = parseSnapshot({
  type: 'snapshot',
  tasks: [
    { id: 's', goal: 'ok', status: 'success', created_at: '2026-05-06T12:00:00Z' },
    { id: 'f', goal: 'no', status: 'failed', created_at: '2026-05-06T12:00:00Z' },
    { id: 'q', goal: 'wait', status: 'queued', created_at: '2026-05-06T12:00:00Z' }
  ],
  stats: { running: 0, queued: 1, succeeded: 1, failed: 1, uptime_secs: 9 }
});
assertNotNull(termSnap, 'terminal snapshot parses');
const mapped = (termSnap as any).tasks.map((t: any) => dbStatusToUiStatus(t.status));
eq(mapped, ['completed', 'failed', 'queued'], 'snapshot terminal rows bucket off canonical strings');

console.log(`\n── Result: ${pass} passed, ${fail} failed ──`);
if (fail > 0) process.exit(1);
