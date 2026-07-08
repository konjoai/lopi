/**
 * Pre-flight kill tests for stores/stack.ts — run with
 * `npx tsx src/lib/stores/stack.test.ts`. Pure ops + the composer grammar
 * parser only; no Svelte, no browser, no backend.
 */
import {
  addCard,
  removeCard,
  duplicateCard,
  reorderCard,
  moveCardBeforeOrAfter,
  insertCardAt,
  patchCard,
  toggleEval,
  applySuite,
  stepMaxIterations,
  maxIterationsLabel,
  guardActive,
  evalActive,
  configActive,
  buildCronString,
  cronHuman,
  computeNextRuns,
  cardToTaskPayload,
  applyToPaneCards,
  insertIntoPane,
  parseComposerInput,
  suggestPreset,
  buildCard,
  defaultCron,
  defaultGuardrails,
  BASELINE_EVAL,
  type StackCard,
  type StackPaneState
} from './stack';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

function card(id: string, goal = id): StackCard {
  return { ...buildCard(`"${goal}"`), id };
}

function pane(key: string, cards: StackCard[] = []): StackPaneState {
  return { key, title: key, cards };
}

// ── add — prepends ────────────────────────────────────────────────────────────
eq(addCard([], card('a')).map((c) => c.id), ['a'], 'add into empty stack');
eq(addCard([card('a')], card('b')).map((c) => c.id), ['b', 'a'], 'add prepends to top');

// ── remove — drops by id ──────────────────────────────────────────────────────
eq(
  removeCard([card('a'), card('b')], 'a').map((c) => c.id),
  ['b'],
  'remove drops the matching card'
);
eq(
  removeCard([card('a')], 'missing').map((c) => c.id),
  ['a'],
  'remove is a no-op for an unknown id'
);
eq(removeCard([card('a')], 'a'), [], 'remove down to empty stack');

// ── duplicate — clones in place, resets run state ─────────────────────────────
{
  const running: StackCard = { ...card('a'), status: 'running', iteration: { current: 1, total: 3 }, taskId: 't1' };
  const dup = duplicateCard([running, card('b')], 'a');
  eq(dup.length, 3, 'duplicate grows the stack by one');
  eq(dup[0].id, 'a', 'duplicate keeps the original at its position');
  eq(dup[1].goal, 'a', 'duplicate clones the original goal');
  ok(dup[1].id !== 'a', 'duplicate gets a fresh id');
  eqIs(dup[1].status, 'idle', 'duplicate resets status to idle');
  eqIs(dup[1].iteration, undefined, 'duplicate clears iteration progress');
  eqIs(dup[1].taskId, undefined, 'duplicate clears taskId');
  eq(dup[2].id, 'b', 'duplicate does not disturb later cards');
}
eq(duplicateCard([card('a')], 'missing').length, 1, 'duplicate is a no-op for an unknown id');

// ── reorder — moves by index ──────────────────────────────────────────────────
eq(
  reorderCard([card('a'), card('b'), card('c')], 0, 2).map((c) => c.id),
  ['b', 'c', 'a'],
  'reorder moves the card to the target index'
);
eq(
  reorderCard([card('a'), card('b')], 0, 99).map((c) => c.id),
  ['a', 'b'],
  'reorder out-of-range `to` is a no-op'
);
eq(
  reorderCard([card('a'), card('b')], -1, 1).map((c) => c.id),
  ['a', 'b'],
  'reorder out-of-range `from` is a no-op'
);

// ── drag-and-drop relative reorder ────────────────────────────────────────────
{
  const cards = [card('a'), card('b'), card('c'), card('d')];
  eq(
    moveCardBeforeOrAfter(cards, 0, 2, true).map((c) => c.id),
    ['b', 'a', 'c', 'd'],
    'dragging a earlier card to just before a later target lands right before it'
  );
  eq(
    moveCardBeforeOrAfter(cards, 0, 2, false).map((c) => c.id),
    ['b', 'c', 'a', 'd'],
    'dragging a earlier card to just after a later target lands right after it'
  );
  eq(
    moveCardBeforeOrAfter(cards, 3, 1, true).map((c) => c.id),
    ['a', 'd', 'b', 'c'],
    'dragging a later card to just before an earlier target lands right before it'
  );
  eq(
    moveCardBeforeOrAfter(cards, 3, 1, false).map((c) => c.id),
    ['a', 'b', 'd', 'c'],
    'dragging a later card to just after an earlier target lands right after it'
  );
  eq(moveCardBeforeOrAfter(cards, 1, 1, true), cards, 'dropping a card onto itself is a no-op');
}

// ── insert — at index ─────────────────────────────────────────────────────────
eq(
  insertCardAt([card('a'), card('c')], 1, card('b')).map((c) => c.id),
  ['a', 'b', 'c'],
  'insert lands at the given index'
);
eq(
  insertCardAt([], 5, card('a')).map((c) => c.id),
  ['a'],
  'insert clamps an out-of-range index into an empty stack'
);

// ── patch — shallow merge by id ────────────────────────────────────────────────
{
  const patched = patchCard([card('a'), card('b')], 'a', { goal: 'renamed' });
  eqIs(patched[0].goal, 'renamed', 'patch merges the given fields');
  eqIs(patched[1].goal, 'b', 'patch leaves other cards untouched');
}
eq(patchCard([card('a')], 'missing', { goal: 'x' })[0].goal, 'a', 'patch is a no-op for an unknown id');

// ── empty stack ⇒ callers render EmptyState (nothing to assert on the store
// itself beyond "an empty array is a valid, terminal state") ─────────────────
eq(removeCard([card('a')], 'a'), [], 'stack can reach empty');

// ── composer grammar parser ───────────────────────────────────────────────────
eq(
  parseComposerInput(':optimize "x" @squish x3'),
  { alias: 'optimize', goal: 'x', repo: 'squish', loopN: 3 },
  'alias + quoted goal + repo + loop count all parse'
);
eq(
  parseComposerInput('"fix the bug"'),
  { alias: null, goal: 'fix the bug', repo: null, loopN: null },
  'a quoted literal with no alias parses as goal-only'
);
eq(
  parseComposerInput('fix the bug'),
  { alias: null, goal: 'fix the bug', repo: null, loopN: null },
  'an unquoted literal parses as goal-only'
);
eq(
  parseComposerInput(':research "paged attention"'),
  { alias: 'research', goal: 'paged attention', repo: null, loopN: null },
  'alias without repo/loop still parses'
);

// ── keyword suggestion — highlight only, never attached by the parser ────────
eqIs(suggestPreset('add a gate'), 'implement', 'keyword match suggests implement');
eqIs(suggestPreset('optimize the dequant kernel'), 'optimize', 'keyword match suggests optimize');
eqIs(suggestPreset('draft a changelog entry'), null, 'no keyword match suggests nothing');

// ── buildCard — preset attachment through either door ─────────────────────────
{
  const viaAlias = buildCard(':implement "add verifier gate"');
  eqIs(viaAlias.preset, 'implement', 'buildCard attaches preset via recognized alias');
  eq(viaAlias.evals.length, 6, 'alias-attached preset carries its full eval suite');
  ok(!viaAlias.literal, 'alias-built card is not literal');
}
{
  const viaChip = buildCard('improve the dequant kernel', 'optimize');
  eqIs(viaChip.preset, 'optimize', 'buildCard attaches preset via explicit chip/grid selection');
  eq(viaChip.evals.length, 4, 'chip-attached preset carries its full eval suite');
}
{
  const literal = buildCard('draft weekly changelog digest');
  eqIs(literal.preset, undefined, 'no alias, no explicit preset ⇒ no preset attached');
  ok(literal.literal, 'plain text builds a literal card');
  eq(literal.evals, [{ name: 'execution ok', tier: 'base' }], 'literal card carries only the baseline eval');
}
{
  const withLoop = buildCard(':optimize "x" @squish x3');
  eqIs(withLoop.maxIterations, 3, 'xN grammar seeds maxIterations');
  eqIs(withLoop.config.repo, 'squish', '@repo grammar seeds config.repo');
}
{
  const plain = buildCard('a plain goal');
  eqIs(plain.maxIterations, 25, 'no xN ⇒ maxIterations defaults to the backend default (25)');
  eqIs(plain.scheduled, false, 'fresh card is not scheduled');
  eqIs(plain.status, 'idle', 'fresh card starts idle');
}
{
  const a = buildCard('a');
  const b = buildCard('b');
  ok(a.cron !== b.cron, 'each card gets its own cron object, not a shared reference');
  ok(a.guardrails !== b.guardrails, 'each card gets its own guardrails object, not a shared reference');
}

// ── eval-set ops ───────────────────────────────────────────────────────────────
{
  const toggled = toggleEval([BASELINE_EVAL], 'unit');
  eq(toggled.map((e) => e.name), ['execution ok', 'unit'], 'toggleEval turns an eval on');
  const toggledOff = toggleEval(toggled, 'unit');
  eq(toggledOff.map((e) => e.name), ['execution ok'], 'toggleEval turns it back off');
  eq(toggleEval([BASELINE_EVAL], 'execution ok'), [BASELINE_EVAL], 'toggleEval never turns off the baseline');
  eq(toggleEval([BASELINE_EVAL], 'not-a-real-eval'), [BASELINE_EVAL], 'toggleEval ignores unknown names');
}
{
  const suited = applySuite([BASELINE_EVAL], ['vuln scan', 'adversarial']);
  eq(suited.map((e) => e.name), ['execution ok', 'vuln scan', 'adversarial'], 'applySuite adds every named eval');
  const again = applySuite(suited, ['vuln scan']);
  eq(again.map((e) => e.name), ['execution ok', 'vuln scan', 'adversarial'], 'applySuite never duplicates an already-on eval');
}

// ── iteration stepper — floor 2, wraps to infinite (0) ────────────────────────
eqIs(stepMaxIterations(25, 1), 26, 'stepping up increments normally');
eqIs(stepMaxIterations(25, -1), 24, 'stepping down decrements normally');
eqIs(stepMaxIterations(2, -1), 0, 'stepping below the floor wraps to infinite');
eqIs(stepMaxIterations(3, -2), 0, 'a multi-step decrement below the floor also wraps to infinite');
eqIs(stepMaxIterations(0, 1), 2, 'stepping up from infinite lands on the floor, not 1');
eqIs(stepMaxIterations(0, -1), 0, 'stepping down from infinite stays infinite');
eqIs(maxIterationsLabel(0), '∞', 'label renders the infinite sentinel as ∞');
eqIs(maxIterationsLabel(5), '5', 'label renders a finite ceiling as its number');

// ── active-state predicates ────────────────────────────────────────────────────
eqIs(guardActive(defaultGuardrails()), false, 'fresh guardrails are inactive');
eqIs(guardActive({ ...defaultGuardrails(), gate: true }), true, 'gate alone activates guardrails');
eqIs(guardActive({ ...defaultGuardrails(), until: true }), true, 'until alone activates guardrails');
eqIs(evalActive(buildCard('x')), false, 'baseline-only card has inactive evals');
eqIs(evalActive(buildCard(':implement "x"')), true, 'preset-attached card has active evals');
{
  const defaults = { model: 'm', effort: 'e', repo: 'r', branch: 'b', autonomy: 'a' };
  const plain = buildCard('x');
  eqIs(configActive(plain, defaults), false, 'no overrides ⇒ config inactive');
  const overridden = { ...plain, config: { model: 'other' } };
  eqIs(configActive(overridden, defaults), true, 'a single overridden field activates config');
}

// ── cron helpers ───────────────────────────────────────────────────────────────
eqIs(buildCronString(defaultCron()), '0 2 * * *', 'default cron (daily 2am) builds the expected 5-field string');
eqIs(buildCronString({ ...defaultCron(), freq: 'every minute' }), '* * * * *', 'every-minute cron');
eqIs(buildCronString({ ...defaultCron(), freq: 'hourly', min: 15 }), '15 * * * *', 'hourly cron uses the minute field');
eqIs(
  buildCronString({ ...defaultCron(), freq: 'weekly', dow: 'Fri', hour12: 6, ampm: 'PM', min: 30 }),
  '30 18 * * 5',
  'weekly cron resolves 12h PM time and weekday number'
);
eqIs(buildCronString({ ...defaultCron(), freq: 'custom', raw: '*/5 * * * *' }), '*/5 * * * *', 'custom cron passes raw through');
eqIs(cronHuman(defaultCron()), 'every day at 2:00 AM', 'human echo for the default cron');
eqIs(cronHuman({ ...defaultCron(), freq: 'hourly', min: 5 }), 'every hour at :05', 'human echo pads minutes');

// ── computeNextRuns — cron field matcher ──────────────────────────────────────
{
  const from = new Date('2026-07-08T10:00:00');
  const runs = computeNextRuns('0 2 * * *', from, 3);
  eq(runs.length, 3, 'daily cron finds 3 upcoming runs within the search window');
  eqIs(runs[0].getHours(), 2, 'each run lands on the specified hour');
  eqIs(runs[0].getMinutes(), 0, 'each run lands on the specified minute');
  eqIs(runs[0].getDate(), 9, 'the first run after 10am is the next calendar day at 2am');
  eqIs(runs[1].getDate(), 10, 'runs are one day apart for a daily cadence');
}
{
  const from = new Date('2026-07-08T10:00:00');
  const runs = computeNextRuns('* * * * *', from, 2);
  eq(runs.length, 2, 'every-minute cron finds runs immediately');
  eqIs(runs[1].getTime() - runs[0].getTime(), 60_000, 'every-minute runs are exactly 60s apart');
}
eq(computeNextRuns('not a cron', new Date(), 3), [], 'a malformed cron expression yields no results rather than throwing');
{
  const from = new Date('2026-07-08T10:00:00'); // a Wednesday
  const runs = computeNextRuns('0 6 * * 5', from, 1); // Friday 6am
  eq(runs.length, 1, 'weekly cron with a day-of-week field finds the next matching weekday');
  eqIs(runs[0].getDay(), 5, 'the matched run falls on the requested weekday');
}

// ── backend round-trip (WIRED fields → CreateTaskOptions shape) ───────────────
{
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const plain = buildCard('do the thing');
  const payload = cardToTaskPayload(plain, defaults);
  eqIs(payload.goal, 'do the thing', 'payload carries the goal verbatim');
  eqIs(payload.repo, 'konjoai/lopi', 'no repo override ⇒ payload falls back to the pane default');
  eqIs(payload.options.model, 'sonnet', 'no model override ⇒ payload falls back to the pane default');
  eqIs(payload.options.max_iterations, 25, 'payload carries maxIterations as max_iterations');
  eqIs(payload.options.on_fail, 'stop', 'payload carries the default on_fail policy');
  eqIs(payload.options.gate, undefined, 'gate omitted when the guardrail toggle is off');
}
{
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const guarded = buildCard('do the thing');
  guarded.config.repo = 'squish';
  guarded.guardrails = { gate: true, gateCmd: './kill_test.sh', until: true, untilCmd: 'cargo test', onFail: 'backoff', budget: '200k' };
  const payload = cardToTaskPayload(guarded, defaults);
  eqIs(payload.repo, 'squish', 'a config.repo override wins over the pane default');
  eqIs(payload.options.gate, './kill_test.sh', 'enabled gate carries its command');
  eqIs(payload.options.until, 'cargo test', 'enabled until carries its command');
  eqIs(payload.options.on_fail, 'backoff', 'payload carries the chosen on_fail policy');
}
{
  // `until` off is never exercised above (that test only checks `gate`'s
  // off-state) — a regression that swapped the two guardrail fields would
  // slip past it.
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const untilOff = buildCard('x');
  eqIs(cardToTaskPayload(untilOff, defaults).options.until, undefined, 'until is omitted when its guardrail toggle is off');
}

// ── V&V: table-driven WIRED round-trip (§C) — one non-default value per WIRED
// field, asserting it lands correctly in CreateTaskOptions and that no WIRED
// field is silently dropped or renamed. `maxIterations: 0` (the ∞ sentinel)
// gets its own row since it's the one value JS falsy-coercion bugs love to
// eat (`0 ?? default` is fine; `0 || default` would silently swap it out —
// this table would catch that class of regression).
{
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  type Row = { name: string; apply: (c: StackCard) => void; field: string; expected: unknown };
  const rows: Row[] = [
    { name: 'model override', apply: (c) => (c.config.model = 'claude-opus-4-8'), field: 'model', expected: 'claude-opus-4-8' },
    { name: 'effort override', apply: (c) => (c.config.effort = 'high'), field: 'effort', expected: 'high' },
    { name: 'repo override', apply: (c) => (c.config.repo = 'konjoai/squish'), field: 'repo', expected: 'konjoai/squish' },
    { name: 'gate on', apply: (c) => (c.guardrails = { ...c.guardrails, gate: true, gateCmd: './gate.sh' }), field: 'gate', expected: './gate.sh' },
    { name: 'until on', apply: (c) => (c.guardrails = { ...c.guardrails, until: true, untilCmd: 'exit 0' }), field: 'until', expected: 'exit 0' },
    { name: 'on_fail continue', apply: (c) => (c.guardrails = { ...c.guardrails, onFail: 'continue' }), field: 'on_fail', expected: 'continue' },
    { name: 'on_fail backoff', apply: (c) => (c.guardrails = { ...c.guardrails, onFail: 'backoff' }), field: 'on_fail', expected: 'backoff' },
    { name: 'maxIterations finite override (7)', apply: (c) => (c.maxIterations = 7), field: 'max_iterations', expected: 7 },
    { name: 'maxIterations infinite sentinel (0)', apply: (c) => (c.maxIterations = 0), field: 'max_iterations', expected: 0 }
  ];
  for (const row of rows) {
    const c = buildCard('table-driven row');
    row.apply(c);
    const payload = cardToTaskPayload(c, defaults);
    const actual =
      row.field === 'repo'
        ? payload.repo
        : (payload.options as unknown as Record<string, unknown>)[row.field];
    eqIs(actual, row.expected, `WIRED round-trip: ${row.name} → options.${row.field}`);
  }
  // Key-name completeness: a field silently renamed (e.g. `onFail` leaking
  // through unconverted instead of `on_fail`) would pass every value-level
  // assertion above yet still be wrong — assert the actual key set.
  const fullyGuarded = buildCard('x');
  fullyGuarded.guardrails = { gate: true, gateCmd: 'g', until: true, untilCmd: 'u', onFail: 'stop', budget: 'auto' };
  const keys = Object.keys(cardToTaskPayload(fullyGuarded, defaults).options).sort();
  eq(keys, ['effort', 'gate', 'max_iterations', 'model', 'on_fail', 'until'], 'options carries exactly the expected WIRED key names — no silent rename/drop');
}

// ── V&V: schedule cron never "snaps" a custom expression to a preset (§C) ────
// `freq` is explicit UI state set by the popover (never inferred from
// `raw`), so a raw cron that happens to numerically match a preset's shape
// must still read as "custom cron" — there is no reverse-detection logic to
// regress. `buildCronString`'s existing "custom cron passes raw through"
// test covers the forward (preset→string) direction; this covers the
// human-echo side of "stays custom, never snaps."
eqIs(
  cronHuman({ ...defaultCron(), freq: 'custom', raw: '0 2 * * *' }),
  'custom cron',
  'a custom-flagged cron that happens to match the daily preset\'s shape still echoes as "custom cron", never snaps to "every day at..."'
);

// ── pane-keyed dispatch (the pre-flight's stack.insert(stackKey, index, loop)) ─
{
  const state = [pane('s1', [card('a')]), pane('s2', [card('x')])];
  const inserted = insertIntoPane(state, 's1', 1, card('b'));
  eq(inserted[0].cards.map((c) => c.id), ['a', 'b'], 'insertIntoPane inserts into the named pane at the given index');
  eq(inserted[1].cards.map((c) => c.id), ['x'], 'insertIntoPane leaves the other pane untouched');
  ok(inserted[1] === state[1], 'the untouched pane keeps its object identity (no wasted re-render)');
}
{
  const state = [pane('s1', [card('a')])];
  const untouched = insertIntoPane(state, 'missing', 0, card('b'));
  ok(untouched === state, 'insertIntoPane into an unknown key is a total no-op');
}
{
  const state = [pane('s1', [card('a'), card('b')])];
  const removed = applyToPaneCards(state, 's1', (cards) => cards.filter((c) => c.id !== 'a'));
  eq(removed[0].cards.map((c) => c.id), ['b'], 'applyToPaneCards composes with any pure card-list op');
}

// ── V&V: reorder is provably within-pane only (StackConnector §B) ────────────
// `reorderInPaneRelative`/`reorderInPane` (the writable-store wrappers) both
// compose `applyToPaneCards(state, key, ...)` with a single `key` — there is
// no exported op that even accepts two different pane keys, so a cross-pane
// reorder is not just untested, it's inexpressible through this API. These
// tests exercise that same composition (`applyToPaneCards` + `reorderCard`/
// `moveCardBeforeOrAfter`, exactly what the store wrappers call) end to end,
// closing the "pane keys stable across ops" gap the V&V audit flagged.
{
  const state = [pane('s1', [card('a'), card('b'), card('c')]), pane('s2', [card('x'), card('y')])];
  const reordered = applyToPaneCards(state, 's1', (cards) => reorderCard(cards, 0, 2));
  eq(reordered[0].cards.map((c) => c.id), ['b', 'c', 'a'], 'reorder via applyToPaneCards affects only the named pane');
  eq(reordered[1].cards.map((c) => c.id), ['x', 'y'], 'reorder on s1 leaves s2 completely untouched');
  ok(reordered[1] === state[1], 's2 keeps its object identity across an s1-only reorder');
}
{
  const state = [pane('s1', [card('a'), card('b')]), pane('s2', [card('x'), card('y'), card('z')])];
  const dragged = applyToPaneCards(state, 's2', (cards) => moveCardBeforeOrAfter(cards, 2, 0, true));
  eq(dragged[1].cards.map((c) => c.id), ['z', 'x', 'y'], 'drag-relative reorder via applyToPaneCards affects only the named pane');
  eq(dragged[0].cards.map((c) => c.id), ['a', 'b'], 'drag-relative reorder on s2 leaves s1 completely untouched');
  ok(dragged[0] === state[0], 's1 keeps its object identity across an s2-only drag reorder');
}

namedSummary('stack');
