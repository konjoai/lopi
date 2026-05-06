/**
 * Agent state store — mirrors lopi-core types.
 *
 * Provides:
 *   - Real WebSocket client connecting to `/ws/tasks` (lopi sail server)
 *   - Mock data generator for standalone preview when no server is running
 *   - Derived stores for the active agent (drives the Forge)
 */
import { writable, derived, type Readable } from 'svelte/store';
import { browser } from '$app/environment';

// ── Types (mirror lopi-core Rust types) ───────────────────────────────────────
export type Phase =
  | 'Boot'
  | 'Discovery'
  | 'Planning'
  | 'Implementation'
  | 'Testing'
  | 'Conclusion';

export type Status = 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface AgentState {
  id: string;
  goal: string;
  repo: string;
  status: Status;
  phase: Phase;
  attempt: number;
  branch: string;
  startedAt: number;
  elapsedMs: number;
  pressure: number;     // 0..1 — context fill
  activity: number;     // 0..1 — tokens/sec normalized
  health: number;       // 0..1 — recent success rate
  score?: number;       // 0..1 — final scorecard
  cost: number;         // USD accumulated
  thought?: string;     // current planning text
}

export interface LogEntry {
  ts: number;
  taskId: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
}

// ── Phase → color mapping (mirrors :root vars in app.css) ─────────────────────
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
export const connectionState = writable<'connecting' | 'connected' | 'offline' | 'mock'>(
  'connecting'
);

// ── Derived: the active agent (drives the Forge) ──────────────────────────────
export const activeAgent: Readable<AgentState | null> = derived(
  [agents, activeAgentId],
  ([$agents, $activeId]) => {
    if ($activeId && $agents.has($activeId)) return $agents.get($activeId)!;
    // Fallback: first running agent
    for (const a of $agents.values()) if (a.status === 'running') return a;
    // Or first agent at all
    return $agents.values().next().value ?? null;
  }
);

// ── Derived: aggregate stats ──────────────────────────────────────────────────
export const stats = derived(agents, ($agents) => {
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
  return { running, queued, completed, failed, total: $agents.size, totalCost };
});

// ── WebSocket client ──────────────────────────────────────────────────────────
let ws: WebSocket | null = null;
let reconnectDelay = 1000;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let mockTimer: ReturnType<typeof setInterval> | null = null;

function applyMessage(msg: any) {
  // The Rust side sends TaskStatus + AgentEvent variants.
  // For now we accept the shape `{ type, taskId, ...payload }`.
  if (!msg || typeof msg !== 'object' || !msg.type) return;

  agents.update((m) => {
    const next = new Map(m);
    const existing = next.get(msg.taskId);
    switch (msg.type) {
      case 'queued':
        next.set(msg.taskId, makeAgent(msg));
        break;
      case 'running':
      case 'phase':
      case 'progress':
        if (existing) {
          next.set(msg.taskId, {
            ...existing,
            status: 'running',
            phase: msg.phase ?? existing.phase,
            attempt: msg.attempt ?? existing.attempt,
            pressure: msg.pressure ?? existing.pressure,
            activity: msg.activity ?? existing.activity,
            thought: msg.thought ?? existing.thought,
            cost: msg.cost ?? existing.cost,
            elapsedMs: Date.now() - existing.startedAt
          });
        } else {
          next.set(msg.taskId, makeAgent(msg));
        }
        break;
      case 'completed':
      case 'failed':
      case 'cancelled':
        if (existing) {
          next.set(msg.taskId, {
            ...existing,
            status: msg.type,
            score: msg.score,
            cost: msg.cost ?? existing.cost
          });
        }
        break;
      case 'log':
        logs.update((l) => {
          const entry: LogEntry = {
            ts: Date.now(),
            taskId: msg.taskId,
            level: msg.level ?? 'info',
            message: msg.message ?? ''
          };
          // Keep last 200 entries
          return [...l.slice(-199), entry];
        });
        break;
    }
    return next;
  });
}

function makeAgent(msg: any): AgentState {
  return {
    id: msg.taskId,
    goal: msg.goal ?? 'unnamed task',
    repo: msg.repo ?? '',
    status: msg.type === 'queued' ? 'queued' : 'running',
    phase: msg.phase ?? 'Boot',
    attempt: msg.attempt ?? 1,
    branch: msg.branch ?? '',
    startedAt: Date.now(),
    elapsedMs: 0,
    pressure: msg.pressure ?? 0.1,
    activity: msg.activity ?? 0.0,
    health: msg.health ?? 0.85,
    cost: msg.cost ?? 0,
    thought: msg.thought
  };
}

function connect() {
  if (!browser) return;
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

  connectionState.set('connecting');
  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  const url = `${proto}://${location.host}/ws/tasks`;
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
      applyMessage(JSON.parse(e.data));
    } catch {
      /* malformed frame — ignore */
    }
  };

  ws.onclose = () => {
    ws = null;
    connectionState.set('offline');
    scheduleReconnect();
  };

  ws.onerror = () => {
    // Don't escalate — onclose will fire and trigger reconnect.
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    reconnectDelay = Math.min(reconnectDelay * 2, 30000);
    connect();
    // If we still can't connect after the first attempt, fall back to mock
    if (!ws) startMockData();
  }, reconnectDelay);
}

// ── Mock data generator (for standalone preview) ──────────────────────────────
function startMockData() {
  if (mockTimer) return;
  connectionState.set('mock');

  const phases: Phase[] = [
    'Boot',
    'Discovery',
    'Planning',
    'Implementation',
    'Testing',
    'Conclusion'
  ];

  const mockAgents = [
    {
      id: 'demo-1',
      goal: 'Add Postgres-backed semantic cache to the RAG pipeline',
      repo: '~/kyro',
      branch: 'feat/pg-cache'
    },
    {
      id: 'demo-2',
      goal: 'Refactor the encoder hot path to use NEON 32-wide unroll',
      repo: '~/vectro',
      branch: 'perf/neon-unroll'
    },
    {
      id: 'demo-3',
      goal: 'Wire OTel trace export from /generate spans',
      repo: '~/kairu',
      branch: 'feat/otel'
    }
  ];

  // Initialize
  agents.update((m) => {
    const next = new Map(m);
    for (const seed of mockAgents) {
      next.set(seed.id, {
        id: seed.id,
        goal: seed.goal,
        repo: seed.repo,
        branch: seed.branch,
        status: 'running',
        phase: 'Discovery',
        attempt: 1,
        startedAt: Date.now() - Math.random() * 60000,
        elapsedMs: 0,
        pressure: 0.2 + Math.random() * 0.3,
        activity: 0.3 + Math.random() * 0.4,
        health: 0.85,
        cost: 0.001 + Math.random() * 0.02,
        thought: 'Reading repository structure to identify entry points…'
      });
    }
    return next;
  });

  activeAgentId.set('demo-1');

  // Tick: organic pressure/activity drift, occasional phase transitions
  let tick = 0;
  mockTimer = setInterval(() => {
    tick++;
    agents.update((m) => {
      const next = new Map(m);
      for (const [id, a] of next) {
        if (a.status !== 'running') continue;

        // Smooth random walk on pressure (0..1, drift toward 0.6, hard ceiling)
        const targetP = 0.6;
        const newPressure = Math.max(
          0.05,
          Math.min(0.95, a.pressure + (targetP - a.pressure) * 0.02 + (Math.random() - 0.5) * 0.04)
        );

        // Activity oscillates with a bit of noise
        const newActivity = Math.max(
          0,
          Math.min(1, a.activity + (Math.sin(tick * 0.1 + id.length) * 0.05 + (Math.random() - 0.5) * 0.08))
        );

        // Phase progression — every ~30 ticks (15s) advance one
        let newPhase = a.phase;
        let newThought = a.thought;
        if (tick % 30 === 0) {
          const idx = phases.indexOf(a.phase);
          const nextIdx = Math.min(idx + 1, phases.length - 1);
          newPhase = phases[nextIdx];
          newThought = THOUGHT_TEMPLATES[newPhase] ?? a.thought;
        }

        next.set(id, {
          ...a,
          pressure: newPressure,
          activity: newActivity,
          phase: newPhase,
          thought: newThought,
          elapsedMs: Date.now() - a.startedAt,
          cost: a.cost + 0.00005 * newActivity
        });
      }
      return next;
    });

    // Occasional log lines
    if (tick % 5 === 0) {
      const taskIds = Array.from(mockAgents.map((a) => a.id));
      const taskId = taskIds[Math.floor(Math.random() * taskIds.length)];
      const sample = MOCK_LOG_LINES[Math.floor(Math.random() * MOCK_LOG_LINES.length)];
      logs.update((l) => [
        ...l.slice(-199),
        {
          ts: Date.now(),
          taskId,
          level: sample.level,
          message: sample.message
        }
      ]);
    }
  }, 500);
}

const THOUGHT_TEMPLATES: Record<Phase, string> = {
  Boot: 'Loading task context, allowed dirs, and prior patterns…',
  Discovery: 'Reading repository structure to identify entry points…',
  Planning: 'Decomposing the goal into discrete edits with concrete success criteria…',
  Implementation: 'Editing source files and re-running tests after each batch…',
  Testing: 'Running cargo nextest + clippy to verify the change passes all gates…',
  Conclusion: 'Summarizing the change and preparing the PR description…'
};

const MOCK_LOG_LINES: { level: 'info' | 'warn' | 'error' | 'debug'; message: string }[] = [
  { level: 'info', message: 'Read 14 files, 2.3k lines analyzed' },
  { level: 'info', message: 'Plan generated: 4 edits across 2 files' },
  { level: 'debug', message: 'Token pressure: 42% (within budget)' },
  { level: 'info', message: 'Edit applied to crates/lopi-agent/src/runner.rs' },
  { level: 'info', message: 'cargo check: clean' },
  { level: 'warn', message: 'clippy: 1 hint in lib.rs:47 (auto-fixed)' },
  { level: 'info', message: 'cargo nextest: 39 passed, 0 failed' },
  { level: 'info', message: 'Eviction fired: 12 turns reclaimed (4.2k tokens)' },
  { level: 'debug', message: 'Cache hit on system prompt — 1850 tokens saved' }
];

// ── Public API ────────────────────────────────────────────────────────────────
export function init() {
  if (!browser) return;
  connect();
  // If WS doesn't open in 1.5s, also start mock so the UI is never empty
  setTimeout(() => {
    if (!ws || ws.readyState !== WebSocket.OPEN) startMockData();
  }, 1500);
}

export function selectAgent(id: string) {
  activeAgentId.set(id);
}

export function postTask(goal: string, repo: string, priority: 'low' | 'normal' | 'high' = 'normal') {
  if (!browser) return Promise.reject('not-browser');
  return fetch('/api/tasks', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ goal, repo, priority })
  });
}
