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
  insertCardAt,
  parseComposerInput,
  suggestPreset,
  buildCard,
  type StackCard
} from './stack';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

function card(id: string, goal = id): StackCard {
  return { id, goal, literal: true, evals: [] };
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

// ── duplicate — clones in place ───────────────────────────────────────────────
{
  const dup = duplicateCard([card('a'), card('b')], 'a');
  eq(dup.length, 3, 'duplicate grows the stack by one');
  eq(dup[0].id, 'a', 'duplicate keeps the original at its position');
  eq(dup[1].goal, 'a', 'duplicate clones the original goal');
  ok(dup[1].id !== 'a', 'duplicate gets a fresh id');
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

namedSummary('stack');
