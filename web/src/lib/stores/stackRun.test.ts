/**
 * Stack-run sequencer tests — run with `npx tsx src/lib/stores/stackRun.test.ts`.
 * Mocks global `fetch` (same pattern as `api.test.ts`) and substitutes a
 * plain `writable(new Map())` for the live `agents` store, per
 * `stackRun.ts`'s own doc comment on why it takes `statusSource` as a
 * parameter instead of importing `./agents` directly.
 *
 * The fetch mock resolves each `POST /api/tasks` synchronously and, in the
 * same synchronous tick, pushes that card's pre-arranged terminal status
 * into the fake `statusSource` — keyed by `client_ref` (== card id, per
 * `cardToTaskPayload`), which the mock echoes back as the task id. That
 * makes `waitForTerminal`'s subscribe resolve immediately (Svelte stores
 * invoke a new subscriber synchronously with the current value), so a
 * whole card's launch-and-complete cycle settles within a few microtask
 * turns — no timers, no real backend, no new test-runner dependency.
 *
 * Pause/drain/bump are exercised by calling them from *inside* the fetch
 * mock for a specific card — i.e. "the user clicked pause while this card
 * was in flight" — since that's the only deterministic way to interrupt a
 * synchronously-resolving mock mid-run.
 */
import { get, writable } from 'svelte/store';
import {
  runStack,
  pauseStack,
  resumeStack,
  drainStack,
  bumpCard,
  scheduleStack,
  runs,
  type AgentStatusSource
} from './stackRun';
import { panes, buildCard, type StackCard, type PaneDefaults } from './stack';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

function card(id: string, goal = id): StackCard {
  return { ...buildCard(`"${goal}"`), id };
}

const defaults: PaneDefaults = { model: 'm', effort: 'e', repo: 'r' };

interface Captured {
  path: string;
  body: Record<string, unknown>;
}

type StatusStore = ReturnType<typeof writable<Map<string, { status?: string }>>>;

/** Arms a fetch mock where each card id's terminal outcome is pre-decided.
 * A card whose id is missing from `outcomes` never resolves (simulating
 * "still running") — cards after it in the plan should then never launch. */
function mockBackend(
  statusSource: StatusStore,
  outcomes: Record<string, 'completed' | 'failed' | 'cancelled'>,
  onTaskCreate?: (cardId: string) => void
): Captured[] {
  const captured: Captured[] = [];
  (globalThis as { fetch: unknown }).fetch = (path: string, init?: RequestInit) => {
    const body = init?.body ? JSON.parse(String(init.body)) : {};
    captured.push({ path, body });
    if (path === '/api/tasks') {
      const cardId = body.client_ref as string;
      onTaskCreate?.(cardId);
      const outcome = outcomes[cardId];
      if (outcome) {
        statusSource.update((m) => {
          const next = new Map(m);
          next.set(cardId, { status: outcome });
          return next;
        });
      }
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () =>
          Promise.resolve({
            id: cardId,
            goal: body.goal,
            queued: true,
            duplicate_of: null,
            client_ref: cardId
          })
      });
    }
    if (path === '/api/schedules') {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ id: 'sched-1', ...body })
      });
    }
    return Promise.resolve({ ok: false, status: 404, json: () => Promise.resolve({ error: 'unmocked path' }) });
  };
  return captured;
}

function seedPane(key: string, cards: StackCard[]): void {
  panes.update((state) => state.map((p) => (p.key === key ? { ...p, cards } : p)));
}

function resetPanes(): void {
  panes.set([
    { key: 's1', title: 'stack one', cards: [] },
    { key: 's2', title: 'stack two', cards: [] }
  ]);
  runs.set(new Map());
}

/** N turns of the microtask queue — enough for a chain of
 * `await createTask` / `await waitForTerminal` / store updates to settle
 * across a handful of cards. */
async function flush(turns = 30): Promise<void> {
  for (let i = 0; i < turns; i++) await Promise.resolve();
}

function runState(paneKey: string) {
  return get(runs).get(paneKey);
}

function paneCardIds(paneKey: string): string[] {
  return get(panes).find((p) => p.key === paneKey)!.cards.map((c) => c.id);
}

async function main() {
  // ── ordering: a 3-card stack runs bottom-to-top (execution order) ─────────
  {
    resetPanes();
    // Composer prepends, so pane.cards is newest-first: [c, b, a] means
    // execution order is [a, b, c].
    seedPane('s1', [card('c'), card('b'), card('a')]);
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', b: 'completed', c: 'completed' });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush();

    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 'b', 'c'],
      '3-card stack launches in execution order (bottom of stack first)'
    );
    eqIs(runState('s1')?.phase, 'done', 'a fully-successful run ends in phase "done"');
    eqIs(runState('s1')?.cursor, 3, 'the cursor lands past the end of the plan once every card is done');
  }

  // ── a failing gate on card 2 halts the stack — card 3 never launches ─────
  {
    resetPanes();
    seedPane('s1', [card('c'), card('b'), card('a')]);
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', b: 'failed' });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush();

    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 'b'],
      'a failing second card stops the run before the third card ever launches'
    );
    eqIs(runState('s1')?.phase, 'error', 'a card ending failed puts the whole run into phase "error"');
    ok(!!runState('s1')?.error?.includes('failed'), 'the error message names the outcome');
  }

  // ── pause: halts after the in-flight card finishes, not mid-card ─────────
  {
    resetPanes();
    seedPane('s1', [card('c'), card('b'), card('a')]);
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', b: 'completed', c: 'completed' }, (cardId) => {
      if (cardId === 'a') pauseStack('s1');
    });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush();

    eq(captured.map((c) => c.body.client_ref), ['a'], 'pausing while card "a" is in flight lets it finish, then halts before "b"');
    eqIs(runState('s1')?.phase, 'paused', 'phase is "paused", not "done" or "error"');
    eqIs(runState('s1')?.cursor, 1, 'the cursor advanced past the finished card before halting');

    resumeStack('s1', defaults, statusSource as AgentStatusSource);
    await flush();

    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 'b', 'c'],
      'resuming a paused run continues launching the remaining cards'
    );
    eqIs(runState('s1')?.phase, 'done', 'resuming through to the end reaches phase "done"');
  }

  // ── drain: finishes the in-flight card, then stops for good (no resume) ──
  {
    resetPanes();
    seedPane('s1', [card('c'), card('b'), card('a')]);
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', b: 'completed', c: 'completed' }, (cardId) => {
      if (cardId === 'a') drainStack('s1');
    });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush();

    eq(captured.map((c) => c.body.client_ref), ['a'], 'draining while card "a" is in flight lets it finish, then stops');
    eqIs(runState('s1')?.phase, 'done', 'a drained run finalizes to phase "done", not "paused"');

    resumeStack('s1', defaults, statusSource as AgentStatusSource);
    await flush();
    eq(
      captured.map((c) => c.body.client_ref),
      ['a'],
      'resumeStack is a no-op on a drained (done) run — draining is not resumable'
    );
  }

  // ── bumpCard: reorders a queued card and reflects it into the pane too ───
  // Uses 4 cards, not 3: bumpInOrder (see stack.test.ts) reserves the
  // cursor's own slot — the card that's up next — so it can never be
  // swapped into or out of; only cards further back in the queue than that
  // can trade places. A 3-card stack paused after card 1 leaves only one
  // queued card free to move, which is exactly the untouchable slot.
  {
    resetPanes();
    seedPane('s1', [card('d'), card('c'), card('b'), card('a')]);
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(
      statusSource,
      { a: 'completed', b: 'completed', c: 'completed', d: 'completed' },
      (cardId) => {
        if (cardId === 'a') pauseStack('s1');
      }
    );

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush();
    eqIs(runState('s1')?.phase, 'paused', 'sanity: paused after "a" before exercising bump');

    const bumped = bumpCard('s1', 'd', 'up');
    ok(bumped.ok, 'bumping the queued "d" up (ahead of "c", not touching next-up "b") succeeds');
    eq(runState('s1')?.order, ['a', 'b', 'd', 'c'], "the run's own plan reflects the swap");
    eq(paneCardIds('s1'), ['c', 'd', 'b', 'a'], "the pane's rendered card order reflects the same swap");

    resumeStack('s1', defaults, statusSource as AgentStatusSource);
    await flush();
    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 'b', 'd', 'c'],
      'the bumped order is what actually launches, not the original plan'
    );
  }

  // ── bumpCard rejects illegal transitions instead of a silent no-op ───────
  {
    resetPanes();
    seedPane('s1', [card('c'), card('b'), card('a')]);
    const statusSource: StatusStore = writable(new Map());
    mockBackend(statusSource, {}, (cardId) => {
      if (cardId === 'a') pauseStack('s1');
    });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush();

    const bumpRunning = bumpCard('s1', 'a', 'down');
    ok(!bumpRunning.ok, 'bumping the already-run card "a" is rejected, not silently ignored');

    const bumpNoRun = bumpCard('s2', 'x', 'up');
    ok(!bumpNoRun.ok, 'bumping in a pane with no active run is rejected');
  }

  // ── scheduleStack: honest about only scheduling the first card ──────────
  {
    resetPanes();
    seedPane('s1', [card('c'), card('b'), card('a')]);
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, {});

    const result = await scheduleStack('s1', '0 * * * *', defaults);
    ok(result.ok, 'scheduleStack succeeds when createSchedule succeeds');
    eqIs(result.scheduledCardId, 'a', 'only the bottom-of-stack (first-to-run) card is actually scheduled');
    eq(result.skippedCardIds, ['b', 'c'], 'every other card is reported back as skipped, not silently dropped');
    eqIs(captured.length, 1, 'exactly one POST /api/schedules is made, not one per card');
    eqIs(captured[0]?.path, '/api/schedules', 'scheduleStack hits the real schedules endpoint');
  }

  namedSummary('stackRun');
}

main();
