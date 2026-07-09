/**
 * Goal-pursuit core tests — run with `npx tsx src/lib/stores/stackGoal.test.ts`.
 * Pure functions only: no store, no fetch mock, no timers.
 */
import {
  precede,
  isSuccessStop,
  stackStopLabel,
  decideAfterMiss,
  foldGain,
  STACK_GAIN_MARGIN,
  type StackStopReason,
  type GainState
} from './stackGoal';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

// ── precedence mirrors lopi-core StopReason: goal_met > budget > no_progress
//    > max_chain_loops ───────────────────────────────────────────────────────
eqIs(precede('no_progress', 'budget'), 'budget', 'budget outranks no_progress');
eqIs(precede('budget', 'no_progress'), 'budget', 'precede is order-independent');
eqIs(precede('max_chain_loops', 'goal_met'), 'goal_met', 'goal_met outranks everything');
eqIs(precede('no_progress', 'max_chain_loops'), 'no_progress', 'no_progress outranks the ceiling backstop');
eqIs(precede('budget', 'budget'), 'budget', 'same reason is idempotent');

// ── success predicate: only goal_met is a success ────────────────────────────
ok(isSuccessStop('goal_met'), 'goal_met is a successful stop');
for (const r of ['budget', 'no_progress', 'max_chain_loops'] as StackStopReason[]) {
  ok(!isSuccessStop(r), `${r} is not a successful stop`);
}

// ── every reason has a distinct, non-empty label ─────────────────────────────
{
  const labels = (['goal_met', 'budget', 'no_progress', 'max_chain_loops'] as StackStopReason[]).map(
    stackStopLabel
  );
  ok(labels.every((l) => l.length > 0), 'every stop reason renders a non-empty label');
  ok(new Set(labels).size === labels.length, 'each stop reason renders a distinct label');
}

// ── decideAfterMiss: keep re-running until a cap trips ───────────────────────
eq(
  decideAfterMiss({ chainRun: 1, maxChainLoops: 3, noGainStreak: 0, noProgressLimit: 3 }),
  { kind: 'rerun' },
  'below every cap → re-run the chain'
);
eq(
  decideAfterMiss({ chainRun: 3, maxChainLoops: 3, noGainStreak: 0, noProgressLimit: 3 }),
  { kind: 'stop', reason: 'max_chain_loops' },
  'reaching the chain-loop ceiling stops with max_chain_loops'
);
eq(
  decideAfterMiss({ chainRun: 5, maxChainLoops: 0, noGainStreak: 2, noProgressLimit: 3 }),
  { kind: 'rerun' },
  'an infinite (0) ceiling never trips max_chain_loops on its own'
);
eq(
  decideAfterMiss({ chainRun: 2, maxChainLoops: 0, noGainStreak: 3, noProgressLimit: 3 }),
  { kind: 'stop', reason: 'no_progress' },
  'a stalled infinite-ceiling stack stops with no_progress'
);
eq(
  decideAfterMiss({ chainRun: 3, maxChainLoops: 3, noGainStreak: 3, noProgressLimit: 3 }),
  { kind: 'stop', reason: 'no_progress' },
  'when both caps trip together, the higher-precedence no_progress is reported (specific, not generic)'
);
eq(
  decideAfterMiss({ chainRun: 5, maxChainLoops: 0, noGainStreak: 5, noProgressLimit: 0 }),
  { kind: 'rerun' },
  'noProgressLimit 0 disables the no-progress detector'
);

// ── foldGain: reuse A3's gain margin at stack scope ──────────────────────────
{
  const start: GainState = { best: undefined, streak: 0 };
  const first = foldGain(start, 0.5);
  eq(first, { best: 0.5, streak: 0 }, 'the first observed score seeds the best with a zero streak');

  const gained = foldGain(first, 0.5 + STACK_GAIN_MARGIN);
  eq(gained, { best: 0.5 + STACK_GAIN_MARGIN, streak: 0 }, 'beating best by the margin counts as progress (streak resets)');

  const stalled = foldGain(gained, gained.best!);
  eqIs(stalled.streak, 1, 'a score that only ties the best is not progress — streak increments');
  eqIs(stalled.best, gained.best, 'a non-gaining chain-run leaves the best untouched');

  const regressed = foldGain(stalled, 0.1);
  eqIs(regressed.streak, 2, 'a regression increments the streak again');

  const unobservable = foldGain(regressed, undefined);
  eq(unobservable, regressed, 'an unobservable (undefined) score changes neither best nor streak');
}

namedSummary('stackGoal');
