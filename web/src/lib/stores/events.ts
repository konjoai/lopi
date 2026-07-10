/**
 * Live event feed — a bounded, typed ring buffer of every AgentEvent the
 * store has processed, plus derived alert streams.
 *
 * The agents store owns state reduction; this module is the *narrative* layer:
 * it records what happened, in order, so the UI can render a live nerve-centre
 * (the Pulse feed), budget toasts, and verifier verdicts without each consumer
 * re-deriving them from the agent map.
 *
 * `recordEvent` is called from the agents store's `applyMessage`, so every
 * frame — WebSocket or mock — flows through here exactly once.
 */
import { writable } from 'svelte/store';
import type { AgentEvent, BudgetScope } from '$lib/types';

/** A single entry in the live feed, normalized for rendering. */
export interface PulseEntry {
  /** Monotonic id — render key + ordering. */
  seq: number;
  /** Wall-clock receipt time (ms). */
  ts: number;
  /** The wire event `type`. */
  kind: AgentEvent['type'];
  /** Owning task id, when the event carries one. */
  taskId: string | null;
  /** One-line human summary. */
  summary: string;
  /** Severity tier — drives color. */
  tier: 'info' | 'good' | 'warn' | 'bad';
}

/** Active budget breach — surfaced as a dismissible toast. */
export interface BudgetAlert {
  seq: number;
  ts: number;
  taskId: string | null;
  scope: BudgetScope;
  limitUsd: number;
  burnedUsd: number;
}

const MAX_PULSE = 200;

export const pulse = writable<PulseEntry[]>([]);
export const budgetAlerts = writable<BudgetAlert[]>([]);

/** Total events seen this session — drives the live counter in the header. */
export const pulseCount = writable(0);

let seq = 0;

/** Per-kind tier + summary formatting. Kept pure for unit testing. */
export function describe(ev: AgentEvent): { summary: string; tier: PulseEntry['tier'] } {
  switch (ev.type) {
    case 'task_queued':
      return { summary: `queued · ${ev.goal}`, tier: 'info' };
    case 'task_started':
      return { summary: `started attempt ${ev.attempt} on ${ev.branch}`, tier: 'info' };
    case 'status_changed':
      return { summary: `→ ${statusName(ev.status)} (attempt ${ev.attempt})`, tier: 'info' };
    case 'log_line':
      return {
        summary: ev.line,
        tier: ev.level === 'error' ? 'bad' : ev.level === 'warn' ? 'warn' : 'info'
      };
    case 'score_updated':
      return {
        summary: `scored ${Math.round(ev.test_pass_rate * 100)}% pass · ${ev.lint_errors} lint · ${ev.diff_lines} Δlines`,
        tier: ev.test_pass_rate >= 0.8 ? 'good' : ev.lint_errors > 0 ? 'warn' : 'info'
      };
    case 'task_completed': {
      const failed = typeof ev.outcome === 'object' && 'Failed' in ev.outcome;
      return {
        summary: failed ? 'failed — all retries exhausted' : `completed in ${ev.total_attempts} attempt(s)`,
        tier: failed ? 'bad' : 'good'
      };
    }
    case 'task_cancelled':
      return { summary: 'cancelled', tier: 'warn' };
    case 'pool_stats':
      return {
        summary: `pool · ${ev.running} running · ${ev.queued} queued · ${ev.succeeded}✓ ${ev.failed}✗`,
        tier: 'info'
      };
    case 'turn_metrics':
      return {
        summary: `turn · ${Math.round(ev.pressure * 100)}% pressure · ${Math.round(ev.tokens_per_sec)} tok/s · $${ev.cost_usd.toFixed(4)}`,
        tier: ev.pressure > 0.85 ? 'warn' : 'info'
      };
    case 'verifier_verdict':
      return {
        summary: ev.passed
          ? 'verifier: all rubric criteria met'
          : `verifier: ${ev.gaps.length} gap(s) — ${ev.gaps[0] ?? 'see detail'}`,
        tier: ev.passed ? 'good' : 'warn'
      };
    case 'budget_exceeded':
      return {
        summary: `budget · ${ev.scope} cap $${ev.limit_usd.toFixed(2)} hit ($${ev.burned_usd.toFixed(2)} burned)`,
        tier: 'bad'
      };
    default:
      return { summary: (ev as { type: string }).type, tier: 'info' };
  }
}

function statusName(status: unknown): string {
  if (typeof status === 'string') return status;
  if (status && typeof status === 'object') return Object.keys(status)[0] ?? 'Unknown';
  return 'Unknown';
}

function eventTaskId(ev: AgentEvent): string | null {
  if (ev.type === 'pool_stats') return null;
  if (ev.type === 'budget_exceeded') return ev.task_id;
  return (ev as { task_id?: string }).task_id ?? null;
}

/** Record one event into the live feed (+ budget alert stream). */
export function recordEvent(ev: AgentEvent) {
  const { summary, tier } = describe(ev);
  seq += 1;
  const entry: PulseEntry = {
    seq,
    ts: Date.now(),
    kind: ev.type,
    taskId: eventTaskId(ev),
    summary,
    tier
  };
  pulse.update((p) => {
    const next = p.length >= MAX_PULSE ? p.slice(p.length - MAX_PULSE + 1) : p.slice();
    next.push(entry);
    return next;
  });
  pulseCount.update((n) => n + 1);

  if (ev.type === 'budget_exceeded') {
    budgetAlerts.update((a) => [
      ...a.filter((x) => x.scope !== ev.scope || x.taskId !== ev.task_id).slice(-4),
      {
        seq,
        ts: Date.now(),
        taskId: ev.task_id,
        scope: ev.scope,
        limitUsd: ev.limit_usd,
        burnedUsd: ev.burned_usd
      }
    ]);
  }
}

/** Dismiss a budget alert toast. */
export function dismissBudgetAlert(seqId: number) {
  budgetAlerts.update((a) => a.filter((x) => x.seq !== seqId));
}
