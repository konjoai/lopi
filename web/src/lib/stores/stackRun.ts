/**
 * Client-side stack-run sequencer (Backend-1 Phase 2 + Phase 4).
 *
 * There is no server-side "stack"/"plan" concept — stacks are 100%
 * client-only (confirmed by the UI-2 V&V audit). So "run a stack" is a
 * small TS state machine that submits one card's task at a time via the
 * real `createTask`, waits for it to reach a terminal status through the
 * already-existing `agents` store (itself fed by the app's one global
 * WebSocket + event reducer), and only checks pause/drain state *between*
 * cards. That gives "pause = halt after the current iteration completes"
 * and "drain = let the current loop finish, then stop" for free, without
 * needing the pool to interrupt a running task's internal retry loop —
 * which the Backend-1 pre-flight found it can't do mid-subprocess anyway.
 *
 * This module deliberately does NOT import `./agents` directly — that
 * module pulls in `$app/environment` (a SvelteKit virtual module unusable
 * outside a Vite build, e.g. under the `tsx` runner this repo's other
 * store tests use). Instead, callers pass their already-live `agents`
 * store in as `statusSource` — any object shaped like a Svelte
 * `Readable<Map<string, {status?: string}>>` works, which is exactly what
 * lets unit tests substitute a plain `writable(new Map())` in place of the
 * real WebSocket-fed store.
 */
import { get, writable, type Readable } from 'svelte/store';
import {
  createTask,
  createSchedule,
  effectiveTaskId,
  type ScheduleBody,
  type CreateTaskOptions,
  type Acceptance
} from '$lib/api';
import {
  panes,
  updateCardInPane,
  reorderInPane,
  executionOrder,
  cardToTaskPayload,
  cardToTaskPayloadForRunOnce,
  paneSubmitPayload,
  evalsToAcceptance,
  stackPursuesGoal,
  bumpInOrder,
  type StackCard,
  type PaneDefaults,
  type OnFail
} from './stack';
import { decideAfterMiss, foldGain, stackStopLabel, type StackStopReason } from './stackGoal';
import { AUTO_MODEL } from './options';

/** Which run-menu action started this run — governs the payload each card
 *  submits (`cardToTaskPayload` vs. the max-iterations-forced-to-1 variant). */
export type RunIntent = 'run' | 'run-once';

/** A run's lifecycle. `paused` is resumable (the next card just hasn't
 *  launched yet); `draining` finalizes to `done` once the in-flight card's
 *  wait resolves, and does not resume. */
export type RunPhase = 'idle' | 'running' | 'paused' | 'draining' | 'done' | 'error';

/** One pane's active (or just-finished) run. `order`/`cursor` are a
 *  snapshot of execution order taken at launch time — mutating a pane's
 *  cards mid-run (e.g. `bumpCard`) updates `order` but never re-derives it
 *  from the pane, so a run's plan stays stable even if the composer adds
 *  more cards while it's mid-flight.
 *
 *  `loopTarget`/`onFail` are likewise snapshotted from `pane.config` at
 *  launch, same reasoning: tweaking the dock's loop-count or on-fail policy
 *  mid-run shouldn't reshuffle a run already in flight. `repetition` counts
 *  completed passes through `order` (`0` during the first pass);
 *  `loopTarget === 0` is the same infinite sentinel `maxIterations` uses
 *  elsewhere in this codebase. `hadFailure` tracks whether any card in any
 *  repetition ended non-`completed`, so a chain that pressed on past a
 *  failure (`onFail: 'continue'`/`'backoff'`) still finishes as `'error'`
 *  overall rather than quietly reporting `'done'`. */
export interface StackRunState {
  paneKey: string;
  phase: RunPhase;
  intent: RunIntent;
  order: string[];
  cursor: number;
  repetition: number;
  loopTarget: number;
  onFail: OnFail;
  hadFailure: boolean;
  error?: string;
  /** B1 — the compiled stack acceptance the chain is pursuing. `undefined`
   *  means "not pursuing a goal": the run keeps the legacy fixed-`loopTarget`
   *  repetition behavior, unchanged and backward-compatible. When set,
   *  `advance` evaluates it after every chain-run and re-runs the whole chain
   *  until it passes (`goal_met`) or a stack stop reason fires. */
  acceptance?: Acceptance;
  /** B1 — consecutive non-gaining chain-runs tolerated before a `no_progress`
   *  stop (`0` disables the detector). Snapshotted from the stack goal facet at
   *  launch, same as `loopTarget`/`onFail`. */
  noProgressLimit: number;
  /** B1 — live no-gain streak across chain-runs (reset on progress). */
  noGainStreak: number;
  /** B1 — best stack-eval score observed so far; `undefined` until the first
   *  chain-run yields an observable scalar. */
  goalBest?: number;
  /** B1 — the specific reason a goal run halted, recorded on the run so the
   *  dock tells `goal_met` apart from `no_progress`/`max_chain_loops`. */
  stopReason?: StackStopReason;
}

/** Active runs, keyed by pane key. Client-only, in-memory — a page reload
 *  loses run state exactly like it loses pane state. */
export const runs = writable<Map<string, StackRunState>>(new Map());

/** The minimal shape `waitForTerminal` needs from a live agent-state store —
 *  satisfied by the real `agents` store (`stores/agents.ts`) and, in tests,
 *  by a plain `writable(new Map())`. */
export type AgentStatusSource = Readable<Map<string, { status?: string; score?: number }>>;

function setRun(paneKey: string, patch: Partial<StackRunState>): void {
  runs.update((m) => {
    const current = m.get(paneKey);
    if (!current) return m;
    const next = new Map(m);
    next.set(paneKey, { ...current, ...patch });
    return next;
  });
}

function findCard(paneKey: string, cardId: string): StackCard | undefined {
  return get(panes)
    .find((p) => p.key === paneKey)
    ?.cards.find((c) => c.id === cardId);
}

/** Resolve once `taskId` reaches a terminal `AgentState.status`, reusing
 *  whatever live status store the caller passes in — no new polling or
 *  transport needed. Deferred unsubscribe (`queueMicrotask`) avoids
 *  referencing `unsub` before its `const` binding completes during a
 *  Svelte store's synchronous initial-subscribe callback. */
function waitForTerminal(
  taskId: string,
  statusSource: AgentStatusSource
): Promise<'completed' | 'failed' | 'cancelled'> {
  return new Promise((resolve) => {
    const unsub = statusSource.subscribe((m) => {
      const status = m.get(taskId)?.status;
      if (status === 'completed' || status === 'failed' || status === 'cancelled') {
        resolve(status as 'completed' | 'failed' | 'cancelled');
        queueMicrotask(() => unsub());
      }
    });
  });
}

/** The driver: launches queued cards one at a time until the run pauses,
 *  drains, errors, or runs out of cards — then, per Stack-1's chain-loop
 *  extension, either starts the next repetition of the same `order` or
 *  finishes for good. Safe to call again after a pause is lifted
 *  (`resumeStack`) — it always re-reads `runs`/`panes` fresh at the top of
 *  each iteration rather than closing over a stale snapshot, which is also
 *  what makes an infinite (`loopTarget === 0`) chain safe: every pass
 *  re-checks pause/drain before doing anything, so it can never spin past a
 *  user's pause/drain request even though it never numerically bounds
 *  itself. */
async function advance(
  paneKey: string,
  defaults: PaneDefaults,
  statusSource: AgentStatusSource
): Promise<void> {
  for (;;) {
    const state = get(runs).get(paneKey);
    if (!state) return;
    if (state.phase === 'paused') return;
    if (state.phase === 'draining') {
      setRun(paneKey, {
        phase: state.hadFailure ? 'error' : 'done',
        error: state.hadFailure ? (state.error ?? 'drained after at least one failed loop') : undefined
      });
      return;
    }
    if (state.phase !== 'running') return;

    if (state.cursor >= state.order.length) {
      // A full pass over the chain (one chain-run) just completed.
      if (state.acceptance) {
        // B1 run-until-goal: evaluate the stack acceptance and either re-run
        // the whole chain or stop with a specific stack-level reason.
        if ((await pursueGoal(paneKey, state, defaults, statusSource)) === 'stop') return;
        continue;
      }
      // Legacy (no goal): fixed-`loopTarget` chain repetition, unchanged.
      const nextRepetition = state.repetition + 1;
      const moreRepetitions = state.loopTarget === 0 || nextRepetition < state.loopTarget;
      if (moreRepetitions) {
        setRun(paneKey, { cursor: 0, repetition: nextRepetition });
        continue;
      }
      setRun(paneKey, {
        phase: state.hadFailure ? 'error' : 'done',
        error: state.hadFailure ? (state.error ?? 'chain completed with at least one failed loop') : undefined
      });
      return;
    }

    const cardId = state.order[state.cursor];
    const card = findCard(paneKey, cardId);
    if (!card) {
      // Card was removed from the pane mid-run — skip it rather than hang
      // the rest of the stack waiting on a task that will never launch.
      setRun(paneKey, { cursor: state.cursor + 1 });
      continue;
    }

    const payload =
      state.intent === 'run-once'
        ? cardToTaskPayloadForRunOnce(card, defaults)
        : cardToTaskPayload(card, defaults);

    updateCardInPane(paneKey, cardId, { status: 'queued' });

    let resp;
    try {
      resp = await createTask(payload.goal, payload.repo, payload.priority, payload.options);
    } catch (err) {
      updateCardInPane(paneKey, cardId, { status: 'idle' });
      setRun(paneKey, {
        phase: 'error',
        hadFailure: true,
        error: `"${card.goal}" failed to launch: ${err instanceof Error ? err.message : String(err)}`
      });
      return;
    }

    const taskId = effectiveTaskId(resp);
    updateCardInPane(paneKey, cardId, { status: 'running', taskId });

    const terminal = await waitForTerminal(taskId, statusSource);
    updateCardInPane(paneKey, cardId, { status: 'done' });

    if (terminal !== 'completed') {
      const error = `"${card.goal}" ended ${terminal}`;
      // Chain-level on-fail (Stack-1), reusing the per-loop `OnFail`
      // vocabulary at chain scope: `stop` halts the whole chain immediately
      // (the pre-Stack-1 hardcoded behavior); `continue` skips past this
      // card to the next one in the same pass; `backoff` ends this pass
      // early (skips its remaining cards) but still attempts the next
      // repetition if one is queued — a failed pass doesn't necessarily
      // kill the whole ×N chain, only itself. All three still leave the
      // run's final phase `'error'` if any card ever failed (`hadFailure`),
      // even when the chain pressed on and technically finished.
      if (state.onFail === 'stop') {
        setRun(paneKey, { phase: 'error', hadFailure: true, error });
        return;
      }
      if (state.onFail === 'continue') {
        setRun(paneKey, { cursor: state.cursor + 1, hadFailure: true, error });
        continue;
      }
      setRun(paneKey, { cursor: state.order.length, hadFailure: true, error });
      continue;
    }

    setRun(paneKey, { cursor: state.cursor + 1 });
  }
}

/** B1 — one run-until-goal step, taken after a chain-run completes. Evaluates
 *  the stack acceptance and, from its verdict, either stops `goal_met`, stops
 *  with a specific stack stop reason, or re-runs the whole chain. Returns
 *  `'stop'` once the run has reached a terminal phase, `'rerun'` when it reset
 *  the cursor for another chain-run. Reuses A3's precedence + gain idea at
 *  chain scope via `stackGoal.ts` — no new termination logic here. */
async function pursueGoal(
  paneKey: string,
  state: StackRunState,
  defaults: PaneDefaults,
  statusSource: AgentStatusSource
): Promise<'stop' | 'rerun'> {
  const verdict = await evaluateStackAcceptance(paneKey, state, defaults, statusSource);
  if (verdict === null) return 'stop'; // eval couldn't launch — phase already errored
  const chainRun = state.repetition + 1;
  if (verdict.passed) {
    setRun(paneKey, { phase: 'done', stopReason: 'goal_met', error: undefined });
    return 'stop';
  }
  const gain = foldGain({ best: state.goalBest, streak: state.noGainStreak }, verdict.score);
  const decision = decideAfterMiss({
    chainRun,
    maxChainLoops: state.loopTarget,
    noGainStreak: gain.streak,
    noProgressLimit: state.noProgressLimit
  });
  if (decision.kind === 'rerun') {
    setRun(paneKey, { cursor: 0, repetition: chainRun, goalBest: gain.best, noGainStreak: gain.streak });
    return 'rerun';
  }
  setRun(paneKey, {
    phase: 'error',
    stopReason: decision.reason,
    goalBest: gain.best,
    noGainStreak: gain.streak,
    error: stackStopLabel(decision.reason)
  });
  return 'stop';
}

/** The stack-scope eval seam (B1 pre-flight §2). There is no server-side
 *  "stack", so the stack acceptance runs through A1's tiered executor the only
 *  way the client has: a dedicated task carrying the compiled `acceptance`.
 *  Its terminal status *is* the stack-level `EvalOutcome` verdict — A1 makes a
 *  task complete iff its acceptance passed (`crates/lopi-agent/src/runner/
 *  eval_runner.rs`) — and the live score store surfaces the scalar the
 *  no-progress detector reads. `max_iterations: 1` keeps it a single
 *  verification attempt; the iterative progress comes from re-running the
 *  chain, not from this eval doing the work. Returns `null` (and marks the run
 *  errored) only if the eval task can't even launch. */
async function evaluateStackAcceptance(
  paneKey: string,
  state: StackRunState,
  defaults: PaneDefaults,
  statusSource: AgentStatusSource
): Promise<{ passed: boolean; score?: number } | null> {
  const acceptance = state.acceptance;
  if (!acceptance) return { passed: true };
  const evalRef = `${paneKey}::stack-eval::${state.repetition}`;
  const options: CreateTaskOptions = {
    effort: defaults.effort,
    max_iterations: 1,
    acceptance,
    client_ref: evalRef
  };
  // `auto` means "no override" — see `cardToTaskPayload`'s matching comment
  // in `stores/stack.ts`; sending the literal string here would hit the same
  // `--model auto` CLI failure.
  if (defaults.model && defaults.model !== AUTO_MODEL) options.model = defaults.model;
  let resp;
  try {
    resp = await createTask(stackGoalPrompt(paneKey), defaults.repo, 'normal', options);
  } catch (err) {
    setRun(paneKey, {
      phase: 'error',
      hadFailure: true,
      error: `stack acceptance eval failed to launch: ${err instanceof Error ? err.message : String(err)}`
    });
    return null;
  }
  const taskId = effectiveTaskId(resp);
  const terminal = await waitForTerminal(taskId, statusSource);
  const score = get(statusSource).get(taskId)?.score;
  return { passed: terminal === 'completed', score };
}

/** The natural-language goal the stack-acceptance eval task runs under, derived
 *  from the pane — a stack has no free-text goal field of its own (its goal
 *  *is* its acceptance evals), so the eval reads as "verify <stack>
 *  acceptance". */
function stackGoalPrompt(paneKey: string): string {
  const title = get(panes).find((p) => p.key === paneKey)?.title ?? paneKey;
  return `verify stack acceptance for "${title}"`;
}

/** Run-menu "Run now" / "Run once": launch a fresh run for this pane's
 *  cards in execution order. Replaces any prior run state for the pane.
 *  `loopTarget`/`onFail` snapshot the pane's stack config at launch, same
 *  as `order` does for the card sequence — see `StackRunState`'s doc
 *  comment. B1 — when the stack is pursuing a goal (`stackPursuesGoal`) under
 *  a plain "Run", the compiled `acceptance` is snapshotted too, flipping the
 *  chain from fixed-count to run-until-goal. "Run once" never pursues (it is an
 *  explicit single pass), so it always keeps the legacy behavior. */
export function runStack(
  paneKey: string,
  intent: RunIntent,
  defaults: PaneDefaults,
  statusSource: AgentStatusSource
): void {
  const pane = get(panes).find((p) => p.key === paneKey);
  if (!pane || pane.cards.length === 0) return;
  const order = executionOrder(pane.cards).map((c) => c.id);
  const pursuing = intent === 'run' && stackPursuesGoal(pane.config);
  const acceptance = pursuing ? evalsToAcceptance(pane.config.evals) : undefined;
  runs.update((m) => {
    const next = new Map(m);
    next.set(paneKey, {
      paneKey,
      phase: 'running',
      intent,
      order,
      cursor: 0,
      repetition: 0,
      loopTarget: pane.config.loopCount,
      onFail: pane.config.guardrails.onFail,
      hadFailure: false,
      acceptance,
      noProgressLimit: pane.config.goal.noProgressLimit,
      noGainStreak: 0
    });
    return next;
  });
  void advance(paneKey, defaults, statusSource);
}

/** F2 — launch a *bare* pane's single staged card.
 *
 *  A 0-or-1-card pane never renders `StackControlDock` (Unify-2 §3:
 *  `paneIsBare` = ≤1 card), so it never got the dock's run button — and no
 *  other affordance called `createTask`, which is why Verify-1 (F2) found a
 *  bare pane could not be launched from the UI at all. This is that missing
 *  affordance: it submits the one card through `paneSubmitPayload` — the
 *  deliberately loop-semantics-free payload Unify-1 built for exactly the
 *  "one prompt, no stack chrome" case (`max_iterations`/`on_fail`/`gate`/
 *  `acceptance` are all omitted) — and wires the returned `taskId` + terminal
 *  status back onto the card via the same `updateCardInPane`/`waitForTerminal`
 *  path `advance` uses, so the card's orb and output render live identically.
 *  One prompt, one run: no chain, no repetition. No-op unless the pane has
 *  exactly one card and isn't already in flight. */
export function runBarePane(
  paneKey: string,
  defaults: PaneDefaults,
  statusSource: AgentStatusSource
): void {
  const pane = get(panes).find((p) => p.key === paneKey);
  if (!pane || pane.cards.length !== 1) return;
  if (get(runs).get(paneKey)?.phase === 'running') return;
  const card = pane.cards[0];
  runs.update((m) => {
    const next = new Map(m);
    next.set(paneKey, {
      paneKey,
      phase: 'running',
      intent: 'run-once',
      order: [card.id],
      cursor: 0,
      repetition: 0,
      loopTarget: 1,
      onFail: 'stop',
      hadFailure: false,
      noProgressLimit: 0,
      noGainStreak: 0
    });
    return next;
  });
  void launchBareCard(paneKey, card, defaults, statusSource);
}

/** The bare-pane launch body: submit one card as a bare (no-loop) payload,
 *  wire taskId + terminal status back onto it. Mirrors `advance`'s single-card
 *  section without any chain/repetition/goal machinery. */
async function launchBareCard(
  paneKey: string,
  card: StackCard,
  defaults: PaneDefaults,
  statusSource: AgentStatusSource
): Promise<void> {
  const payload = paneSubmitPayload({
    goal: card.goal,
    repo: card.config.repo ?? defaults.repo,
    priority: 'normal',
    model: card.config.model ?? defaults.model,
    effort: card.config.effort ?? defaults.effort,
    branch: card.config.branch
  });
  updateCardInPane(paneKey, card.id, { status: 'queued' });
  let resp;
  try {
    resp = await createTask(payload.goal, payload.repo, payload.priority, payload.options);
  } catch (err) {
    updateCardInPane(paneKey, card.id, { status: 'idle' });
    setRun(paneKey, {
      phase: 'error',
      hadFailure: true,
      error: `"${card.goal}" failed to launch: ${err instanceof Error ? err.message : String(err)}`
    });
    return;
  }
  const taskId = effectiveTaskId(resp);
  updateCardInPane(paneKey, card.id, { status: 'running', taskId });
  const terminal = await waitForTerminal(taskId, statusSource);
  updateCardInPane(paneKey, card.id, { status: 'done' });
  setRun(
    paneKey,
    terminal === 'completed'
      ? { phase: 'done', error: undefined }
      : { phase: 'error', hadFailure: true, error: `"${card.goal}" ended ${terminal}` }
  );
}

/** Halt after the currently-running card's task reaches a terminal status;
 *  resumable via `resumeStack`. No-op if there's no active run. */
export function pauseStack(paneKey: string): void {
  const state = get(runs).get(paneKey);
  if (!state || state.phase !== 'running') return;
  setRun(paneKey, { phase: 'paused' });
}

/** Continue a paused run from where it left off. No-op if the run isn't
 *  currently paused. */
export function resumeStack(
  paneKey: string,
  defaults: PaneDefaults,
  statusSource: AgentStatusSource
): void {
  const state = get(runs).get(paneKey);
  if (!state || state.phase !== 'paused') return;
  setRun(paneKey, { phase: 'running' });
  void advance(paneKey, defaults, statusSource);
}

/** Let the current card finish, then stop for good (unlike pause, not
 *  resumable — a fresh `runStack` is required to run the rest). */
export function drainStack(paneKey: string): void {
  const state = get(runs).get(paneKey);
  if (!state) return;
  if (state.phase === 'paused') {
    setRun(paneKey, {
      phase: state.hadFailure ? 'error' : 'done',
      error: state.hadFailure ? (state.error ?? 'drained after at least one failed loop') : undefined
    });
    return;
  }
  if (state.phase === 'running') {
    setRun(paneKey, { phase: 'draining' });
  }
}

/** Whether `cardId` can be bumped right now, and in which directions — the
 *  pure predicate the card's bump-button UI renders from (Phase 5 — `bumpCard`
 *  previously had no UI trigger), kept separate from the Svelte component so
 *  it's unit-testable without a component harness. Mirrors `bumpCard`'s own
 *  legality checks exactly (queue position past the cursor, room left to move
 *  in that direction) so a button is never shown enabled for a call that
 *  would actually be rejected. `runState` is `undefined` when no run is
 *  active for the pane — nothing is bumpable then. */
export function bumpUiState(
  runState: StackRunState | undefined,
  cardId: string
): { visible: boolean; canSooner: boolean; canLater: boolean } {
  const runActive =
    runState?.phase === 'running' || runState?.phase === 'paused' || runState?.phase === 'draining';
  if (!runState || !runActive) return { visible: false, canSooner: false, canLater: false };
  const idx = runState.order.indexOf(cardId);
  const visible = idx > runState.cursor;
  return {
    visible,
    canSooner: visible && idx - 1 > runState.cursor,
    canLater: visible && idx + 1 < runState.order.length
  };
}

/** Reorder a not-yet-started card within an active run's remaining queue,
 *  reflecting the swap into both the run's own plan (`order`) and the
 *  pane's real card array (so the UI's rendered stack order matches).
 *  Rejects illegal transitions (bumping the running/finished card, or past
 *  either end of the queue) with a clear error rather than a silent no-op. */
export function bumpCard(
  paneKey: string,
  cardId: string,
  direction: 'up' | 'down'
): { ok: true } | { ok: false; error: string } {
  const state = get(runs).get(paneKey);
  if (!state) return { ok: false, error: 'no active run for this pane' };

  const idx = state.order.indexOf(cardId);
  const neighborId = idx === -1 ? undefined : state.order[direction === 'up' ? idx - 1 : idx + 1];

  const result = bumpInOrder(state.order, state.cursor, cardId, direction);
  if (!result.ok) return result;

  setRun(paneKey, { order: result.order });

  if (neighborId) {
    const pane = get(panes).find((p) => p.key === paneKey);
    const fromIdx = pane?.cards.findIndex((c) => c.id === cardId) ?? -1;
    const neighborIdx = pane?.cards.findIndex((c) => c.id === neighborId) ?? -1;
    if (fromIdx !== -1 && neighborIdx !== -1) {
      reorderInPane(paneKey, fromIdx, neighborIdx);
    }
  }

  return { ok: true };
}

/** The result of `scheduleStack` — honest about the fact that it can only
 *  attach the given cron to the bottom-of-stack (first-to-run) card, not
 *  the whole plan. */
export interface ScheduleStackResult {
  ok: boolean;
  scheduledCardId?: string;
  /** Every other card in the stack — the server-side `Schedule` model
   *  (`crates/lopi-*` `ScheduleBody.goal: String`) has no concept of a
   *  multi-goal pipeline, so a "schedule stack" run-menu intent can only
   *  ever wire up one goal per cron. Making the rest run on the same
   *  trigger would need a backend change (`ScheduleSpec.goal: String` →
   *  `Vec<String>`, per the pre-flight's own notes) that's out of scope
   *  here — this reports the gap instead of silently dropping it. */
  skippedCardIds: string[];
  error?: string;
}

/** Run-menu "Schedule stack": deliberately minimal, and says so. Attaches
 *  one cron to the first card in execution order via the real
 *  `createSchedule`; every other card is reported back as skipped rather
 *  than pretended-scheduled. */
export async function scheduleStack(
  paneKey: string,
  cronExpr: string,
  defaults: PaneDefaults
): Promise<ScheduleStackResult> {
  const pane = get(panes).find((p) => p.key === paneKey);
  if (!pane || pane.cards.length === 0) {
    return { ok: false, skippedCardIds: [], error: 'nothing to schedule' };
  }
  const [first, ...rest] = executionOrder(pane.cards);
  const payload = cardToTaskPayload(first, defaults);
  const body: ScheduleBody = {
    name: `stack:${paneKey}:${first.id}`,
    cron: cronExpr,
    goal: payload.goal,
    repo: payload.repo,
    priority: payload.priority
  };
  try {
    await createSchedule(body);
  } catch (err) {
    return {
      ok: false,
      skippedCardIds: rest.map((c) => c.id),
      error: err instanceof Error ? err.message : String(err)
    };
  }
  return { ok: true, scheduledCardId: first.id, skippedCardIds: rest.map((c) => c.id) };
}
