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
import { createTask, createSchedule, effectiveTaskId, type ScheduleBody } from '$lib/api';
import {
  panes,
  updateCardInPane,
  reorderInPane,
  executionOrder,
  cardToTaskPayload,
  cardToTaskPayloadForRunOnce,
  bumpInOrder,
  type StackCard,
  type PaneDefaults
} from './stack';

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
 *  more cards while it's mid-flight. */
export interface StackRunState {
  paneKey: string;
  phase: RunPhase;
  intent: RunIntent;
  order: string[];
  cursor: number;
  error?: string;
}

/** Active runs, keyed by pane key. Client-only, in-memory — a page reload
 *  loses run state exactly like it loses pane state. */
export const runs = writable<Map<string, StackRunState>>(new Map());

/** The minimal shape `waitForTerminal` needs from a live agent-state store —
 *  satisfied by the real `agents` store (`stores/agents.ts`) and, in tests,
 *  by a plain `writable(new Map())`. */
export type AgentStatusSource = Readable<Map<string, { status?: string }>>;

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
 *  drains, errors, or runs out of cards. Safe to call again after a pause
 *  is lifted (`resumeStack`) — it always re-reads `runs`/`panes` fresh at
 *  the top of each iteration rather than closing over a stale snapshot. */
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
      setRun(paneKey, { phase: 'done' });
      return;
    }
    if (state.phase !== 'running') return;
    if (state.cursor >= state.order.length) {
      setRun(paneKey, { phase: 'done' });
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
        error: `"${card.goal}" failed to launch: ${err instanceof Error ? err.message : String(err)}`
      });
      return;
    }

    const taskId = effectiveTaskId(resp);
    updateCardInPane(paneKey, cardId, { status: 'running', taskId });

    const terminal = await waitForTerminal(taskId, statusSource);
    updateCardInPane(paneKey, cardId, { status: 'done' });

    if (terminal !== 'completed') {
      // Stop the whole run rather than silently continuing past a failed
      // card — matches the brief's on_fail semantics being per-loop, not
      // "ignore and move on" at the stack level.
      setRun(paneKey, { phase: 'error', error: `"${card.goal}" ended ${terminal}` });
      return;
    }

    setRun(paneKey, { cursor: state.cursor + 1 });
  }
}

/** Run-menu "Run now" / "Run once": launch a fresh run for this pane's
 *  cards in execution order. Replaces any prior run state for the pane. */
export function runStack(
  paneKey: string,
  intent: RunIntent,
  defaults: PaneDefaults,
  statusSource: AgentStatusSource
): void {
  const pane = get(panes).find((p) => p.key === paneKey);
  if (!pane || pane.cards.length === 0) return;
  const order = executionOrder(pane.cards).map((c) => c.id);
  runs.update((m) => {
    const next = new Map(m);
    next.set(paneKey, { paneKey, phase: 'running', intent, order, cursor: 0 });
    return next;
  });
  void advance(paneKey, defaults, statusSource);
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
    setRun(paneKey, { phase: 'done' });
    return;
  }
  if (state.phase === 'running') {
    setRun(paneKey, { phase: 'draining' });
  }
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
