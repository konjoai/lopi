/**
 * Badge helper tests — runs as a standalone Node script.
 * Usage: `npx tsx src/lib/components/ui/badges.test.ts` from web/
 */
import { statusColor, levelColor, statusLabel } from './badges';

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

// ── statusLabel ───────────────────────────────────────────────────────────────
eq(statusLabel('Queued'), 'Queued', 'unit variant passes through');
eq(statusLabel({ Failed: { reason: 'x' } }), 'Failed', 'struct variant uses key');
eq(
  statusLabel({ Success: { branch: 'b', pr_url: null } }),
  'Success',
  'success variant uses key'
);
eq(statusLabel('{"Retrying":{"attempt":2}}'), 'Retrying', 'pre-rendered JSON string parses');
eq(statusLabel('{not json'), '{not json', 'malformed JSON string passes through');
eq(statusLabel(null), 'Unknown', 'null is Unknown');
eq(statusLabel(42), 'Unknown', 'number is Unknown');
eq(statusLabel({}), 'Unknown', 'empty object is Unknown');

// ── statusColor ───────────────────────────────────────────────────────────────
eq(statusColor('Success'), 'var(--konjo-jade)', 'success is jade');
eq(statusColor('Failed'), 'var(--konjo-rose)', 'failed is rose');
eq(statusColor('RolledBack'), 'var(--konjo-rose)', 'rolled back is rose');
eq(statusColor('Queued'), 'var(--konjo-sun)', 'queued is sun');
eq(statusColor('Retrying'), 'var(--konjo-flame)', 'retrying is flame');
eq(statusColor('Implementing'), 'var(--konjo-ice)', 'in-flight is ice');

// ── levelColor ────────────────────────────────────────────────────────────────
eq(levelColor('error'), 'var(--konjo-rose)', 'error is rose');
eq(levelColor('warn'), 'var(--konjo-flame)', 'warn is flame');
eq(levelColor('info'), 'var(--konjo-ice)', 'info is ice');
eq(levelColor('debug'), 'rgba(245,245,245,0.4)', 'debug is dim paper');

console.log(`\n── Result: ${pass} passed, ${fail} failed ──`);
if (fail > 0) process.exit(1);
