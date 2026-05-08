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

// ── WebSocket client ──────────────────────────────────────────────────────────
let ws: WebSocket | null = null;
let reconnectDelay = 1000;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let mockTimer: ReturnType<typeof setInterval> | null = null;
let mockTick = 0;

function connect() {
  if (!browser) return;
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

  connectionState.set('connecting');
  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  // Server exposes both /ws (canonical, since v0.8.0) and /ws/tasks (legacy).
  const url = `${proto}://${location.host}/ws`;
  try {
    ws = new WebSocket(url);
  } catch {
    scheduleReconnect();
    return;
  }

  ws.onopen = () => {
    connectionState.set('connected');
    reconnectDelay = 1000;
    if (mockTimer) {
      clearInterval(mockTimer);
      mockTimer = null;
    }
  };

  ws.onmessage = (e) => {
    try {
      const raw = JSON.parse(e.data);
      const parsed = parseWireMessage(raw);
      if (parsed) applyMessage(parsed);
      else {
        // Don't crash — log once at debug level. Most likely a server protocol mismatch.
        console.debug('[lopi] dropped malformed wire frame', raw);
      }
    } catch {
      console.debug('[lopi] dropped non-JSON frame');
    }
  };

  ws.onclose = () => {
    ws = null;
    connectionState.set('offline');
    scheduleReconnect();
  };

  ws.onerror = () => {
    /* onclose fires too — consolidate handling there */
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    reconnectDelay = Math.min(reconnectDelay * 2, 30000);
    connect();
    if (!ws) startMockData();
  }, reconnectDelay);
}

// ── Mock data generator (real wire format) ────────────────────────────────────
function startMockData() {
  if (mockTimer) return;
  connectionState.set('mock');

  // Two pairs of seed agents share signals so the Constellation's insight
  // lines actually appear in preview mode:
  //   • demo-1 + demo-2 share repo (~/kyro) AND share goal keywords
  //     ("semantic cache redis")
  //   • demo-3 + demo-4 share repo (~/vectro) but different goals
  //   • demo-5 stands alone as a control case with no peers
  const seedAgents: { task_id: string; goal: string; branch: string; repo: string }[] = [
    {
      task_id: 'demo-1-' + crypto.randomUUID().slice(0, 8),
      goal: 'Add Redis-backed semantic cache to the RAG pipeline',
      branch: 'feat/pg-cache',
      repo: '~/kyro'
    },
    {
      task_id: 'demo-2-' + crypto.randomUUID().slice(0, 8),
      goal: 'Migrate semantic cache invalidation logic to Redis streams',
      branch: 'feat/redis-cache',
      repo: '~/kyro'
    },
    {
      task_id: 'demo-3-' + crypto.randomUUID().slice(0, 8),
      goal: 'Refactor the encoder hot path to use NEON 32-wide unroll',
      branch: 'perf/neon-unroll',
      repo: '~/vectro'
    },
    {
      task_id: 'demo-4-' + crypto.randomUUID().slice(0, 8),
      goal: 'Wire AVX-512 VNNI dispatch into encode_fast_into',
      branch: 'perf/avx512',
      repo: '~/vectro'
    },
    {
      task_id: 'demo-5-' + crypto.randomUUID().slice(0, 8),
      goal: 'Wire OTel trace export from /generate spans',
      branch: 'feat/otel',
      repo: '~/kairu'
    }
  ];

  // Emit a snapshot first
  applyMessage({
    type: 'snapshot',
    tasks: seedAgents.map((a) => ({
      id: a.task_id,
      goal: a.goal,
      status: 'Planning' as TaskStatus,
      created_at: new Date(Date.now() - Math.random() * 60000).toISOString()
    })),
    stats: { running: seedAgents.length, queued: 0, succeeded: 12, failed: 1, uptime_secs: 1820 }
  });

  // Hydrate repo + branch fields manually (snapshot doesn't carry these)
  agents.update((m) => {
    const next = new Map(m);
    for (const seed of seedAgents) {
      const cur = next.get(seed.task_id);
      if (cur) next.set(seed.task_id, { ...cur, repo: seed.repo, branch: seed.branch });
    }
    return next;
  });

  activeAgentId.set(seedAgents[0].task_id);

  const phaseCycle: TaskStatus[] = ['Planning', 'Implementing', 'Testing', 'Scoring'];
  const logTemplates: { level: 'info' | 'warn' | 'error' | 'debug'; line: string }[] = [
    { level: 'info', line: 'Read 14 files, 2.3k lines analyzed' },
    { level: 'info', line: 'Plan generated: 4 edits across 2 files' },
    { level: 'debug', line: 'Token pressure: 42% (within budget)' },
    { level: 'info', line: 'Edit applied to crates/lopi-agent/src/runner.rs' },
    { level: 'info', line: 'cargo check: clean' },
    { level: 'warn', line: 'clippy: 1 hint in lib.rs:47 (auto-fixed)' },
    { level: 'info', line: 'cargo nextest: 39 passed, 0 failed' },
    { level: 'info', line: 'Eviction fired: 12 turns reclaimed (4.2k tokens)' },
    { level: 'debug', line: 'Cache hit on system prompt — 1850 tokens saved' }
  ];

  mockTimer = setInterval(() => {
    mockTick++;
    for (const seed of seedAgents) {
      // Phase progression every 30 ticks
      if (mockTick % 30 === 5) {
        const idx = Math.floor(mockTick / 30) % phaseCycle.length;
        applyMessage({
          type: 'status_changed',
          task_id: seed.task_id,
          status: phaseCycle[idx],
          attempt: 1
        });
      }

      // Turn metrics every 2 ticks (1s) — drives the Forge
      if (mockTick % 2 === 0) {
        const tNorm = mockTick * 0.05;
        const pressure = clamp01(
          0.45 + Math.sin(tNorm + seed.task_id.length) * 0.18 + (Math.random() - 0.5) * 0.04
        );
        const activity = clamp01(
          0.55 + Math.cos(tNorm * 0.7 + seed.task_id.length) * 0.3 + (Math.random() - 0.5) * 0.06
        );
        applyMessage({
          type: 'turn_metrics',
          task_id: seed.task_id,
          pressure,
          activity,
          tokens_per_sec: activity * 80,
          cost_usd: (mockTick * 0.0001 * activity).toFixed(6) as unknown as number
        });
      }

      // Log every 4 ticks
      if (mockTick % 4 === 1) {
        const tpl = logTemplates[Math.floor(Math.random() * logTemplates.length)];
        applyMessage({
          type: 'log_line',
          task_id: seed.task_id,
          line: tpl.line,
          level: tpl.level,
          ts: new Date().toISOString()
        });
      }

      // Score updates every 50 ticks
      if (mockTick % 50 === 25) {
        applyMessage({
          type: 'score_updated',
          task_id: seed.task_id,
          test_pass_rate: 0.82 + Math.random() * 0.15,
          lint_errors: Math.floor(Math.random() * 3),
          diff_lines: Math.floor(20 + Math.random() * 80)
        });
      }
    }
  }, 500);
}

// ── Public API ────────────────────────────────────────────────────────────────
export function init() {
  if (!browser) return;
  startElapsedTicker();
  connect();
  // Fall back to mock if WS hasn't opened in 1.5s — the UI is never empty.
  setTimeout(() => {
    if (!ws || ws.readyState !== WebSocket.OPEN) startMockData();
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

// ── Helpers ───────────────────────────────────────────────────────────────────
function clamp01(n: number): number {
  if (!Number.isFinite(n)) return 0;
  if (n < 0) return 0;
  if (n > 1) return 1;
  return n;
}
