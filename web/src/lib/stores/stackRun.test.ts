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
  runBarePane,
  pauseStack,
  resumeStack,
  drainStack,
  bumpCard,
  bumpUiState,
  scheduleStack,
  runs,
  type AgentStatusSource,
  type StackRunState
} from './stackRun';
import {
  panes,
  buildCard,
  defaultStackConfig,
  makeDraft,
  BASELINE_EVAL,
  type StackCard,
  type PaneDefaults,
  type StackConfig
} from './stack';
import { AUTO_MODEL } from './options';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

function card(id: string, goal = id): StackCard {
  return { ...buildCard(`"${goal}"`), id };
}

const defaults: PaneDefaults = { model: 'm', effort: 'e', repo: 'r' };

/** A minimal `StackRunState` for `bumpUiState` unit tests — pure
 *  construction, no `runStack`/mock-backend machinery needed since
 *  `bumpUiState` only reads `phase`/`order`/`cursor`. */
function runStateFixture(patch: Partial<StackRunState>): StackRunState {
  return {
    paneKey: 's1',
    phase: 'running',
    intent: 'run',
    order: ['a', 'b', 'c', 'd'],
    cursor: 1,
    repetition: 0,
    loopTarget: 1,
    onFail: 'stop',
    hadFailure: false,
    noProgressLimit: 0,
    noGainStreak: 0,
    ...patch
  };
}

interface Captured {
  path: string;
  body: Record<string, unknown>;
}

type StatusStore = ReturnType<typeof writable<Map<string, { status?: string; score?: number }>>>;

/** Arms a fetch mock where each card id's terminal outcome is pre-decided.
 * A card whose id is missing from `outcomes` never resolves (simulating
 * "still running") — cards after it in the plan should then never launch. */
function mockBackend(
  statusSource: StatusStore,
  outcomes: Record<string, 'completed' | 'failed' | 'cancelled'>,
  onTaskCreate?: (cardId: string) => void,
  scores?: Record<string, number>
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
          next.set(cardId, { status: outcome, ...(scores && cardId in scores ? { score: scores[cardId] } : {}) });
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

/** Patch a pane's stack-level config (loop count / chain on-fail) for the
 *  chain-loop tests — a thin test-local helper, not exported by
 *  `stores/stack.ts` since production code always goes through
 *  `updateStackConfig`. */
function seedStackConfig(key: string, patch: Partial<StackConfig>): void {
  panes.update((state) => state.map((p) => (p.key === key ? { ...p, config: { ...p.config, ...patch } } : p)));
}

function resetPanes(): void {
  panes.set([
    { key: 's1', title: 'stack one', cards: [], config: defaultStackConfig(), draft: makeDraft() },
    { key: 's2', title: 'stack two', cards: [], config: defaultStackConfig(), draft: makeDraft() }
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

  // ── bumpUiState: the pure predicate the bump-button UI renders from ──────
  // order = [a, b, c, d], cursor = 1 (a done, b is next up — the untouchable
  // cursor slot, same reservation `bumpInOrder` enforces).
  {
    eq(
      bumpUiState(undefined, 'a'),
      { visible: false, canSooner: false, canLater: false },
      'no active run for the pane ⇒ never bumpable'
    );
    eq(
      bumpUiState(runStateFixture({ phase: 'done' }), 'c'),
      { visible: false, canSooner: false, canLater: false },
      'a finished run is never bumpable, even for a card still in its order'
    );
    eq(
      bumpUiState(runStateFixture({}), 'a'),
      { visible: false, canSooner: false, canLater: false },
      'the already-run card (at/before the cursor) is never bumpable'
    );
    eq(
      bumpUiState(runStateFixture({}), 'b'),
      { visible: false, canSooner: false, canLater: false },
      "the cursor's own next-up slot is never bumpable — matches bumpInOrder's reservation"
    );
    eq(
      bumpUiState(runStateFixture({}), 'c'),
      { visible: true, canSooner: false, canLater: true },
      'c is bumpable later but not sooner — moving sooner would land on the reserved cursor slot'
    );
    eq(
      bumpUiState(runStateFixture({}), 'd'),
      { visible: true, canSooner: true, canLater: false },
      'd (the last card) is bumpable sooner but not later — nowhere further back to go'
    );
    for (const phase of ['paused', 'draining'] as const) {
      eq(
        bumpUiState(runStateFixture({ phase }), 'c').visible,
        true,
        `${phase} is still an active run — bumpable, same as running`
      );
    }
    for (const phase of ['idle', 'error'] as const) {
      eq(
        bumpUiState(runStateFixture({ phase }), 'c').visible,
        false,
        `${phase} is not an active run — never bumpable`
      );
    }
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

  // ── chain loop: a 3-card stack looped ×2 runs 6 launches in order ───────
  {
    resetPanes();
    seedPane('s1', [card('c'), card('b'), card('a')]);
    seedStackConfig('s1', { loopCount: 2 });
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', b: 'completed', c: 'completed' });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush(60);

    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 'b', 'c', 'a', 'b', 'c'],
      'a 3-loop chain looped ×2 launches every card twice, in the same order each pass'
    );
    eqIs(runState('s1')?.phase, 'done', 'a fully-successful ×2 chain ends in phase "done"');
    eqIs(runState('s1')?.repetition, 1, 'the run settles on the second (index 1) repetition');
  }

  // ── chain on-fail: 'stop' (default) halts the whole chain, even mid-plan ─
  {
    resetPanes();
    seedPane('s1', [card('c'), card('b'), card('a')]);
    seedStackConfig('s1', { loopCount: 3 });
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', b: 'failed' });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush();

    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 'b'],
      "on-fail 'stop' halts immediately — card 'c' and every later repetition never launch"
    );
    eqIs(runState('s1')?.phase, 'error', "a halted chain ends in phase 'error'");
  }

  // ── chain on-fail: 'continue' skips the failed card, keeps looping ──────
  {
    resetPanes();
    seedPane('s1', [card('c'), card('b'), card('a')]);
    seedStackConfig('s1', { loopCount: 2, guardrails: { onFail: 'continue', budget: 'auto' } });
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', b: 'failed', c: 'completed' });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush(60);

    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 'b', 'c', 'a', 'b', 'c'],
      "on-fail 'continue' presses past the failed card within each pass and still repeats the chain"
    );
    eqIs(runState('s1')?.phase, 'error', "the run still reports 'error' overall — a failure happened, it just didn't stop the chain");
  }

  // ── chain on-fail: 'backoff' ends the pass early, still tries next rep ──
  {
    resetPanes();
    seedPane('s1', [card('c'), card('b'), card('a')]);
    seedStackConfig('s1', { loopCount: 2, guardrails: { onFail: 'backoff', budget: 'auto' } });
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', b: 'failed', c: 'completed' });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush();

    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 'b', 'a', 'b'],
      "on-fail 'backoff' skips the rest of a failed pass ('c' never launches that pass) but still attempts the next repetition"
    );
    eqIs(runState('s1')?.phase, 'error', "a chain that never once completed a full clean pass still reports 'error'");
  }

  // ── B1 run-until-goal: re-runs the chain until the stack acceptance passes ─
  {
    resetPanes();
    seedPane('s1', [card('b'), card('a')]);
    // Infinite chain-loop ceiling (0) so only the goal decides when to stop;
    // acceptance beyond baseline + pursue on → goal pursuit engaged.
    seedStackConfig('s1', {
      loopCount: 0,
      evals: [BASELINE_EVAL, { name: 'tests pass', tier: 'test' }],
      goal: { pursue: true, noProgressLimit: 3 }
    });
    const statusSource: StatusStore = writable(new Map());
    // Cards always complete; the first chain-run's stack eval fails, the
    // second passes → goal met on the second chain-run.
    const captured = mockBackend(statusSource, {
      a: 'completed',
      b: 'completed',
      's1::stack-eval::0': 'failed',
      's1::stack-eval::1': 'completed'
    });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush(80);

    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 'b', 's1::stack-eval::0', 'a', 'b', 's1::stack-eval::1'],
      'the chain re-runs, evaluating the stack acceptance after each chain-run, until the goal passes'
    );
    eqIs(runState('s1')?.phase, 'done', 'meeting the goal ends the run in phase "done"');
    eqIs(runState('s1')?.stopReason, 'goal_met', 'the specific stop reason recorded is goal_met');
  }

  // ── B1: the stack-eval task carries the compiled acceptance + a single attempt
  {
    resetPanes();
    seedPane('s1', [card('a')]);
    seedStackConfig('s1', {
      loopCount: 1,
      evals: [BASELINE_EVAL, { name: 'tests pass', tier: 'test' }],
      goal: { pursue: true, noProgressLimit: 3 }
    });
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', 's1::stack-eval::0': 'completed' });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush(80);

    const evalPost = captured.find((c) => c.body.client_ref === 's1::stack-eval::0');
    ok(!!evalPost, 'a dedicated stack-acceptance eval task is launched after the chain');
    ok(!!evalPost?.body.acceptance, 'the eval task carries the compiled stack acceptance (A1 executor at stack scope)');
    eqIs(evalPost?.body.max_iterations, 1, 'the stack eval is a single verification attempt, not more work');
    eqIs(runState('s1')?.stopReason, 'goal_met', 'a passing single-run stack meets its goal');
  }

  // ── auto model: the stack-eval launch omits `model` for a pane default of
  // `auto`, same as the per-card run-stack path — a regression here would
  // send the literal string "auto" straight to the CLI and fail. ──────────
  {
    resetPanes();
    seedPane('s1', [card('a')]);
    seedStackConfig('s1', {
      loopCount: 1,
      evals: [BASELINE_EVAL, { name: 'tests pass', tier: 'test' }],
      goal: { pursue: true, noProgressLimit: 3 }
    });
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed', 's1::stack-eval::0': 'completed' });
    const autoDefaults: PaneDefaults = { model: AUTO_MODEL, effort: 'e', repo: 'r' };

    runStack('s1', 'run', autoDefaults, statusSource as AgentStatusSource);
    await flush(80);

    const evalPost = captured.find((c) => c.body.client_ref === 's1::stack-eval::0');
    ok(!!evalPost, 'the stack-eval task still launches under an auto pane default');
    eqIs(evalPost?.body.model, undefined, 'auto pane default omits model on the stack-eval payload, not the literal string');
  }

  // ── B1: halts on the chain-loop ceiling with the specific reason ─────────
  {
    resetPanes();
    seedPane('s1', [card('a')]);
    seedStackConfig('s1', {
      loopCount: 2,
      evals: [BASELINE_EVAL, { name: 'tests pass', tier: 'test' }],
      goal: { pursue: true, noProgressLimit: 5 }
    });
    const statusSource: StatusStore = writable(new Map());
    // Goal never met — every stack eval fails, no observable score.
    const captured = mockBackend(statusSource, {
      a: 'completed',
      's1::stack-eval::0': 'failed',
      's1::stack-eval::1': 'failed'
    });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush(80);

    eq(
      captured.map((c) => c.body.client_ref),
      ['a', 's1::stack-eval::0', 'a', 's1::stack-eval::1'],
      'the chain re-runs up to the loopCount ceiling, evaluating each time'
    );
    eqIs(runState('s1')?.phase, 'error', 'giving up without meeting the goal ends in phase "error"');
    eqIs(runState('s1')?.stopReason, 'max_chain_loops', 'the recorded reason is the specific max_chain_loops, not a generic stop');
  }

  // ── B1: halts on no-progress when the stack-eval score stops gaining ─────
  {
    resetPanes();
    seedPane('s1', [card('a')]);
    seedStackConfig('s1', {
      loopCount: 0, // infinite ceiling — only no-progress can stop it here
      evals: [BASELINE_EVAL, { name: 'tests pass', tier: 'test' }],
      goal: { pursue: true, noProgressLimit: 2 }
    });
    const statusSource: StatusStore = writable(new Map());
    // Every stack eval fails with the *same* score → no gain across re-runs.
    const captured = mockBackend(
      statusSource,
      {
        a: 'completed',
        's1::stack-eval::0': 'failed',
        's1::stack-eval::1': 'failed',
        's1::stack-eval::2': 'failed'
      },
      undefined,
      { 's1::stack-eval::0': 0.5, 's1::stack-eval::1': 0.5, 's1::stack-eval::2': 0.5 }
    );

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush(120);

    eqIs(runState('s1')?.stopReason, 'no_progress', 'a stack whose eval score stops gaining halts with no_progress, not an endless loop');
    eqIs(runState('s1')?.phase, 'error', 'a no-progress halt is not a success');
  }

  // ── B1: "Run once" never pursues a goal — one pass, no stack eval ────────
  {
    resetPanes();
    seedPane('s1', [card('a')]);
    seedStackConfig('s1', {
      loopCount: 1,
      evals: [BASELINE_EVAL, { name: 'tests pass', tier: 'test' }],
      goal: { pursue: true, noProgressLimit: 3 }
    });
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed' });

    runStack('s1', 'run-once', defaults, statusSource as AgentStatusSource);
    await flush(60);

    eq(
      captured.map((c) => c.body.client_ref),
      ['a'],
      '"Run once" runs the chain a single pass and launches no stack-acceptance eval, even with a goal set'
    );
    eqIs(runState('s1')?.phase, 'done', 'a clean run-once ends done with no goal pursuit');
    eqIs(runState('s1')?.stopReason, undefined, 'no stop reason is recorded when no goal was pursued');
  }

  // ── B1: pursue on but only baseline acceptance → not a real goal, legacy ─
  {
    resetPanes();
    seedPane('s1', [card('a')]);
    // pursue is on, but evals are baseline-only (nothing to check) → inert;
    // the run must fall back to the legacy fixed-loopCount behavior.
    seedStackConfig('s1', { loopCount: 1, goal: { pursue: true, noProgressLimit: 3 } });
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, { a: 'completed' });

    runStack('s1', 'run', defaults, statusSource as AgentStatusSource);
    await flush(60);

    eq(
      captured.map((c) => c.body.client_ref),
      ['a'],
      'pursue with baseline-only acceptance launches no eval — an inert goal falls back to legacy behavior'
    );
    eqIs(runState('s1')?.stopReason, undefined, 'no goal pursued → no stop reason recorded (backward-compatible)');
  }

  // ── Verify-1 F2: the bare-pane launch. A 0-or-1-card pane has no dock, so
  //    `runBarePane` is its run affordance. It submits the single card through
  //    the loop-semantics-free `paneSubmitPayload` (no client_ref / max_iterations
  //    / on_fail) and wires taskId + terminal status onto the card.
  {
    resetPanes();
    seedPane('s1', [card('c1', 'summarize main.rs')]);
    const statusSource: StatusStore = writable(new Map());
    const TASK_ID = 'bare-task-1';
    const captured: Captured[] = [];
    (globalThis as { fetch: unknown }).fetch = (path: string, init?: RequestInit) => {
      const body = init?.body ? JSON.parse(String(init.body)) : {};
      captured.push({ path, body });
      statusSource.update((m) => {
        const next = new Map(m);
        next.set(TASK_ID, { status: 'completed' });
        return next;
      });
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () =>
          Promise.resolve({ id: TASK_ID, goal: body.goal, queued: true, duplicate_of: null, client_ref: null })
      });
    };

    runBarePane('s1', defaults, statusSource as AgentStatusSource);
    await flush();

    const posts = captured.filter((c) => c.path === '/api/tasks');
    eqIs(posts.length, 1, 'bare pane launches exactly one task via createTask (F2)');
    const b = posts[0].body;
    eqIs(b.goal, 'summarize main.rs', 'bare payload carries the card goal');
    eqIs(b.repo, 'r', 'bare payload falls back to the pane default repo');
    ok(
      b.max_iterations === undefined && b.on_fail === undefined && b.client_ref === undefined,
      'bare payload omits stack-loop semantics (paneSubmitPayload, not cardToTaskPayload)'
    );
    eqIs(runState('s1')?.phase, 'done', 'bare run reaches a terminal done phase');
    const wired = get(panes).find((p) => p.key === 's1')!.cards[0];
    eqIs(wired.status, 'done', 'the card is marked done');
    eqIs(wired.taskId, TASK_ID, 'the card carries the launched task id (orb/output can render)');
  }

  {
    // A pane with 0 cards, or with 2+ (a real stack, which has the dock), is a
    // no-op for runBarePane — it only handles the single-card bare case.
    resetPanes();
    const statusSource: StatusStore = writable(new Map());
    const captured = mockBackend(statusSource, {});
    runBarePane('s1', defaults, statusSource as AgentStatusSource); // 0 cards
    await flush();
    seedPane('s2', [card('x'), card('y')]);
    runBarePane('s2', defaults, statusSource as AgentStatusSource); // 2 cards
    await flush();
    eqIs(captured.length, 0, 'runBarePane is a no-op for 0-card and 2+-card panes');
  }

  namedSummary('stackRun');
}

main();
