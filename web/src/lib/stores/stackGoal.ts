/**
 * B1 — goal-directed stacks: the pure decision core the client sequencer
 * (`stores/stackRun.ts`) drives.
 *
 * A stack stops being "run the chain ×N" and becomes "run the chain until its
 * acceptance passes, or a stack-level stop reason fires." This module holds the
 * termination *policy* as pure, side-effect-free functions so it unit-tests
 * without a store, a fetch mock, or a timer — the sequencer only supplies the
 * observed outcome of each chain-run.
 *
 * The stop-reason vocabulary mirrors the backend's A3 termination
 * (`crates/lopi-core/src/stop_reason.rs`) at *chain* scope: the loop-scope
 * `max_iterations` backstop is re-cast as `max_chain_loops` (the ceiling on how
 * many times the whole chain re-runs while pursuing the goal), and the same
 * precedence — `goal_met > budget > no_progress > max_chain_loops` — decides
 * which reason "wins" when more than one condition trips together.
 */

/** A chain-scope stop reason. Mirrors `lopi_core::StopReason`
 *  (`goal_met`/`budget`/`no_progress`/`max_iterations`) with the loop-scope
 *  `max_iterations` re-cast as the chain-scope `max_chain_loops`. The wire
 *  strings match the backend's `StopReason::as_str` so a future server-side
 *  stack outcome can reuse them verbatim. */
export type StackStopReason = 'goal_met' | 'budget' | 'no_progress' | 'max_chain_loops';

/** Precedence rank, higher wins — mirrors `StopReason::rank`
 *  (`goal_met` 3 > `budget` 2 > `no_progress` 1 > `max_chain_loops` 0). */
const RANK: Record<StackStopReason, number> = {
  max_chain_loops: 0,
  no_progress: 1,
  budget: 2,
  goal_met: 3
};

/** The higher-precedence of two reasons — the one that "wins" when both trip in
 *  the same chain-run. Mirrors `StopReason::precede`. */
export function precede(a: StackStopReason, b: StackStopReason): StackStopReason {
  return RANK[b] > RANK[a] ? b : a;
}

/** Whether a stop reason represents a *successful* termination (the goal was
 *  met) as opposed to a resource/progress cutoff. Mirrors
 *  `StopReason::is_success`. */
export function isSuccessStop(reason: StackStopReason): boolean {
  return reason === 'goal_met';
}

/** A short, human-readable line for the recorded stop reason — what the dock
 *  renders when a goal run halts, so "done" is told apart from "gave up". */
export function stackStopLabel(reason: StackStopReason): string {
  switch (reason) {
    case 'goal_met':
      return 'goal met — stack acceptance passed';
    case 'budget':
      return 'stopped — stack budget exhausted';
    case 'no_progress':
      return 'stopped — no progress across chain re-runs';
    case 'max_chain_loops':
      return 'stopped — reached the chain-loop ceiling without meeting the goal';
  }
}

/** The margin a chain-run's stack-eval score must beat the best-so-far by to
 *  count as progress — mirrors A3's objective `GainRule.margin`
 *  (`crates/lopi-core/src/gain.rs`). At or below it, the chain-run "did not
 *  gain" and feeds the no-progress streak. */
export const STACK_GAIN_MARGIN = 0.01;

/** The live goal-pursuit counters the sequencer threads across chain-runs. */
export interface GoalPursuit {
  /** Completed chain-runs so far (`1` after the first chain-run finishes). */
  chainRun: number;
  /** Ceiling on chain re-runs; `0` = infinite (the same sentinel `loopCount`
   *  uses elsewhere). */
  maxChainLoops: number;
  /** Consecutive chain-runs that did not gain on the stack metric. */
  noGainStreak: number;
  /** Consecutive non-gaining chain-runs tolerated before a `no_progress` stop;
   *  `0` disables the no-progress detector. */
  noProgressLimit: number;
}

/** What the sequencer should do after a chain-run whose stack acceptance did
 *  not pass. */
export type GoalDecision = { kind: 'rerun' } | { kind: 'stop'; reason: StackStopReason };

/** Decide what to do after a chain-run whose stack acceptance did **not** pass
 *  (`goal_met` is handled by the caller, before this is ever reached).
 *
 *  Both caps are checked and the higher-precedence one is reported, so the
 *  recorded reason is *specific* — `no_progress` when the run stalled, rather
 *  than a generic "hit the ceiling". `budget` is intentionally never tripped
 *  here: there is no observable stack-level token meter on the client (the same
 *  honesty stance as Stack-1's unenforced stack budget), so it stays in the
 *  precedence for when a real meter lands but never fires client-side today. */
export function decideAfterMiss(p: GoalPursuit): GoalDecision {
  const tripped: StackStopReason[] = [];
  if (p.maxChainLoops !== 0 && p.chainRun >= p.maxChainLoops) tripped.push('max_chain_loops');
  if (p.noProgressLimit !== 0 && p.noGainStreak >= p.noProgressLimit) tripped.push('no_progress');
  if (tripped.length === 0) return { kind: 'rerun' };
  return { kind: 'stop', reason: tripped.reduce(precede) };
}

/** The best-score + no-gain-streak carried between chain-runs. */
export interface GainState {
  best: number | undefined;
  streak: number;
}

/** Fold a completed chain-run's stack-eval score into the no-gain streak,
 *  reusing A3's gain idea at stack scope. A score at or above `best + margin`
 *  is progress (resets the streak, raises the best); anything less increments
 *  the streak. An `undefined` score — the eval produced no observable scalar —
 *  leaves both unchanged: an unobservable result is counted as neither progress
 *  nor a stall (don't fake a signal that isn't there). */
export function foldGain(prev: GainState, score: number | undefined): GainState {
  if (score === undefined) return prev;
  if (prev.best === undefined || score >= prev.best + STACK_GAIN_MARGIN) {
    return { best: score, streak: 0 };
  }
  return { best: prev.best, streak: prev.streak + 1 };
}
