/**
 * Shared assertion harness for this repo's standalone `*.test.ts` files —
 * each runs as a plain Node script (`npx tsx <file>.test.ts`), not through a
 * test runner, so this is the one place the pass/fail bookkeeping is
 * written rather than each file re-declaring its own counters.
 */
let pass = 0;
let fail = 0;

/** Lowest-level primitive: record one pass/fail outcome, logging `failMsg`
 * only on failure. Every assertion helper below (and any file-local
 * comparator, e.g. an approx-equal `close()`) is built on this so the
 * counters are only ever mutated here. */
export function record(passed: boolean, failMsg: string): void {
  if (passed) {
    pass++;
  } else {
    fail++;
    console.error(`✗ ${failMsg}`);
  }
}

/** Deep-equal assertion (structural, via `JSON.stringify`) — for objects/arrays. */
export function eq(actual: unknown, expected: unknown, name: string): void {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  record(a === e, `${name}: expected ${e}, got ${a}`);
}

/** Identity assertion (`Object.is`) — for primitives, where `JSON.stringify`
 * would obscure the actual failing value (e.g. distinguishing `undefined`
 * from a stringified `"undefined"`). */
export function eqIs(actual: unknown, expected: unknown, name: string): void {
  record(Object.is(actual, expected), `${name}: expected ${expected}, got ${actual}`);
}

/** Boolean assertion — records a pass/fail with no expected/actual diff. */
export function ok(cond: boolean, name: string): void {
  record(cond, name);
}

/** Print the final tally and exit(1) if anything failed. Call once, at the
 * end of each file. */
export function summary(): void {
  console.log(`\n── Result: ${pass} passed, ${fail} failed ──`);
  if (fail > 0) process.exit(1);
}

/** Same as {@link summary}, but with a file-specific label instead of the
 * generic "Result" banner (several test files name themselves in the tally). */
export function namedSummary(label: string): void {
  console.log(`\n${label}: ${pass} passed, ${fail} failed`);
  if (fail > 0) process.exit(1);
}
