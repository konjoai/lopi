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
  stepCardIterations,
  cardIterationsLabel,
  DEFAULT_MAX_ITERATIONS,
  guardActive,
  evalActive,
  configActive,
  buildCronString,
  cronHuman,
  computeNextRuns,
  cardToTaskPayload,
  cardToTaskPayloadForRunOnce,
  paneSubmitPayload,
  budgetToTokens,
  resolvePresetAlias,
  aliasAutocomplete,
  evalsToAcceptance,
  executionOrder,
  dryRunStack,
  bumpInOrder,
  applyToPaneCards,
  insertIntoPane,
  parseComposerInput,
  suggestPreset,
  buildCard,
  defaultCron,
  defaultMaxx,
  maxxSummary,
  defaultGuardrails,
  defaultStackConfig,
  duplicateStack,
  loadStackCardsInto,
  paneIsBare,
  makeBlankStack,
  addStack,
  reorderStacks,
  moveStackBeforeOrAfter,
  deleteStack,
  stackGuardActive,
  stackEvalActive,
  stackDefaultsActive,
  stackDefaultsSummary,
  stackGoalActive,
  stackPursuesGoal,
  stackGoalSummary,
  defaultStackGoal,
  perLoopScheduleGoverned,
  makeDraft,
  draftIsHot,
  finalizeDraft,
  applyPreset,
  applyPromptTemplate,
  applyStackTemplate,
  promptTemplateFromCard,
  stackTemplateFromCards,
  PRESET_CATALOG,
  BASELINE_EVAL,
  adoptRepoDefaultIfUnset,
  CARD_COMMANDS,
  STACK_COMMANDS,
  commandAutocomplete,
  commandValueAutocomplete,
  detectPendingCommand,
  evalSuiteOptions,
  type PromptTemplate,
  type StackTemplate,
  type StackCard,
  type StackPaneState,
  type StackConfig
} from './stack';
import { DEFAULT_STACK_DEFAULTS } from './stackDefaults';
import { AUTO_MODEL, MODEL_OPTIONS, EFFORT_OPTIONS } from './options';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

function card(id: string, goal = id): StackCard {
  return { ...buildCard(`"${goal}"`), id };
}

function pane(key: string, cards: StackCard[] = [], config: Partial<StackConfig> = {}): StackPaneState {
  return { key, title: key, cards, config: { ...defaultStackConfig(), ...config }, draft: makeDraft() };
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

// ── MAXX — every card gets its own object, defaults match the locked design ──
{
  const a = buildCard('"x"');
  const b = buildCard('"y"');
  ok(a.maxx !== b.maxx, 'every card gets its own maxx object, not a shared reference');
  eqIs(a.maxx.enabled, false, 'maxx starts disabled');
  eq(a.maxx.quietHours, [23, 7], 'default quiet hours match the locked popover design (11PM-7AM)');
  eqIs(a.maxx.headroomGate, true, 'headroom gate defaults on');
  eq(a.maxx.windows, ['five_hour', 'seven_day'], 'both windows checked by default');
}

// ── MAXX — duplicate never shares the original's backend entry ───────────────
{
  const on: StackCard = { ...card('a'), maxx: { ...defaultMaxx(), enabled: true }, maxxEntryId: 'entry-1' };
  const dup = duplicateCard([on], 'a');
  eqIs(dup[1].maxxEntryId, undefined, 'duplicate clears the backend entry id');
  eqIs(dup[1].maxx.enabled, false, 'duplicate resets maxx to disabled — no entry to back it');
  eqIs(dup[0].maxxEntryId, 'entry-1', 'the original keeps its own entry id');
}

// ── MAXX — summary text ───────────────────────────────────────────────────────
eq(
  maxxSummary({ ...card('a'), maxx: { enabled: true, quietHours: [23, 7], headroomGate: true, windows: [] } }),
  'quiet hours + headroom',
  'summary lists every active condition'
);
eq(
  maxxSummary({ ...card('a'), maxx: { enabled: true, quietHours: [23, 7], headroomGate: false, windows: [] } }),
  'quiet hours',
  'summary drops headroom when its gate is off'
);

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
  eqIs(plain.maxIterations, 0, 'no xN ⇒ maxIterations defaults to off (0) — a fresh card does not loop');
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

// ── iteration stepper — three states: off (1), floor 2.., infinite (0) ────────
eqIs(stepMaxIterations(25, 1), 26, 'stepping up increments normally');
eqIs(stepMaxIterations(25, -1), 24, 'stepping down decrements normally');
eqIs(stepMaxIterations(2, -1), 1, 'stepping below the floor lands on off (1)');
eqIs(stepMaxIterations(3, -2), 1, 'a multi-step decrement below the floor also lands on off (1)');
eqIs(stepMaxIterations(1, -1), 0, 'stepping down from off wraps to infinite');
eqIs(stepMaxIterations(1, 1), 2, 'stepping up from off reaches the floor');
eqIs(stepMaxIterations(0, 1), 1, 'stepping up from infinite lands on off, not the floor');
eqIs(stepMaxIterations(0, -1), 0, 'stepping down from infinite stays infinite');
eqIs(maxIterationsLabel(0), '∞', 'label renders the infinite sentinel as ∞');
eqIs(maxIterationsLabel(1), 'off', 'label renders a single run with no repeat as off');
eqIs(maxIterationsLabel(5), '5', 'label renders a finite ceiling as its number');

// ── card iteration stepper — floors at 0 = "off", never wraps to infinite ─────
eqIs(stepCardIterations(0, 1), 1, 'stepping up from off lands on 1');
eqIs(stepCardIterations(1, -1), 0, 'stepping down from 1 reaches off (0)');
eqIs(stepCardIterations(0, -1), 0, 'stepping down from off stays off — never wraps to infinite');
eqIs(stepCardIterations(3, 2), 5, 'stepping up increments normally');
eqIs(cardIterationsLabel(0), 'off', 'card label renders 0 as off');
eqIs(cardIterationsLabel(4), '4', 'card label renders a finite ceiling as its number');
eqIs(DEFAULT_MAX_ITERATIONS, 0, 'a fresh card defaults to off (0), not looping');

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
  eqIs(payload.options.max_iterations, 1, 'a fresh (off) card sends a single pass — off (0) maps to max_iterations 1 on the wire');
  eqIs(payload.options.on_fail, 'stop', 'payload carries the default on_fail policy');
  eqIs(payload.options.gate, undefined, 'gate omitted when the guardrail toggle is off');
}
{
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const guarded = buildCard('do the thing');
  guarded.config.repo = 'squish';
  guarded.guardrails = { gate: true, gateCmd: './kill_test.sh', until: true, untilCmd: 'cargo test', onFail: 'backoff', budget: '200k' };
  const payload = cardToTaskPayload(guarded, defaults);

  // A3 — the '200k' budget preset compiles to the metered budget_tokens.
  eqIs(payload.options.budget_tokens, 200_000, "budget '200k' → budget_tokens 200000");
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
// ── A3 — budget preset → metered budget_tokens (only real caps flow) ──────────
eqIs(budgetToTokens('200k'), 200_000, "budget '200k' resolves to a 200000-token cap");
eqIs(budgetToTokens('auto'), undefined, "budget 'auto' inherits — no hard cap in the payload");
eqIs(budgetToTokens('none'), undefined, "budget 'none' is uncapped — no hard cap in the payload");
{
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const inheritCard = buildCard('x'); // defaultGuardrails ⇒ budget: 'auto'
  eqIs(
    cardToTaskPayload(inheritCard, defaults).options.budget_tokens,
    undefined,
    'the inherit budget preset omits budget_tokens (no inert enforced-limit claim)'
  );
}
// ── A3 — the `:ratchet` → `:gain` rename keeps the legacy alias resolving ─────
eqIs(resolvePresetAlias('gain'), 'gain', "the `:gain` alias resolves to the gain preset");
eqIs(resolvePresetAlias('ratchet'), 'gain', "the legacy `:ratchet` alias still resolves to `gain`");
eqIs(resolvePresetAlias('nonsense'), null, 'an unknown alias resolves to null');
eqIs(buildCard(':ratchet "self improve"').preset, 'gain', 'a `:ratchet` composer string builds a gain-preset card');

// ── alias autocomplete — filters PRESET_KEYS while the goal is a bare :token ──
{
  // `:re` is deliberately ambiguous (`research` and `report` both start with
  // it) — `:res`/`:rep` is where the two diverge and each becomes unique.
  eqIs(aliasAutocomplete(':re').length, 2, 'a shared prefix returns every matching preset, research before report (catalog order)');
  eqIs(aliasAutocomplete(':re')[0].alias, ':research', 'research sorts first — PRESET_KEYS declaration order');
  eqIs(aliasAutocomplete(':res').length, 1, 'a unique prefix returns exactly one match');
  eqIs(aliasAutocomplete(':res')[0].alias, ':research', 'the match carries the full canonical alias');
  eqIs(aliasAutocomplete(':res')[0].label, 'research', 'the match carries the preset label');
  eqIs(aliasAutocomplete(':res')[0].hint, 'explore & investigate — judge-reviewed', 'the match carries the one-line description');
  eqIs(aliasAutocomplete(':').length, 8, 'a bare colon matches every built-in preset');
  eqIs(aliasAutocomplete(':RES')[0].alias, ':research', 'matching is case-insensitive');
  eqIs(aliasAutocomplete(':nope').length, 0, 'no preset starts with an unknown prefix');
  eqIs(aliasAutocomplete(':ratchet').length, 0, 'the legacy `:ratchet` alias is never suggested, only `:gain` is');
  eqIs(aliasAutocomplete('').length, 0, 'an empty goal (no colon yet) suggests nothing');
  eqIs(aliasAutocomplete('research').length, 0, 'plain text with no leading colon suggests nothing');
  eqIs(aliasAutocomplete(':research and more').length, 0, 'once a space follows the token, the goal text has moved on — no suggestions');
  eqIs(aliasAutocomplete(':research ').length, 0, 'a trailing space after a completed alias also closes the list');
}

// ── inline `/command` autocomplete — level 1 (command names) + level 2
//    (`/command/value`), mirroring `@repo`'s trailing-word grammar ───────────
{
  eqIs(commandAutocomplete('/mo', CARD_COMMANDS).length, 1, 'a unique command prefix returns one match');
  eqIs(commandAutocomplete('/mo', CARD_COMMANDS)[0].token, '/model', 'the match carries the full command token');
  eqIs(commandAutocomplete('fix the bug /', CARD_COMMANDS).length, CARD_COMMANDS.length, 'a bare slash matches every command');
  eqIs(commandAutocomplete('/', CARD_COMMANDS).length, CARD_COMMANDS.length, 'works with no goal text before it too');
  eqIs(commandAutocomplete('/nope', CARD_COMMANDS).length, 0, 'no command starts with an unknown prefix');
  eqIs(commandAutocomplete('fix /model bug', CARD_COMMANDS).length, 0, 'once a space follows the token, the goal has moved on');
  eqIs(commandAutocomplete('fix the bug', CARD_COMMANDS).length, 0, 'no trailing slash means no suggestions');
  ok(
    STACK_COMMANDS.some((c) => c.command === 'loop') && !CARD_COMMANDS.some((c) => c.command === 'loop'),
    '`loop` is stack-scope only — no per-card loop count to override'
  );
  ok(
    CARD_COMMANDS.some((c) => c.command === 'guard') && STACK_COMMANDS.some((c) => c.command === 'guard'),
    '`guard` exists at both scopes, opening the card’s or the stack’s own guardrails popover'
  );

  eqIs(
    commandValueAutocomplete('/model/op', 'model', MODEL_OPTIONS).length,
    1,
    'level 2 filters the given catalog by the value typed so far'
  );
  eqIs(
    commandValueAutocomplete('/model/op', 'model', MODEL_OPTIONS)[0].token,
    '/model/claude-opus-4-8',
    'the level-2 token embeds the real value directly — no label/path resolution step, unlike @repo'
  );
  eqIs(commandValueAutocomplete('/model/', 'model', MODEL_OPTIONS).length, MODEL_OPTIONS.length, 'an empty value query matches everything');
  eqIs(commandValueAutocomplete('/model/nope', 'model', MODEL_OPTIONS).length, 0, 'no option starts with an unknown value prefix');
  eqIs(commandValueAutocomplete('/model/opus done', 'model', MODEL_OPTIONS).length, 0, 'a space after the value token closes the list');
  eqIs(
    commandValueAutocomplete('/effort/lo', 'effort', EFFORT_OPTIONS)[0]?.token,
    '/effort/low',
    'a different command matches its own catalog, not the one from the last call'
  );
  eqIs(
    detectPendingCommand(':research /model/', CARD_COMMANDS),
    'model',
    'hand-typing a value-picker token enters level-2 mode, same as clicking the level-1 suggestion would'
  );
  eqIs(detectPendingCommand('/model/op', CARD_COMMANDS), 'model', 'detects even with a partial value already typed');
  eqIs(detectPendingCommand('/guard/', CARD_COMMANDS), null, 'a non-value-picker command never enters level-2 mode');
  eqIs(detectPendingCommand('/nope/', CARD_COMMANDS), null, 'an unknown command name matches nothing');
  eqIs(detectPendingCommand('fix the bug', CARD_COMMANDS), null, 'no trailing /command/ token means no pending command');
  eqIs(detectPendingCommand('/loop/3', STACK_COMMANDS), 'loop', 'stack-scope commands are matched against their own list');

  eqIs(evalSuiteOptions().length, 3, "eval's catalog is the three suite shortcuts, not individual eval names");
  ok(evalSuiteOptions().every((o) => !o.label.includes(' ')), 'every suite name is space-free — the trailing-token grammar could not carry a spaced value');
}

// ── V&V: table-driven WIRED round-trip (§C) — one non-default value per WIRED
// field, asserting it lands correctly in CreateTaskOptions and that no WIRED
// field is silently dropped or renamed. `maxIterations: 0` ("off") gets its
// own row: off maps to a single pass (`max_iterations: 1`) on the wire, and
// it's the one value JS falsy-coercion bugs love to eat — this table would
// catch that class of regression.
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
    { name: 'maxIterations off (0) → single pass on the wire', apply: (c) => (c.maxIterations = 0), field: 'max_iterations', expected: 1 }
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
  eq(
    keys,
    ['acceptance', 'client_ref', 'effort', 'gate', 'max_iterations', 'model', 'on_fail', 'until'],
    'options carries exactly the expected WIRED key names — no silent rename/drop'
  );
  eqIs(
    cardToTaskPayload(fullyGuarded, defaults).options.client_ref,
    fullyGuarded.id,
    'client_ref always carries the card\'s own id, so the response traces back to this card even under dedup'
  );
}

// ── Loop-Stack connect: a card's branch override reaches the run-stack path,
// not just the bare-pane launch (`cardToTaskPayload` mirrors `paneSubmitPayload`'s
// "Target branch: …" constraint encoding) ─────────────────────────────────────
{
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi', branch: 'main' };
  const overridden = buildCard('x');
  overridden.config.branch = 'feature/x';
  eq(
    cardToTaskPayload(overridden, defaults).options.constraints,
    ['Target branch: feature/x'],
    'a card branch override surfaces as a planning constraint on the run-stack payload'
  );
  const inherited = buildCard('x');
  eq(
    cardToTaskPayload(inherited, defaults).options.constraints,
    ['Target branch: main'],
    'no card override ⇒ falls back to the pane default branch'
  );
  const noBranch = buildCard('x');
  const noBranchDefaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  eqIs(
    cardToTaskPayload(noBranch, noBranchDefaults).options.constraints,
    undefined,
    'no card override and no pane default branch ⇒ no constraints entry'
  );
  const whitespace = buildCard('x');
  whitespace.config.branch = '   ';
  eqIs(
    cardToTaskPayload(whitespace, defaults).options.constraints,
    undefined,
    'a whitespace-only branch override is treated as unset'
  );
}

// ── `auto` model: a non-concrete sentinel that must never hit the wire as
// the literal string "auto" — `select_model`'s override check would pass it
// straight to the CLI as `--model auto` and fail. Omitting `model` entirely
// is how the heuristic gets to run. ─────────────────────────────────────────
{
  ok(
    MODEL_OPTIONS.some((o) => o.value === AUTO_MODEL),
    'MODEL_OPTIONS carries a real `auto` entry'
  );
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const autoCard = buildCard('x');
  autoCard.config.model = AUTO_MODEL;
  eqIs(
    cardToTaskPayload(autoCard, defaults).options.model,
    undefined,
    'a card explicitly set to auto omits model from the run-stack payload'
  );
  const autoDefaults = { model: AUTO_MODEL, effort: 'medium', repo: 'konjoai/lopi' };
  const plainCard = buildCard('x');
  eqIs(
    cardToTaskPayload(plainCard, autoDefaults).options.model,
    undefined,
    'a pane default of auto (no card override) also omits model'
  );
  eqIs(
    paneSubmitPayload({ goal: 'g', repo: 'r', model: AUTO_MODEL }).options.model,
    undefined,
    'a bare-pane launch set to auto also omits model, not the literal string'
  );
  eqIs(
    stackDefaultsSummary({ ...DEFAULT_STACK_DEFAULTS, model: AUTO_MODEL }),
    'model Auto · every loop inherits',
    'the dock summary renders the Auto label, not the bare "auto" wire value'
  );
}

// ── Backend-1: "Run once" forces max_iterations=1 without mutating the card ──
{
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const c = buildCard(':optimize "x" x7');
  eqIs(c.maxIterations, 7, 'sanity: the card itself carries the xN value');
  const runOncePayload = cardToTaskPayloadForRunOnce(c, defaults);
  eqIs(runOncePayload.options.max_iterations, 1, 'Run once overrides max_iterations to 1 in the outgoing payload');
  eqIs(c.maxIterations, 7, 'Run once never mutates the card\'s own stored maxIterations');
  const off = buildCard('x');
  off.maxIterations = 0;
  eqIs(
    cardToTaskPayloadForRunOnce(off, defaults).options.max_iterations,
    1,
    'Run once on an off (0) card still sends a single pass'
  );
}

// ── Unify-1 Phase 1: bare pane prompt → the same unified createTask payload ───
// A Forge-style pane's submit now flows through `createTask` (via
// `paneSubmitPayload`) exactly as a stack card's launch does — no separate
// `postTask`. These prove a bare prompt (a) carries only what its launch
// controls set, forcing no stack-loop semantics, and (b) produces the identical
// CreateTaskRequest *shape* a one-card stack would for the same inputs.
{
  // A truly bare prompt: only a goal, everything else unset.
  const bare = paneSubmitPayload({ goal: 'fix foo', repo: '' });
  eqIs(bare.goal, 'fix foo', 'bare prompt carries the goal verbatim');
  eqIs(bare.repo, '', 'bare prompt leaves repo empty (server falls back to its configured repo)');
  eqIs(bare.priority, 'normal', 'bare prompt defaults priority to normal');
  eq(Object.keys(bare.options).sort(), [], 'a bare prompt sets NO options — no model/effort/gate/until/acceptance/max_iterations forced on it');
}
{
  // Launch-control-driven bare prompt: model/effort/priority set, no branch.
  const p = paneSubmitPayload({ goal: 'g', repo: 'konjoai/lopi', priority: 'high', model: 'claude-opus-4-8', effort: 'high' });
  eqIs(p.priority, 'high', 'priority passes through from the launch controls');
  eqIs(p.options.model, 'claude-opus-4-8', 'model surfaces as a first-class option, not a prompt constraint');
  eqIs(p.options.effort, 'high', 'effort surfaces as a first-class option, not a prompt constraint');
  eqIs(p.options.constraints, undefined, 'no branch ⇒ no constraints entry');
  eq(Object.keys(p.options).sort(), ['effort', 'model'], 'only the set launch-control fields appear — nothing stack-only leaks in');
}
{
  // A branch override survives the move off postTask, as a planning constraint.
  const p = paneSubmitPayload({ goal: 'g', repo: 'r', branch: 'feature/x' });
  eq(p.options.constraints, ['Target branch: feature/x'], 'a branch override surfaces as a planning constraint (the channel postTask used)');
  const trimmed = paneSubmitPayload({ goal: 'g', repo: 'r', branch: '   ' });
  eqIs(trimmed.options.constraints, undefined, 'a whitespace-only branch is treated as unset');
}
{
  // Shape parity: for the SAME goal/repo/model/effort/priority, a bare pane
  // prompt and a one-card stack launch agree on every shared field. The card
  // adds only its stack-loop semantics (max_iterations/on_fail/client_ref) —
  // which a bare prompt intentionally omits — so parity is asserted on the
  // fields both actually carry.
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  type Row = { name: string; goal: string; model?: string; effort?: string; priority?: string };
  const rows: Row[] = [
    { name: 'plain goal, pane defaults', goal: 'do the thing' },
    { name: 'model + effort override', goal: 'do the thing', model: 'claude-opus-4-8', effort: 'high' },
    { name: 'high priority', goal: 'urgent', priority: 'high' }
  ];
  for (const row of rows) {
    // The pane launch.
    const pane = paneSubmitPayload({ goal: row.goal, repo: defaults.repo, priority: row.priority, model: row.model ?? defaults.model, effort: row.effort ?? defaults.effort });
    // The equivalent one-card stack launch.
    const c = buildCard(`"${row.goal}"`);
    if (row.model) c.config.model = row.model;
    if (row.effort) c.config.effort = row.effort;
    const stack = cardToTaskPayload(c, defaults);
    eqIs(pane.goal, stack.goal, `parity/${row.name}: same goal`);
    eqIs(pane.repo, stack.repo, `parity/${row.name}: same repo`);
    eqIs(pane.options.model, stack.options.model, `parity/${row.name}: same model`);
    eqIs(pane.options.effort, stack.options.effort, `parity/${row.name}: same effort`);
  }
}

// ── Backend-1: execution order is bottom-of-stack (oldest) first ─────────────
{
  const cards = [card('newest'), card('middle'), card('oldest')];
  eq(
    executionOrder(cards).map((c) => c.id),
    ['oldest', 'middle', 'newest'],
    'execution order reverses the array — the composer prepends, so the last element is the oldest/next-to-run'
  );
  eq(executionOrder([]), [], 'execution order of an empty stack is empty, not an error');
  ok(executionOrder(cards) !== cards, 'execution order never mutates or aliases the input array');
}

// ── Backend-1: dry run validates without ever calling createTask ─────────────
{
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const clean = [card('a', 'do a'), card('b', 'do b')];
  const result = dryRunStack(clean, defaults);
  ok(result.valid, 'a stack of well-formed cards dry-runs clean');
  eq(result.issues, [], 'no issues on a clean stack');
  eq(
    result.plan.map((p) => p.goal),
    ['do b', 'do a'],
    'the plan is listed in execution order (bottom/oldest first), not array order'
  );
}
{
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const empty = buildCard('');
  const badGate = buildCard('has a goal');
  badGate.guardrails = { ...badGate.guardrails, gate: true, gateCmd: '   ' };
  const badUntil = buildCard('also has a goal');
  badUntil.guardrails = { ...badUntil.guardrails, until: true, untilCmd: '' };
  const result = dryRunStack([empty, badGate, badUntil], defaults);
  ok(!result.valid, 'a stack with any bad card is invalid overall');
  eq(result.issues.length, 3, 'each bad card contributes exactly one issue');
  ok(
    result.issues.some((i) => i.cardId === empty.id && i.message.includes('empty')),
    'the empty-goal card is flagged by id'
  );
  ok(
    result.issues.some((i) => i.cardId === badGate.id && i.message.includes('gate')),
    'the empty-gate-command card is flagged by id'
  );
  ok(
    result.issues.some((i) => i.cardId === badUntil.id && i.message.includes('until')),
    'the empty-until-command card is flagged by id'
  );
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

// ── bumpInOrder: reordering a queued (not-yet-started) card mid-run ─────────
{
  const order = ['a', 'b', 'c', 'd'];
  const up = bumpInOrder(order, 0, 'c', 'up');
  ok(up.ok, 'bumping a queued card up succeeds');
  if (up.ok) eq(up.order, ['a', 'c', 'b', 'd'], 'bump up swaps the card with its immediate predecessor');

  const down = bumpInOrder(order, 0, 'b', 'down');
  ok(down.ok, 'bumping a queued card down succeeds');
  if (down.ok) eq(down.order, ['a', 'c', 'b', 'd'], 'bump down swaps the card with its immediate successor');

  ok(order[0] === 'a' && order[1] === 'b', 'bumpInOrder never mutates the input order array');
}
{
  const order = ['a', 'b', 'c', 'd'];
  const missing = bumpInOrder(order, 0, 'z', 'up');
  ok(!missing.ok, 'bumping a card id absent from the order is rejected');
  if (!missing.ok) eq(missing.error, 'card is not part of this run’s plan', 'the not-found error names the actual problem');
}
{
  const order = ['a', 'b', 'c', 'd'];
  const runningCard = bumpInOrder(order, 1, 'a', 'down');
  ok(!runningCard.ok, 'bumping the already-running (at-cursor) card is rejected');
  if (!runningCard.ok) {
    eq(
      runningCard.error,
      'card is already running or finished — only queued cards can be bumped',
      'the at-cursor rejection explains why'
    );
  }
  const finishedCard = bumpInOrder(order, 1, 'a', 'down');
  ok(!finishedCard.ok, 'bumping a finished (before-cursor) card is rejected');
}
{
  const order = ['a', 'b', 'c', 'd'];
  const aboveRunning = bumpInOrder(order, 1, 'c', 'up');
  ok(!aboveRunning.ok, 'bumping a queued card to land at-or-before the cursor is rejected');
  if (!aboveRunning.ok) {
    eq(
      aboveRunning.error,
      'cannot bump above the currently running card',
      'the above-cursor rejection explains why'
    );
  }
}
{
  const order = ['a', 'b', 'c', 'd'];
  const pastEnd = bumpInOrder(order, 0, 'd', 'down');
  ok(!pastEnd.ok, 'bumping the last queued card further down is rejected');
  if (!pastEnd.ok) {
    eq(pastEnd.error, 'cannot bump past the end of the queue', 'the past-the-end rejection explains why');
  }
}

// ── Stack-1: stack-level ops — duplicate / reorder / delete a whole pane ──────
{
  const state = [pane('s1', [card('a'), card('b')]), pane('s2', [card('x')])];
  const dup = duplicateStack(state, 's1');
  eq(dup.map((p) => p.key), [state[0].key, dup[1].key, state[1].key], 'duplicateStack inserts the clone immediately after the original');
  eqIs(dup[1].title, 's1 copy', 'the clone gets a distinguishing title');
  eq(dup[1].cards.map((c) => c.goal), ['a', 'b'], 'the clone carries every card the original had');
  ok(
    dup[1].cards.every((c, i) => c.id !== dup[0].cards[i].id),
    'every cloned card gets a fresh id, not a shared reference to the original'
  );
  ok(dup[1].config !== dup[0].config, 'the clone gets its own config object, not a shared reference');
  ok(dup[1].config.cron !== dup[0].config.cron, 'nested config objects (cron) are cloned too, not shared');
}
{
  const running = card('a');
  running.status = 'running';
  running.iteration = { current: 2, total: 5 };
  running.taskId = 'task-123';
  const state = [pane('s1', [running])];
  const dup = duplicateStack(state, 's1');
  eqIs(dup[1].cards[0].status, 'idle', 'a cloned card resets its run status to idle');
  eqIs(dup[1].cards[0].taskId, undefined, 'a cloned card drops any taskId from the original run');
  eqIs(dup[1].cards[0].iteration, undefined, 'a cloned card drops live iteration progress');
}
{
  const state = [pane('s1'), pane('s2')];
  const untouched = duplicateStack(state, 'missing');
  ok(untouched === state, 'duplicateStack on an unknown key is a total no-op');
}

// ── Stack-Templates-1: "saved stacks" — copy another open pane's cards ───────
{
  const state = [pane('s1', [card('a'), card('b')]), pane('s2', [card('x')])];
  const next = loadStackCardsInto(state, 's2', 's1');
  eq(next[1].cards.map((c) => c.goal), ['a', 'b'], "the target pane's cards become a copy of the source pane's");
  eq(next[0].cards.map((c) => c.goal), ['a', 'b'], 'the source pane is left untouched');
  ok(
    next[1].cards.every((c, i) => c.id !== next[0].cards[i].id),
    'every copied card gets a fresh id, not a shared reference to the source'
  );
}
{
  const running = card('a');
  running.status = 'running';
  running.iteration = { current: 2, total: 5 };
  running.taskId = 'task-123';
  const state = [pane('s1', [running]), pane('s2')];
  const next = loadStackCardsInto(state, 's2', 's1');
  eqIs(next[1].cards[0].status, 'idle', 'a copied card resets its run status to idle');
  eqIs(next[1].cards[0].taskId, undefined, 'a copied card drops any taskId from the source');
  eqIs(next[1].cards[0].iteration, undefined, 'a copied card drops live iteration progress');
}
{
  const state = [pane('s1', [card('a')]), pane('s2')];
  ok(loadStackCardsInto(state, 's1', 's1') === state, 'copying a pane into itself is a no-op');
  ok(loadStackCardsInto(state, 's2', 'missing') === state, 'copying from an unknown source key is a no-op');
}
{
  const state = [pane('a'), pane('b'), pane('c')];
  const moved = reorderStacks(state, 0, 2);
  eq(moved.map((p) => p.key), ['b', 'c', 'a'], 'reorderStacks moves the pane at "from" to index "to"');
  const outOfRange = reorderStacks(state, 0, 9);
  ok(outOfRange === state, 'reorderStacks with an out-of-range index is a no-op');
}
{
  const state = [pane('a'), pane('b'), pane('c')];
  const before = moveStackBeforeOrAfter(state, 2, 0, true);
  eq(before.map((p) => p.key), ['c', 'a', 'b'], 'moveStackBeforeOrAfter(before) drops the dragged pane just before the target');
  const after = moveStackBeforeOrAfter(state, 0, 2, false);
  eq(after.map((p) => p.key), ['b', 'c', 'a'], 'moveStackBeforeOrAfter(after) drops the dragged pane just after the target');
  ok(moveStackBeforeOrAfter(state, 1, 1, true) === state, 'dropping a pane onto itself is a no-op');
}
{
  const state = [pane('s1'), pane('s2')];
  const deleted = deleteStack(state, 's1');
  eq(deleted.map((p) => p.key), ['s2'], 'deleteStack drops the named pane');
}
{
  const state = [pane('only')];
  const guarded = deleteStack(state, 'only');
  ok(guarded === state, 'deleteStack refuses to empty the last remaining pane (no pane-creation affordance exists to recover)');
}

// ── Stack-1: stack-level active-state predicates (hide-inactive summaries) ───
{
  const config = defaultStackConfig();
  ok(!stackGuardActive(config.guardrails), 'a fresh stack\'s guardrails read inactive (onFail is still the default "stop")');
  ok(stackGuardActive({ ...config.guardrails, onFail: 'continue' }), 'onFail moved off "stop" reads as active');
  ok(!stackEvalActive(config), 'a fresh stack\'s evals read inactive (baseline only)');
  ok(stackEvalActive({ ...config, evals: [BASELINE_EVAL, { name: 'tests pass', tier: 'test' }] }), 'more than baseline reads as active');
  ok(!stackDefaultsActive(config.defaults), 'a fresh stack\'s defaults read inactive (still the app-wide baseline)');
  ok(
    stackDefaultsActive({ ...DEFAULT_STACK_DEFAULTS, model: 'claude-sonnet-4-6' }),
    'a defaults field moved off the app-wide baseline reads as active'
  );
}

// ── B1: the stack goal facet — active-state, pursuit gate, summary, default ─
{
  const config = defaultStackConfig();
  ok(!config.goal.pursue, 'a fresh stack does not pursue a goal (additive default off)');
  eqIs(defaultStackGoal().noProgressLimit, 3, 'the default no-progress tolerance is 3 chain-runs');
  ok(!stackGoalActive(config), "a fresh stack's goal facet reads inactive");
  ok(stackGoalActive({ ...config, goal: { pursue: true, noProgressLimit: 3 } }), 'pursue on reads as active');
  // pursue on but baseline-only acceptance is inert — nothing to pursue.
  ok(
    !stackPursuesGoal({ ...config, goal: { pursue: true, noProgressLimit: 3 } }),
    'pursue with baseline-only acceptance is inert — not a real goal'
  );
  ok(
    stackPursuesGoal({
      ...config,
      evals: [BASELINE_EVAL, { name: 'tests pass', tier: 'test' }],
      goal: { pursue: true, noProgressLimit: 3 }
    }),
    'pursue on + acceptance beyond baseline is a real goal the sequencer will pursue'
  );
  ok(
    !stackPursuesGoal({ ...config, evals: [BASELINE_EVAL, { name: 'tests pass', tier: 'test' }] }),
    'acceptance without pursue is not pursued — the toggle is required'
  );
  ok(stackGoalSummary({ ...config, loopCount: 5 }).includes('≤5'), 'a finite ceiling shows the chain-run cap');
  ok(stackGoalSummary({ ...config, loopCount: 0 }).includes('until met'), 'an infinite ceiling reads "until met"');
}

// ── B1: duplicateStack clones the goal facet (own object, not shared) ───────
{
  const state: StackPaneState[] = [
    {
      key: 's1',
      title: 'one',
      cards: [],
      config: { ...defaultStackConfig(), goal: { pursue: true, noProgressLimit: 4 } },
      draft: makeDraft()
    }
  ];
  const dup = duplicateStack(state, 's1');
  eqIs(dup[1].config.goal.pursue, true, 'the clone carries the original goal facet');
  eqIs(dup[1].config.goal.noProgressLimit, 4, 'the clone carries the goal fields verbatim');
  ok(dup[1].config.goal !== dup[0].config.goal, 'the cloned goal is its own object, not a shared reference');
}

// ── Stack-1 §1: the second precedence rule — stack schedule/loop-count GOVERN
// a per-loop card's own schedule display, pure and load-bearing ─────────────
{
  const config = defaultStackConfig();
  ok(!perLoopScheduleGoverned(config), 'an un-scheduled, un-looped (×1) stack does not govern per-loop schedules');
  ok(perLoopScheduleGoverned({ ...config, scheduled: true }), 'a scheduled stack governs per-loop schedules');
  ok(perLoopScheduleGoverned({ ...config, loopCount: 3 }), 'a looped (×3) stack governs per-loop schedules');
  ok(perLoopScheduleGoverned({ ...config, loopCount: 0 }), 'an infinitely-looped (×∞) stack governs per-loop schedules');
  ok(!perLoopScheduleGoverned({ ...config, loopCount: 1, scheduled: false }), 'explicitly ×1 and unscheduled does not govern');
}

// ── Stack-1 §1/§4 Phase 2: default resolution — `loop ?? stack.default ?? DEF`
// table-driven, proving a loop override beats its stack's default and an
// unset loop inherits it. `cardToTaskPayload`'s `defaults` param structurally
// accepts a full `StackDefaults` (a superset of the 3 WIRED fields) — this is
// the exact object shape `pane.config.defaults` is in production. ──────────
{
  const stackDefault = { ...DEFAULT_STACK_DEFAULTS, model: 'claude-sonnet-4-6', effort: 'high', repo: 'konjoai/stack-repo' };
  type Row = { name: string; apply: (c: StackCard) => void; field: 'model' | 'effort' | 'repo'; expected: string };
  const rows: Row[] = [
    { name: 'model: unset loop inherits the stack default', apply: () => {}, field: 'model', expected: 'claude-sonnet-4-6' },
    {
      name: 'model: a loop override beats the stack default',
      apply: (c) => (c.config.model = 'claude-opus-4-8'),
      field: 'model',
      expected: 'claude-opus-4-8'
    },
    { name: 'effort: unset loop inherits the stack default', apply: () => {}, field: 'effort', expected: 'high' },
    {
      name: 'effort: a loop override beats the stack default',
      apply: (c) => (c.config.effort = 'low'),
      field: 'effort',
      expected: 'low'
    },
    { name: 'repo: unset loop inherits the stack default', apply: () => {}, field: 'repo', expected: 'konjoai/stack-repo' },
    {
      name: 'repo: a loop override beats the stack default',
      apply: (c) => (c.config.repo = 'konjoai/other'),
      field: 'repo',
      expected: 'konjoai/other'
    }
  ];
  for (const row of rows) {
    const c = buildCard('precedence row');
    row.apply(c);
    const payload = cardToTaskPayload(c, stackDefault);
    const actual = row.field === 'repo' ? payload.repo : (payload.options as unknown as Record<string, unknown>)[row.field];
    eqIs(actual, row.expected, row.name);
  }
  // And the third rung: a stack itself never overrides — it only supplies a
  // fallback — so a stack default that happens to equal the app-wide DEF is
  // indistinguishable from "nothing was ever configured," which is exactly
  // the intended behavior (no separate "unset stack default" state exists).
  const untouchedStack = { ...DEFAULT_STACK_DEFAULTS };
  const plain = buildCard('no overrides anywhere');
  const payload = cardToTaskPayload(plain, untouchedStack);
  eqIs(payload.options.model, DEFAULT_STACK_DEFAULTS.model, 'with no loop override and an untouched stack default, DEF wins through both rungs');
}

// ── A1: evals → acceptance (the eval UI finally executes) ─────────────────────
{
  // The baseline alone compiles into a single deterministic execution_ok check
  // — objective criteria route to the deterministic tier, never the judge.
  const acc = evalsToAcceptance([BASELINE_EVAL]);
  ok(acc !== undefined, 'baseline compiles into a real acceptance');
  eqIs(acc!.checks.length, 1, 'baseline alone ⇒ one check');
  eqIs(acc!.checks[0].spec.kind, 'execution_ok', 'baseline ⇒ deterministic execution_ok tier');
  eqIs(acc!.checks[0].required, true, 'the baseline check is a hard gate');
}
{
  // base + test tiers collapse into ONE deterministic check (both objective).
  const acc = evalsToAcceptance([BASELINE_EVAL, { name: 'tests pass', tier: 'test' }, { name: 'unit', tier: 'test' }]);
  const kinds = acc!.checks.map((c) => c.spec.kind);
  eq(kinds, ['execution_ok'], 'base + multiple test evals ⇒ a single deterministic check, none sent to the judge');
}
{
  // Multiple judge evals fold into ONE judge check whose rubric criteria are
  // their names — one model call, reserved for genuine judgment.
  const acc = evalsToAcceptance([
    BASELINE_EVAL,
    { name: 'code review', tier: 'judge' },
    { name: 'beats-best', tier: 'judge' }
  ]);
  const judge = acc!.checks.find((c) => c.spec.kind === 'judge');
  ok(judge !== undefined, 'judge evals compile into a judge check');
  eq(
    (judge!.spec as { kind: 'judge'; rubric: { criteria: string[] } }).rubric.criteria,
    ['code review', 'beats-best'],
    'judge check rubric carries every selected judge eval name'
  );
  eqIs(acc!.checks.filter((c) => c.spec.kind === 'judge').length, 1, 'all judge evals fold into a single judge check');
}
{
  // Each suite eval becomes its own suite check, carrying its name.
  const acc = evalsToAcceptance([BASELINE_EVAL, { name: 'vuln scan', tier: 'suite' }, { name: 'adversarial', tier: 'suite' }]);
  const suites = acc!.checks.filter((c) => c.spec.kind === 'suite');
  eqIs(suites.length, 2, 'two suite evals ⇒ two suite checks');
  eq(
    suites.map((s) => (s.spec as { kind: 'suite'; name: string }).name).sort(),
    ['adversarial', 'vuln scan'],
    'suite checks carry their eval names'
  );
}
{
  // Nothing to check ⇒ undefined, so the loop falls back to the legacy gate.
  eqIs(evalsToAcceptance([]), undefined, 'no evals ⇒ no acceptance (legacy score.passed() gate)');
}
{
  // The card payload actually carries the compiled acceptance now — the eval
  // UI is no longer inert intent.
  const defaults = { model: 'sonnet', effort: 'medium', repo: 'konjoai/lopi' };
  const c = buildCard('ship it');
  c.evals = [BASELINE_EVAL, { name: 'code review', tier: 'judge' }];
  const payload = cardToTaskPayload(c, defaults);
  ok(payload.options.acceptance !== undefined, 'cardToTaskPayload now emits a real acceptance');
  eqIs(payload.options.acceptance!.checks.length, 2, 'base + judge ⇒ two checks in the payload');
}

// ── bare vs. stack chrome + pane creation ──────────────────────────────────
{
  const cfg = defaultStackConfig();
  const empty = { key: 'e', title: 't', cards: [], config: cfg, draft: makeDraft() };
  const one = { key: 'o', title: 't', cards: [buildCard('a')], config: cfg, draft: makeDraft() };
  const two = { key: 'w', title: 't', cards: [buildCard('a'), buildCard('b')], config: cfg, draft: makeDraft() };
  ok(paneIsBare(empty), 'an empty pane is bare (composer + idle orb only)');
  ok(!paneIsBare(one), 'the first card already earns the stack chrome (dock + connectors)');
  ok(!paneIsBare(two), 'a second loop keeps the stack chrome');
}
{
  const blank = makeBlankStack();
  eqIs(blank.cards.length, 0, 'a fresh pane starts empty');
  ok(blank.key.length > 0, 'a fresh pane has a unique key');
  ok(paneIsBare(blank), 'a fresh pane is bare');
  const blank2 = makeBlankStack();
  ok(blank.key !== blank2.key, 'each fresh pane gets its own key');
  // config is its own object, never shared (editing one cannot leak to another).
  ok(blank.config !== blank2.config, 'each fresh pane gets its own config object');
}
{
  const state = [makeBlankStack('one')];
  const grown = addStack(state);
  eqIs(grown.length, 2, 'addStack appends one pane');
  ok(grown[0] === state[0], 'addStack leaves existing panes by reference');
  ok(state.length === 1, 'addStack is pure — original array untouched');
}

// ── Creation-Flow-1: draft card, templates, provenance ────────────────────────
{
  // A fresh draft is the composer replacement: status 'draft', empty + not hot.
  const d = makeDraft();
  eqIs(d.status, 'draft', 'makeDraft starts in the draft status');
  eqIs(d.goal, '', 'a fresh draft has an empty goal');
  ok(!draftIsHot(d), 'an empty draft is not hot (nothing to commit)');
  ok(draftIsHot({ ...d, goal: 'fix foo' }), 'a draft with goal text is hot');
  ok(draftIsHot({ ...d, alias: 'research' }), 'a draft with an alias is hot');
  ok(draftIsHot({ ...d, tpl: 'kcqf sprint' }), 'a draft with a template origin is hot');
}
{
  // §1.1 draft-excluded-from-run: executionOrder must never schedule a draft,
  // even if one somehow appears in a card list.
  const runnable = buildCard('do the thing');
  const draft = makeDraft();
  const order = executionOrder([draft, runnable]);
  eqIs(order.length, 1, 'a draft is excluded from the execution order');
  eqIs(order[0].id, runnable.id, 'only the committed card runs');
  ok(
    !executionOrder([draft]).length,
    'a lone draft yields an empty run plan (never falls through to a run path)'
  );
}
{
  // finalizeDraft: a raw draft honors inline `:alias @repo ×N`; the token text
  // is stripped from the committed goal.
  const draft = { ...makeDraft(), goal: ':research investigate X @konjoai/lopi x3' };
  const committed = finalizeDraft(draft);
  eqIs(committed.status, 'idle', 'finalizeDraft commits to idle');
  eqIs(committed.preset, 'research', 'inline :alias resolves to its preset');
  eqIs(committed.goal, 'investigate X', 'tokens are stripped from the committed goal');
  eqIs(committed.config.repo, 'konjoai/lopi', 'inline @repo lands on config');
  eqIs(committed.maxIterations, 3, 'inline ×N sets the iteration ceiling');
}
{
  // finalizeDraft resolves an inline @repo's label to its real path when a
  // catalog is supplied — the bug fix: the stored config.repo must be a path
  // (`CreateTaskRequest.repo` reaches `git2::Repository::open`), never the
  // decorative label the autocomplete inserted into the goal text.
  const repos = [{ value: '/h/lopi', label: 'konjoai/lopi', hint: '/h/lopi' }];
  const draft = { ...makeDraft(), goal: '@konjoai/lopi fix the bug' };
  const committed = finalizeDraft(draft, repos);
  eqIs(committed.config.repo, '/h/lopi', 'inline @repo resolves to its path, not the label, when a catalog is given');
}
{
  // adoptRepoDefaultIfUnset — the "first inline @repo becomes the stack
  // default" rule, pulled out of commitDraft so it's testable without a
  // live panes store.
  const unsetDefaults = DEFAULT_STACK_DEFAULTS;
  eqIs(unsetDefaults.repo, '', 'sanity: the cold-start default repo is the empty/auto sentinel');
  const withRepo = { ...buildCard('"x"'), config: { repo: '/h/lopi' } };
  eqIs(
    adoptRepoDefaultIfUnset(unsetDefaults, withRepo).repo,
    '/h/lopi',
    'a committed card with a repo seeds the still-unset stack default'
  );
  const alreadySet = { ...unsetDefaults, repo: '/h/other' };
  eqIs(
    adoptRepoDefaultIfUnset(alreadySet, withRepo).repo,
    '/h/other',
    'an already-explicit stack default is never clobbered by a later card'
  );
  const noRepoCard = buildCard('"x"');
  eqIs(
    adoptRepoDefaultIfUnset(unsetDefaults, noRepoCard).repo,
    '',
    'a card with no repo of its own leaves the stack default untouched'
  );
}
{
  // A dropdown-configured draft (preset/template already set) commits as-is —
  // inline parsing does not clobber a deliberate configuration.
  const draft = applyPreset(makeDraft(), 'implement');
  draft.goal = 'build the widget';
  const committed = finalizeDraft(draft);
  eqIs(committed.preset, 'implement', 'a configured draft keeps its preset on commit');
  eqIs(committed.goal, 'build the widget', 'a configured draft keeps its literal goal');
}
{
  // applyPreset sets preset/alias/evals and clears any template provenance.
  const withTpl = { ...makeDraft(), tpl: 'x', tplKind: 'prompt' as const };
  const p = applyPreset(withTpl, 'optimize');
  eqIs(p.preset, 'optimize', 'applyPreset sets the preset');
  eqIs(p.alias, 'optimize', 'applyPreset sets the alias to the preset key');
  eq(p.evals, PRESET_CATALOG.optimize.evals, 'applyPreset attaches the preset eval suite');
  eqIs(p.tpl, undefined, 'picking a bare preset clears template provenance');
  eqIs(p.tplKind, undefined, 'picking a bare preset clears the template kind');
}
{
  // provenance-survives-edit: a prompt template stamps tpl/tplKind, and editing
  // the goal afterward must NOT erase the origin (it records origin, not drift).
  const tpl: PromptTemplate = { id: 't1', name: 'deep research', preset: 'research', goal: 'investigate' };
  const filled = applyPromptTemplate(makeDraft(), tpl);
  eqIs(filled.tpl, 'deep research', 'applyPromptTemplate stamps the template name');
  eqIs(filled.tplKind, 'prompt', 'prompt-template provenance kind');
  eqIs(filled.preset, 'research', 'the template preset drives evals/config');
  const edited = { ...filled, goal: 'investigate something else entirely' };
  eqIs(edited.tpl, 'deep research', 'provenance survives an edit to goal');
  eqIs(edited.tplKind, 'prompt', 'provenance kind survives an edit to goal');
  // And it survives a commit too.
  eqIs(finalizeDraft(edited).tpl, 'deep research', 'provenance survives commit');
}
{
  // §1.4 bottom-first round-trip — the easiest thing to get backwards. Build a
  // pane the way addCard does (prepend → newest on top, bottom runs first),
  // serialize it, apply it into an empty pane, and assert identical run order.
  //   run order (executionOrder): first → last.
  let cards: StackCard[] = [];
  cards = addCard(cards, buildCard(':research first'));   // added first → sinks to bottom → runs first
  cards = addCard(cards, buildCard(':implement second'));
  cards = addCard(cards, buildCard(':optimize third'));   // added last → top → runs last
  const runGoalsBefore = executionOrder(cards).map((c) => c.goal);
  eq(runGoalsBefore, ['first', 'second', 'third'], 'sanity: bottom card runs first');

  const tpl = stackTemplateFromCards(cards, 'my chain');
  eqIs(tpl.loops[0].goal, 'first', 'serialized bottom-first: first-to-run is loop[0]');
  eqIs(tpl.loops[0].preset, 'research', 'serialized loop carries its preset');

  const restored = applyStackTemplate([], tpl);
  const runGoalsAfter = executionOrder(restored).map((c) => c.goal);
  eq(runGoalsAfter, runGoalsBefore, 'applyStackTemplate round-trips into the same run order');
  // The template's first loop landed at the BOTTOM of the pane (last index).
  eqIs(restored[restored.length - 1].goal, 'first', "template's first loop lands at the bottom");
  // Round-trips through a second serialization too (idempotent).
  eq(stackTemplateFromCards(restored, 'again').loops.map((l) => l.goal), tpl.loops.map((l) => l.goal), 'double round-trip is stable');
}
{
  // Stack-template loops carry violet provenance + keep their own preset alias.
  const tpl: StackTemplate = {
    id: 's1',
    name: 'kcqf',
    loops: [{ preset: 'research', goal: 'r' }, { preset: 'implement', goal: 'i' }]
  };
  const cards = applyStackTemplate([], tpl);
  ok(cards.every((c) => c.tplKind === 'stack' && c.tpl === 'kcqf'), 'every dropped loop carries stack provenance');
  ok(cards.every((c) => !!c.alias), 'each loop keeps its own preset alias (distinct per loop)');
}
{
  // promptTemplateFromCard captures the card's identity, not its lineage.
  const c = applyPreset(makeDraft(), 'benchmark');
  c.goal = 'measure throughput';
  const t = promptTemplateFromCard(c, 'bench it');
  eqIs(t.name, 'bench it', 'prompt template takes the given name');
  eqIs(t.preset, 'benchmark', 'prompt template captures the preset');
  eqIs(t.goal, 'measure throughput', 'prompt template captures the goal');
}

namedSummary('stack');
