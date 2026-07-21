/**
 * stackOverview — projects the client-only `panes` (Loop Stacks) plus the
 * live `agents` map into the board `/overview` renders: one card per stack,
 * grouped into the four lifecycle columns (queued/running/testing/done).
 *
 * A "stack" here is a `StackPaneState` (see `stores/stack.ts`) — its `cards`
 * are the chained loops, each optionally bound to a live `AgentState` via
 * `taskId`. There is no server-side stack concept (mirrors `stackRun.ts`'s
 * doc comment), so this is a pure client-side rollup, same spirit as
 * `overview.ts`'s per-agent projection. Kept free of `$app`/Svelte imports —
 * only `import type` reaches into `./agents` — so it's unit-testable against
 * a seeded pane list + agent map without a browser.
 */
import type { AgentState } from './agents';
import { type StackCard, type StackPaneState, executionOrder, paneIsBare } from './stack';
import { SEED_BRANCH } from './stackDefaults';
import { formatElapsed } from './overview';

/** The four board columns, in display order. */
export type StackLifecycle = 'queued' | 'running' | 'testing' | 'done';

export const LIFECYCLE_ORDER: StackLifecycle[] = ['queued', 'running', 'testing', 'done'];

export const LIFECYCLE_LABEL: Record<StackLifecycle, string> = {
  queued: 'Queued',
  running: 'Running',
  testing: 'Testing',
  done: 'Done'
};

/** Fixed column accent colors — mirrors the four `--konjo-*` brand tokens
 *  used app-wide for these exact lifecycle meanings (ice=running,
 *  violet=testing, jade=done); queued gets the neutral paper-at-half tone. */
export const LIFECYCLE_COLOR: Record<StackLifecycle, string> = {
  queued: 'rgba(245,245,245,0.5)',
  running: '#00d4ff',
  testing: '#7c3aed',
  done: '#00ff9d'
};

const JADE = '#00ff9d';
const ROSE = '#ff0066';
const DIM = 'rgba(245,245,245,0.15)';

/** One loop's mini-progress-bar segment. */
export interface StackLoopDot {
  id: string;
  color: string;
  pulsing: boolean;
}

/** One stack, ready for the board — already resolved against live agent state. */
export interface StackOverviewCard {
  key: string;
  title: string;
  lifecycle: StackLifecycle;
  /** True when the stack's most recent run ended in a blocked/failed card. */
  failed: boolean;
  /** Left-accent / dot color — the lifecycle color, overridden to rose when `failed`. */
  accentColor: string;
  loopCount: number;
  loops: StackLoopDot[];
  goal: string;
  repo: string;
  branch: string;
  /** Right-aligned meta text — elapsed+cost while live, cost/failed once done, "queued" otherwise. */
  metaRight: string;
  metaRightColor: string;
}

function repoBasename(path: string | undefined): string {
  if (!path) return '';
  const parts = path.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function agentFor(card: StackCard, agents: ReadonlyMap<string, AgentState>): AgentState | undefined {
  return card.taskId ? agents.get(card.taskId) : undefined;
}

function loopDotColor(card: StackCard, accentColor: string): StackLoopDot {
  if (card.status === 'done') return { id: card.id, color: JADE, pulsing: false };
  if (card.status === 'blocked') return { id: card.id, color: ROSE, pulsing: false };
  if (card.status === 'running') return { id: card.id, color: accentColor, pulsing: true };
  return { id: card.id, color: DIM, pulsing: false };
}

/** Resolve one pane's lifecycle bucket + the "representative" card whose
 *  goal/repo/branch/agent stand in for the whole stack: the currently
 *  running card while live, the most recently executed card once done, or
 *  the next-to-run card while queued. */
function classify(
  order: StackCard[],
  agents: ReadonlyMap<string, AgentState>
): { lifecycle: StackLifecycle; rep: StackCard } {
  const running = order.find((c) => c.status === 'running');
  if (running) {
    const agent = agentFor(running, agents);
    return { lifecycle: agent?.phase === 'Testing' ? 'testing' : 'running', rep: running };
  }
  if (order.every((c) => c.status === 'done' || c.status === 'blocked')) {
    return { lifecycle: 'done', rep: order[order.length - 1] };
  }
  const next = order.find((c) => c.status !== 'done' && c.status !== 'blocked');
  return { lifecycle: 'queued', rep: next ?? order[0] };
}

function metaFor(
  lifecycle: StackLifecycle,
  rep: StackCard,
  order: StackCard[],
  failed: boolean,
  accentColor: string,
  agents: ReadonlyMap<string, AgentState>
): { text: string; color: string } {
  if (lifecycle === 'running' || lifecycle === 'testing') {
    const agent = agentFor(rep, agents);
    const elapsed = formatElapsed(agent?.elapsedMs ?? 0);
    const cost = (agent?.cost ?? 0).toFixed(4);
    return { text: `${elapsed} · $${cost}`, color: accentColor };
  }
  if (lifecycle === 'done') {
    if (failed) return { text: 'failed', color: ROSE };
    const total = order.reduce((sum, c) => sum + (agentFor(c, agents)?.cost ?? 0), 0);
    return { text: `$${total.toFixed(4)}`, color: 'rgba(245,245,245,0.4)' };
  }
  return { text: 'queued', color: 'rgba(245,245,245,0.4)' };
}

/** Project every non-bare pane into one board card. Panes with no cards yet
 *  (`paneIsBare`) are left off the board — they're an unstarted composer,
 *  not a stack worth showing on a lifecycle board. */
export function buildStackOverviewCards(
  panes: StackPaneState[],
  agents: ReadonlyMap<string, AgentState>
): StackOverviewCard[] {
  const out: StackOverviewCard[] = [];
  for (const pane of panes) {
    if (paneIsBare(pane)) continue;
    const order = executionOrder(pane.cards);
    if (order.length === 0) continue;

    const { lifecycle, rep } = classify(order, agents);
    const failed = lifecycle === 'done' && order.some((c) => c.status === 'blocked');
    const accentColor = failed ? ROSE : LIFECYCLE_COLOR[lifecycle];
    const loops = order.map((c) => loopDotColor(c, accentColor));
    const meta = metaFor(lifecycle, rep, order, failed, accentColor, agents);

    const repoPath = rep.config.repo || pane.config.defaults.repo;
    const branch = rep.config.branch || pane.config.defaults.branch || SEED_BRANCH;

    out.push({
      key: pane.key,
      title: pane.title,
      lifecycle,
      failed,
      accentColor,
      loopCount: order.length,
      loops,
      goal: rep.goal,
      repo: repoBasename(repoPath) || 'auto',
      branch,
      metaRight: meta.text,
      metaRightColor: meta.color
    });
  }
  return out;
}

/** Group already-built cards by column, in display order. */
export function groupByLifecycle(
  cards: StackOverviewCard[]
): Record<StackLifecycle, StackOverviewCard[]> {
  const groups: Record<StackLifecycle, StackOverviewCard[]> = {
    queued: [],
    running: [],
    testing: [],
    done: []
  };
  for (const card of cards) groups[card.lifecycle].push(card);
  return groups;
}

/** Total cost across every live/historic agent in the map — the board's
 *  "spent" stat. Every task originates from a stack card (the app's one
 *  `createTask` entry point), so the whole-map sum is the whole-board spend. */
export function totalCost(agents: ReadonlyMap<string, AgentState>): number {
  let sum = 0;
  for (const agent of agents.values()) sum += agent.cost;
  return sum;
}
