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
import type {
  AgentEvent,
  Phase,
  PoolStats,
  TaskStatus,
  WireMessage
} from '$lib/types';

// ── Re-export types for consumers (legacy import surface) ─────────────────────
export type { Phase, TaskStatus } from '$lib/types';

// ── UI-side state shape ───────────────────────────────────────────────────────
export type Status = 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface AgentState {
  id: string;
  goal: string;
  repo: string;
  branch: string;
  status: Status;
  taskStatus: TaskStatus | string;
  phase: Phase;
  attempt: number;
  startedAt: number;
  elapsedMs: number;

  // Forge inputs (0..1 each)
  pressure: number;
  activity: number;
  health: number;

  // Score breakdown (from score_updated events)
  testPassRate?: number;
  lintErrors?: number;
  diffLines?: number;
  score?: number; // synthetic 0..1 composite

  cost: number; // USD accumulated
  thought?: string; // last log line (preview)
}

export interface LogEntry {
  ts: number;
  taskId: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
}

// ── Phase color map (mirrors :root vars in app.css) ───────────────────────────
export const PHASE_COLORS: Record<Phase, string> = {
  Boot: '#f5f5f5',
  Discovery: '#00d4ff',
  Planning: '#00ffd4',
  Implementation: '#ff4500',
  Testing: '#ffcc00',
  Conclusion: '#00ff9d'
};

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

// ── Reducer: AgentEvent → AgentState mutation ────────────────────────────────
function reduce(map: Map<string, AgentState>, ev: AgentEvent): Map<string, AgentState> {
  const next = new Map(map);
  switch (ev.type) {
    case 'task_queued': {
      next.set(ev.task_id, {
        id: ev.task_id,
        goal: ev.goal,
        repo: '',
        branch: '',
        status: 'queued',
        taskStatus: 'Queued',
        phase: 'Boot',
        attempt: 0,
        startedAt: Date.now(),
        elapsedMs: 0,
        pressure: 0.05,
        activity: 0.0,
        health: 0.85,
        cost: 0
      });
      break;
    }
    case 'task_started': {
      const cur = next.get(ev.task_id);
      next.set(ev.task_id, {
        ...(cur ?? makeBlank(ev.task_id)),
        status: 'running',
        attempt: ev.attempt,
        branch: ev.branch,
        startedAt: cur?.startedAt ?? Date.now(),
        phase: cur?.phase ?? 'Boot'
      });
      break;
    }
    case 'status_changed': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      const phase = taskStatusToPhase(ev.status);
      const isCompleted = isTerminalStatus(ev.status);
      next.set(ev.task_id, {
        ...cur,
        taskStatus: ev.status,
        phase,
        attempt: ev.attempt,
        status: isCompleted
          ? typeof ev.status === 'object' && 'Failed' in ev.status
            ? 'failed'
            : 'completed'
          : 'running'
      });
      break;
    }
    case 'log_line': {
      const cur = next.get(ev.task_id);
      if (cur) {
        // Keep the most recent meaningful line as the "thought" preview.
        next.set(ev.task_id, { ...cur, thought: ev.line });
      }
      break;
    }
    case 'score_updated': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      // Composite 0..1 score: primarily test_pass_rate, penalized by lint errors.
      const composite = clamp01(
        ev.test_pass_rate * 0.85 - Math.min(ev.lint_errors / 50, 0.15)
      );
      next.set(ev.task_id, {
        ...cur,
        testPassRate: ev.test_pass_rate,
        lintErrors: ev.lint_errors,
        diffLines: ev.diff_lines,
        score: composite,
        // Health drifts toward score over time — recent runs influence the aura.
        health: cur.health * 0.7 + composite * 0.3
      });
      break;
    }
    case 'task_completed': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      const failed = typeof ev.outcome === 'object' && 'Failed' in ev.outcome;
      next.set(ev.task_id, {
        ...cur,
        status: failed ? 'failed' : 'completed',
        taskStatus: ev.outcome,
        phase: 'Conclusion',
        activity: 0.0,
        attempt: ev.total_attempts
      });
      break;
    }
    case 'task_cancelled': {
      const cur = next.get(ev.task_id);
      if (cur) {
        next.set(ev.task_id, { ...cur, status: 'cancelled', activity: 0 });
      }
      break;
    }
    case 'pool_stats': {
      // Handled separately via the `poolStats` store — no per-agent change.
      break;
    }
    case 'turn_metrics': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      next.set(ev.task_id, {
        ...cur,
        pressure: ev.pressure,
        activity: ev.activity,
        cost: ev.cost_usd
      });
      break;
    }
  }
  return next;
}

function makeBlank(id: string): AgentState {
  return {
    id,
    goal: 'unknown',
    repo: '',
    branch: '',
    status: 'queued',
    taskStatus: 'Queued',
    phase: 'Boot',
    attempt: 0,
    startedAt: Date.now(),
    elapsedMs: 0,
    pressure: 0.05,
    activity: 0,
    health: 0.85,
    cost: 0
  };
}

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
            : 'running',
          taskStatus: t.status as TaskStatus | string,
          startedAt: Date.parse(t.created_at) || Date.now()
        });
      }
      return next;
    });
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

  agents.update((m) => reduce(m, msg));
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

export function postTask(
  goal: string,
  repo: string,
  priority: 'low' | 'normal' | 'high' = 'normal'
) {
  if (!browser) return Promise.reject(new Error('not-browser'));
  return fetch('/api/tasks', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ goal, repo, priority })
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

// ── Helpers ───────────────────────────────────────────────────────────────────
function clamp01(n: number): number {
  if (!Number.isFinite(n)) return 0;
  if (n < 0) return 0;
  if (n > 1) return 1;
  return n;
}
