/**
 * Overview board rollup tests — `npx tsx src/lib/stores/stackOverview.test.ts`.
 *
 * Seeds panes (client-only Loop Stacks) plus a synthetic `agents` map and
 * checks `buildStackOverviewCards` buckets each stack into the right
 * lifecycle column, resolves its representative goal/repo/branch, colors
 * its loop dots, and formats its meta line — the same shape `overview.test.ts`
 * uses for the per-agent rollup.
 */
import {
  buildStackOverviewCards,
  groupByLifecycle,
  totalCost,
  LIFECYCLE_COLOR,
  type StackOverviewCard
} from './stackOverview';
import { buildCard, defaultStackConfig, makeDraft, type StackCard, type StackPaneState } from './stack';
import { makeBlank } from './agentReducer';
import type { AgentState } from './agents';
import { eq, eqIs, ok, summary } from '$lib/test-harness';

function mkCard(id: string, goal: string, patch: Partial<StackCard> = {}): StackCard {
  return { ...buildCard(`"${goal}"`), id, ...patch };
}

function mkPane(key: string, cards: StackCard[]): StackPaneState {
  return { key, title: key, cards, config: defaultStackConfig(), draft: makeDraft() };
}

function mkAgent(id: string, patch: Partial<AgentState> = {}): AgentState {
  return { ...makeBlank(id), ...patch };
}

// ── bare panes are excluded ────────────────────────────────────────────────
eq(buildStackOverviewCards([mkPane('bare', [])], new Map()), [], 'a cardless pane never reaches the board');

// ── queued: nothing has ever run ───────────────────────────────────────────
{
  const pane = mkPane('s1', [mkCard('a', 'do the thing'), mkCard('b', 'then this', { status: 'queued' })]);
  const [card] = buildStackOverviewCards([pane], new Map());
  eqIs(card.lifecycle, 'queued', 'no running/terminal cards -> queued');
  eqIs(card.metaRight, 'queued', 'queued meta text');
  eqIs(card.loopCount, 2, 'loop count matches card count');
}

// ── running: a card is mid-flight, agent phase not Testing ────────────────
{
  const running = mkCard('r', 'count files', { status: 'running', taskId: 't1' });
  const pane = mkPane('s2', [running]);
  const agents = new Map([['t1', mkAgent('t1', { phase: 'Implementation', elapsedMs: 134000, cost: 0.0041 })]]);
  const [card] = buildStackOverviewCards([pane], agents);
  eqIs(card.lifecycle, 'running', 'Implementation phase buckets as running, not testing');
  eqIs(card.goal, 'count files', 'goal comes from the running card');
  eqIs(card.metaRight, '2m 14s · $0.0041', 'running meta is elapsed + cost');
  eqIs(card.accentColor, LIFECYCLE_COLOR.running, 'running accent is the ice column color');
}

// ── testing: the running card's agent is in the Testing phase ─────────────
{
  const running = mkCard('t', 'verify report', { status: 'running', taskId: 't2' });
  const pane = mkPane('s3', [running]);
  const agents = new Map([['t2', mkAgent('t2', { phase: 'Testing', elapsedMs: 60000, cost: 0.01 })]]);
  const [card] = buildStackOverviewCards([pane], agents);
  eqIs(card.lifecycle, 'testing', 'Testing phase buckets as testing');
  eqIs(card.accentColor, LIFECYCLE_COLOR.testing, 'testing accent is the violet column color');
}

// ── done: every card terminal, all succeeded ───────────────────────────────
{
  const pane = mkPane('s4', [mkCard('d1', 'summarize', { status: 'done', taskId: 'a1' })]);
  const agents = new Map([['a1', mkAgent('a1', { cost: 0.0012 })]]);
  const [card] = buildStackOverviewCards([pane], agents);
  eqIs(card.lifecycle, 'done', 'all-terminal cards -> done');
  ok(!card.failed, 'no blocked card means not failed');
  eqIs(card.metaRight, '$0.0012', 'done meta is total cost');
  eqIs(card.accentColor, LIFECYCLE_COLOR.done, 'successful done accent is jade');
}

// ── done + failed: a blocked card anywhere marks the whole stack failed ───
{
  const pane = mkPane('s5', [mkCard('noop', 'noop probe', { status: 'blocked', blockReason: 'error' })]);
  const [card] = buildStackOverviewCards([pane], new Map());
  eqIs(card.lifecycle, 'done', 'blocked-only stack still lands in done');
  ok(card.failed, 'a blocked card marks the stack failed');
  eqIs(card.metaRight, 'failed', 'failed meta text overrides cost');
  eqIs(card.accentColor, '#ff0066', 'failed accent overrides jade with rose');
}

// ── loop dots: jade/rose/accent/dim per card status ────────────────────────
{
  const pane = mkPane('s6', [
    mkCard('l1', 'a', { status: 'done' }),
    mkCard('l2', 'b', { status: 'running', taskId: 't3' }),
    mkCard('l3', 'c', { status: 'idle' })
  ]);
  const agents = new Map([['t3', mkAgent('t3', { phase: 'Implementation' })]]);
  const [card] = buildStackOverviewCards([pane], agents);
  // executionOrder reverses pane.cards, so l3 (oldest/bottom) runs first.
  const colors = card.loops.map((l) => l.color);
  eq(colors.includes('#00ff9d'), true, 'a done loop is jade');
  eq(colors.includes('rgba(245,245,245,0.15)'), true, 'an untouched loop is dim');
  ok(card.loops.some((l) => l.pulsing), 'the running loop pulses');
}

// ── repo/branch fall back to the stack defaults when the card has none ────
{
  const pane = mkPane('s7', [mkCard('c1', 'goal')]);
  pane.config.defaults.repo = '/Users/dev/lopi';
  pane.config.defaults.branch = 'feat/x';
  const [card] = buildStackOverviewCards([pane], new Map());
  eqIs(card.repo, 'lopi', 'repo falls back to the pane default, basenamed');
  eqIs(card.branch, 'feat/x', 'branch falls back to the pane default');
}

// ── groupByLifecycle buckets in display order, and totalCost sums the map ─
{
  const cards: StackOverviewCard[] = buildStackOverviewCards(
    [mkPane('g1', [mkCard('a', 'x', { status: 'done' })])],
    new Map()
  );
  const groups = groupByLifecycle(cards);
  eq(groups.done.map((c) => c.key), ['g1'], 'done card lands in the done bucket');
  eq(groups.queued, [], 'queued bucket empty when nothing queued');
}
eqIs(
  totalCost(new Map([['a', mkAgent('a', { cost: 0.01 })], ['b', mkAgent('b', { cost: 0.02 })]])),
  0.03,
  'totalCost sums every agent in the map'
);

summary();
