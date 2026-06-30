/**
 * WebSocket client and opt-in demo data generator.
 *
 * The demo generator (initMock) NEVER auto-triggers. It exists only so the
 * Forge can be shown without a backend, and is invoked solely when the page is
 * loaded with `?demo=1`. A dead/unreachable backend shows an honest
 * offline/empty state, not fabricated `demo-*` agents.
 *
 * Separated from agents.ts to keep store module size under 500 lines.
 */
import { browser } from '$app/environment';
import type { WireMessage, TaskStatus } from '$lib/types';

export type MessageHandler = (msg: WireMessage) => void;

let ws: WebSocket | null = null;
let reconnectDelay = 1000;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let mockTimer: ReturnType<typeof setInterval> | null = null;
let mockTick = 0;
let messageHandler: MessageHandler | null = null;

export function setMessageHandler(handler: MessageHandler) {
  messageHandler = handler;
}

export function connect() {
  if (!browser) return;
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  const url = `${proto}://${location.host}/ws`;
  try {
    ws = new WebSocket(url);
  } catch {
    scheduleReconnect();
    return;
  }

  ws.onopen = () => {
    reconnectDelay = 1000;
    if (mockTimer) {
      clearInterval(mockTimer);
      mockTimer = null;
    }
  };

  ws.onmessage = (e) => {
    if (!messageHandler) return;
    try {
      const raw = JSON.parse(e.data);
      messageHandler(raw);
    } catch {
      console.debug('[lopi] dropped non-JSON frame');
    }
  };

  ws.onclose = () => {
    ws = null;
    scheduleReconnect();
  };

  ws.onerror = () => {
    /* onclose fires too */
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    reconnectDelay = Math.min(reconnectDelay * 2, 30000);
    connect();
    // No mock fallback: a dead backend shows an honest offline/empty state.
    // Demo data is opt-in only, via initMock() behind the ?demo=1 flag.
  }, reconnectDelay);
}

function startMockData() {
  if (mockTimer) return;

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

  // Emit snapshot first
  if (messageHandler) {
    messageHandler({
      type: 'snapshot',
      tasks: seedAgents.map((a) => ({
        id: a.task_id,
        goal: a.goal,
        status: 'Planning' as TaskStatus,
        created_at: new Date(Date.now() - Math.random() * 60000).toISOString()
      })),
      stats: { running: seedAgents.length, queued: 0, succeeded: 12, failed: 1, uptime_secs: 1820 }
    });
  }

  // Emit task_started for each agent so branch/repo are set in the store
  for (const seed of seedAgents) {
    if (messageHandler) {
      messageHandler({
        type: 'task_started',
        task_id: seed.task_id,
        attempt: 1,
        branch: seed.branch,
        repo: seed.repo
      });
    }
  }

  // Phase 11 demo — the first agent pauses at the plan approval gate. With no
  // backend to click in preview mode it auto-resumes after ~17s (see the
  // `gatedId` guard below) so the board keeps moving.
  const gatedId = seedAgents[0].task_id;
  if (messageHandler) {
    messageHandler({
      type: 'plan_proposed',
      task_id: gatedId,
      attempt: 1,
      steps: [
        'Add a RedisCache struct wrapping the connection pool',
        'Wire it into the RAG retrieval path behind a feature flag',
        'Add an eviction policy with a configurable TTL',
        'Cover the cache hit / miss paths with unit tests'
      ],
      plan: 'Add a Redis-backed semantic cache to the RAG pipeline.'
    });
    messageHandler({
      type: 'status_changed',
      task_id: gatedId,
      status: { AwaitingPlanApproval: { attempt: 1 } } as unknown as TaskStatus,
      attempt: 1
    });
  }

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
      if (!messageHandler) continue;

      // Phase progression every 30 ticks. The gated agent is held at the
      // approval gate until tick 30, then rejoins the normal cycle.
      if (mockTick % 30 === 5 && !(seed.task_id === gatedId && mockTick < 30)) {
        const idx = Math.floor(mockTick / 30) % phaseCycle.length;
        messageHandler({
          type: 'status_changed',
          task_id: seed.task_id,
          status: phaseCycle[idx],
          attempt: 1
        });
      }

      // Turn metrics every 2 ticks (1s)
      if (mockTick % 2 === 0) {
        const tNorm = mockTick * 0.05;
        const pressure = clamp01(
          0.45 + Math.sin(tNorm + seed.task_id.length) * 0.18 + (Math.random() - 0.5) * 0.04
        );
        const activity = clamp01(
          0.55 + Math.cos(tNorm * 0.7 + seed.task_id.length) * 0.3 + (Math.random() - 0.5) * 0.06
        );
        messageHandler({
          type: 'turn_metrics',
          task_id: seed.task_id,
          pressure,
          activity,
          tokens_per_sec: activity * 80,
          cost_usd: mockTick * 0.00015
        });
      }

      // Log every 4 ticks
      if (mockTick % 4 === 1) {
        const tpl = logTemplates[Math.floor(Math.random() * logTemplates.length)];
        messageHandler({
          type: 'log_line',
          task_id: seed.task_id,
          line: tpl.line,
          level: tpl.level,
          ts: new Date().toISOString()
        });
      }

      // Score updates every 50 ticks
      if (mockTick % 50 === 25) {
        messageHandler({
          type: 'score_updated',
          task_id: seed.task_id,
          test_pass_rate: 0.82 + Math.random() * 0.15,
          lint_errors: Math.floor(Math.random() * 3),
          diff_lines: Math.floor(20 + Math.random() * 80)
        });
      }

      // Verifier verdict shortly after each score — usually passes.
      if (mockTick % 50 === 32) {
        const passed = Math.random() > 0.3;
        messageHandler({
          type: 'verifier_verdict',
          task_id: seed.task_id,
          passed,
          gaps: passed ? [] : ['error path for empty input is untested', 'public fn missing rustdoc'],
          fix_hints: passed ? [] : ['add a unit test covering the empty case']
        });
      }
    }

    // Occasional fleet-wide budget breach to exercise the alert toast.
    if (mockTick % 140 === 70 && messageHandler) {
      messageHandler({
        type: 'budget_exceeded',
        task_id: null,
        scope: 'fleet',
        limit_usd: 5.0,
        burned_usd: 5.0 + Math.random() * 0.8
      });
    }
  }, 500);
}

export function initMock() {
  startMockData();
}

export function getConnectionState(): 'connecting' | 'connected' | 'offline' | 'mock' {
  if (mockTimer) return 'mock';
  if (!ws) return 'offline';
  if (ws.readyState === WebSocket.OPEN) return 'connected';
  return 'connecting';
}

function clamp01(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return Math.max(0, Math.min(1, n));
}
