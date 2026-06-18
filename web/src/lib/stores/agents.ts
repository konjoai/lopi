/**
 * Agent state store — driven by the lopi-core AgentEvent stream.
 *
 * Architecture:
 *   - WebSocket → parseWireMessage (defensive) → reducer → Svelte store
 *   - Snapshot on connect populates the agents map
 *   - Mock generator emits identical wire shapes when no backend is reachable
 *   - Derived stores compute the active agent + aggregate counts
 *
 * The reducer is the single source of truth for state mutation. Each AgentEvent
 * variant has a dedicated handler that updates the agent map immutably.
 */
import { writable, derived, type Readable } from 'svelte/store';
import { browser } from '$app/environment';
import {
  parseWireMessage,
  taskStatusToPhase,
  isTerminalStatus
} from '$lib/parser';
import { connect, setMessageHandler, initMock, getConnectionState } from './wsClient';
import { recordEvent } from './events';
import { isDeleted, reconcileSessions, tombstoneSession } from './layout';
import { reduce } from './agents-reducer';
import {
  type AgentState,
  type LogEntry,
  type Status,
  PHASE_COLORS,
  makeBlank,
  clamp01
} from './agents-model';
import type { StimulusKind } from '$lib/forge/excitement';
import type { PoolStats, TaskStatus, WireMessage } from '$lib/types';

// ── Re-export the model surface for consumers (stable legacy imports) ─────────
export type { Phase, TaskStatus } from '$lib/types';
export type { AgentState, LogEntry, Status } from './agents-model';
export { PHASE_COLORS };

// ── Stores ────────────────────────────────────────────────────────────────────
export const agents = writable<Map<string, AgentState>>(new Map());
export const logs = writable<LogEntry[]>([]);
export const activeAgentId = writable<string | null>(null);
export const poolStats = writable<PoolStats>({
  running: 0,
  queued: 0,
  succeeded: 0,
  failed: 0,
  uptime_secs: 0
});
export const connectionState = writable<'connecting' | 'connected' | 'offline' | 'mock'>(
  'connecting'
);
let connectionStateInterval: ReturnType<typeof setInterval> | null = null;

// ── Derived: active agent (drives the Forge) ─────────────────────────────────
export const activeAgent: Readable<AgentState | null> = derived(
  [agents, activeAgentId],
  ([$agents, $activeId]) => {
    if ($activeId && $agents.has($activeId)) return $agents.get($activeId)!;
    for (const a of $agents.values()) if (a.status === 'running') return a;
    return $agents.values().next().value ?? null;
  }
);

// ── Derived: aggregate counts ────────────────────────────────────────────────
export const stats = derived([agents, poolStats], ([$agents, $pool]) => {
  let running = 0,
    queued = 0,
    completed = 0,
    failed = 0;
  let totalCost = 0;
  for (const a of $agents.values()) {
    if (a.status === 'running') running++;
    else if (a.status === 'queued') queued++;
    else if (a.status === 'completed') completed++;
    else if (a.status === 'failed') failed++;
    totalCost += a.cost;
  }
  // Prefer server-side pool stats when available, falling back to local count.
  return {
    running: $pool.running || running,
    queued: $pool.queued || queued,
    completed: $pool.succeeded || completed,
    failed: $pool.failed || failed,
    total: $agents.size,
    totalCost,
    uptimeSecs: $pool.uptime_secs
  };
});

// ── Derived: agents waiting for permission (stalled on Claude prompt) ────────
const PERMISSION_PATTERNS = [
  /\[y\/n\]/i,
  /\(yes\/no\)/i,
  /do you want/i,
  /allow.*\?$/i,
  /shall i/i,
  /waiting for input/i,
  /permission.*required/i,
  /awaiting confirmation/i
];

export const permissionWaiting: Readable<Set<string>> = derived(agents, ($agents) => {
  const waiting = new Set<string>();
  for (const [id, agent] of $agents) {
    if (agent.status !== 'running') continue;
    const t = agent.thought ?? '';
    const stalled = agent.activity < 0.02 && agent.elapsedMs > 8000;
    const hasPattern = PERMISSION_PATTERNS.some((re) => re.test(t));
    if (stalled || hasPattern) waiting.add(id);
  }
  return waiting;
});

// ── Periodic UI tick — drives elapsed time + activity decay ──────────────────
let elapsedTimer: ReturnType<typeof setInterval> | null = null;
function startElapsedTicker() {
  if (elapsedTimer) return;
  elapsedTimer = setInterval(() => {
    agents.update((m) => {
      const next = new Map(m);
      let changed = false;
      for (const [id, a] of next) {
        if (a.status !== 'running') continue;
        const elapsedMs = Date.now() - a.startedAt;
        // Activity decays gently when no new turn_metrics arrives — keeps the
        // Forge from looking frozen but doesn't fabricate motion.
        const decayedActivity = Math.max(0, a.activity * 0.985);
        next.set(id, { ...a, elapsedMs, activity: decayedActivity });
        changed = true;
      }
      return changed ? next : m;
    });
  }, 250);
}

// ── Apply a parsed wire message ──────────────────────────────────────────────
function applyMessage(msg: WireMessage) {
  if (msg.type === 'snapshot') {
    // Initialize agents from the server snapshot.
    poolStats.set(msg.stats);
    agents.update((m) => {
      const next = new Map(m);
      for (const t of msg.tasks) {
        if (next.has(t.id)) continue;
        // Tombstoned sessions were permanently deleted — never re-hydrate them,
        // even if the server snapshot still carries the row. This is the core
        // of the "deleted sessions reappear" fix.
        if (isDeleted(t.id)) continue;
        const phase = taskStatusToPhase(t.status as TaskStatus | string);
        const terminal =
          typeof t.status === 'string'
            ? t.status === 'RolledBack'
            : typeof t.status === 'object' && t.status !== null
              ? 'Success' in t.status || 'Failed' in t.status
              : false;
        next.set(t.id, {
          ...makeBlank(t.id),
          goal: t.goal,
          phase,
          status: terminal
            ? typeof t.status === 'object' && t.status !== null && 'Failed' in t.status
              ? 'failed'
              : 'completed'
            : typeof t.status === 'string' && t.status === 'Queued'
              ? 'queued'
              : 'running',
          taskStatus: t.status as TaskStatus | string,
          startedAt: Date.parse(t.created_at) || Date.now()
        });
      }
      return next;
    });
    // Auto-place only sessions we've never seen before; previously-closed or
    // already-known sessions keep the user's persisted pane layout.
    reconcileSessions(msg.tasks.map((t) => t.id));
    return;
  }

  // A late event for a tombstoned session must not resurrect it.
  if ('task_id' in msg && typeof msg.task_id === 'string' && isDeleted(msg.task_id)) {
    return;
  }

  // AgentEvent variants
  if (msg.type === 'log_line') {
    logs.update((l) => [
      ...l.slice(-199),
      {
        ts: Date.parse(msg.ts) || Date.now(),
        taskId: msg.task_id,
        level: msg.level,
        message: msg.line
      }
    ]);
  }
  if (msg.type === 'pool_stats') {
    poolStats.set({
      running: msg.running,
      queued: msg.queued,
      succeeded: msg.succeeded,
      failed: msg.failed,
      uptime_secs: msg.uptime_secs
    });
  }

  // Record every event into the live Pulse feed (+ budget alert stream).
  recordEvent(msg);

  agents.update((m) => reduce(m, msg));

  // A freshly-queued/started task pops into a free pane automatically.
  if (msg.type === 'task_queued' || msg.type === 'task_started') {
    reconcileSessions([msg.task_id]);
  }
}

// ── Connection state management ───────────────────────────────────────────────
function updateConnectionState() {
  const state = getConnectionState();
  connectionState.set(state);
}

// ── Public API ────────────────────────────────────────────────────────────────
export function init() {
  if (!browser) return;
  startElapsedTicker();
  setMessageHandler((raw) => {
    const parsed = parseWireMessage(raw);
    if (parsed) {
      applyMessage(parsed);
    } else {
      console.debug('[lopi] dropped malformed wire frame', raw);
    }
  });
  updateConnectionState();
  connect();
  if (connectionStateInterval) clearInterval(connectionStateInterval);
  connectionStateInterval = setInterval(updateConnectionState, 500);
  // Fall back to mock if not connected in 1.5s
  setTimeout(() => {
    const state = getConnectionState();
    if (state === 'offline' || state === 'connecting') {
      initMock();
      updateConnectionState();
    }
  }, 1500);
}

export function selectAgent(id: string) {
  activeAgentId.set(id);
}

/**
 * Mark an agent as having just received a request — the Forge orb reacts
 * (shake, spin-up, orange glow). Called optimistically on user submission
 * so the orb responds before the server round-trip completes.
 */
export function stimulate(id: string, kind: StimulusKind = 'request') {
  agents.update((m) => {
    const cur = m.get(id);
    if (!cur) return m;
    const next = new Map(m);
    next.set(id, { ...cur, stimulus: Date.now(), stimulusKind: kind });
    return next;
  });
}

/**
 * Permanently delete a session. Unlike closing a pane (which only parks the
 * session in the sidebar), this tombstones the id so the snapshot reducer can
 * never re-hydrate it, drops it from local state, and asks the server to
 * cancel + delete. The tombstone — not the server round-trip — is what
 * guarantees the session stays gone across reloads, so a best-effort DELETE is
 * safe here: even a dropped request can no longer cause a resurrection.
 */
export function deleteSession(id: string) {
  tombstoneSession(id);
  agents.update((m) => {
    const next = new Map(m);
    next.delete(id);
    return next;
  });
  if (!browser) return;
  void fetch(`/api/tasks/${encodeURIComponent(id)}`, { method: 'DELETE' }).catch((err) =>
    console.warn('[lopi] DELETE /api/tasks failed:', err)
  );
}

/** @deprecated Use {@link deleteSession}. Retained for the Tasks page. */
export function removeAgent(id: string) {
  deleteSession(id);
}

/** Per-task launch options surfaced by the pane selectors. */
export interface TaskOptions {
  priority?: 'low' | 'normal' | 'high' | 'critical';
  model?: string;
  effort?: string;
  branch?: string;
  constraints?: string[];
}

/**
 * Build the `constraints` payload from the selector values. Model, effort and
 * branch have no dedicated columns on the task yet, so we surface them as
 * planning constraints (the same channel the backend already appends to the
 * agent's prompt) rather than inventing fields that go nowhere.
 */
export function buildConstraints(opts: TaskOptions): string[] {
  const out = [...(opts.constraints ?? [])];
  if (opts.model) out.push(`Preferred model: ${opts.model}`);
  if (opts.effort) out.push(`Reasoning effort: ${opts.effort}`);
  if (opts.branch) out.push(`Target branch: ${opts.branch}`);
  return out;
}

export function postTask(goal: string, repo: string, opts: TaskOptions = {}) {
  if (!browser) return Promise.reject(new Error('not-browser'));
  const constraints = buildConstraints(opts);
  return fetch('/api/tasks', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      goal,
      repo,
      priority: opts.priority ?? 'normal',
      ...(constraints.length > 0 ? { constraints } : {})
    })
  });
}

export async function cancelTask(id: string): Promise<boolean> {
  if (!browser) return false;
  try {
    const res = await fetch(`/api/tasks/${id}`, { method: 'DELETE' });
    if (!res.ok) return false;
    const body = await res.json();
    return body.cancelled === true;
  } catch {
    return false;
  }
}
