/**
 * Wire-message parser — defensive runtime validation for the WebSocket stream.
 *
 * Why a runtime parser: TypeScript types disappear at runtime. A malformed
 * frame from a misconfigured server would otherwise corrupt the store. This
 * module narrows `unknown` → `WireMessage` with explicit checks, returning
 * `null` on any deviation. The store treats `null` as a no-op.
 */
import type {
  AgentEvent,
  LogLevel,
  Phase,
  Priority,
  SnapshotMessage,
  TaskStatus,
  WireMessage
} from './types';

// ── Type guards ───────────────────────────────────────────────────────────────
function isObject(x: unknown): x is Record<string, unknown> {
  return typeof x === 'object' && x !== null && !Array.isArray(x);
}

function isString(x: unknown): x is string {
  return typeof x === 'string';
}

function isNumber(x: unknown): x is number {
  return typeof x === 'number' && Number.isFinite(x);
}

const PRIORITIES: ReadonlySet<Priority> = new Set(['Low', 'Normal', 'High', 'Critical']);
const LOG_LEVELS: ReadonlySet<LogLevel> = new Set(['info', 'warn', 'error', 'debug']);
const UNIT_STATUSES: ReadonlySet<string> = new Set([
  'Queued',
  'Planning',
  'Implementing',
  'Testing',
  'Scoring',
  'RolledBack'
]);

// ── TaskStatus parser ─────────────────────────────────────────────────────────
export function parseTaskStatus(raw: unknown): TaskStatus | null {
  if (isString(raw)) {
    return UNIT_STATUSES.has(raw) ? (raw as TaskStatus) : null;
  }
  if (!isObject(raw)) return null;

  if (isObject(raw.Retrying) && isNumber(raw.Retrying.attempt)) {
    return { Retrying: { attempt: raw.Retrying.attempt } };
  }
  if (
    isObject(raw.Success) &&
    isString(raw.Success.branch) &&
    (raw.Success.pr_url === null || isString(raw.Success.pr_url))
  ) {
    return {
      Success: { branch: raw.Success.branch, pr_url: raw.Success.pr_url as string | null }
    };
  }
  if (isObject(raw.Failed) && isString(raw.Failed.reason)) {
    return { Failed: { reason: raw.Failed.reason } };
  }
  return null;
}

// ── TaskStatus → Phase mapping ────────────────────────────────────────────────
export function taskStatusToPhase(status: TaskStatus | string | null | undefined): Phase {
  if (!status) return 'Boot';
  if (typeof status === 'string') {
    switch (status) {
      case 'Queued':
        return 'Boot';
      case 'Planning':
        return 'Planning';
      case 'Implementing':
        return 'Implementation';
      case 'Testing':
        return 'Testing';
      case 'Scoring':
      case 'RolledBack':
        return 'Conclusion';
      default:
        return 'Boot';
    }
  }
  if (typeof status === 'object') {
    if ('Retrying' in status) return 'Discovery';
    if ('Success' in status) return 'Conclusion';
    if ('Failed' in status) return 'Conclusion';
  }
  return 'Boot';
}

// ── TaskStatus → terminal-state classification ────────────────────────────────
export function isTerminalStatus(status: TaskStatus | string | null | undefined): boolean {
  if (!status) return false;
  if (typeof status === 'string') return status === 'RolledBack';
  if (typeof status === 'object') return 'Success' in status || 'Failed' in status;
  return false;
}

// ── AgentEvent parser ─────────────────────────────────────────────────────────
export function parseAgentEvent(raw: Record<string, unknown>): AgentEvent | null {
  if (!isString(raw.type)) return null;

  // Most events carry a task_id.
  const tid = raw.task_id;
  switch (raw.type) {
    case 'task_queued': {
      if (!isString(tid) || !isString(raw.goal) || !isString(raw.priority)) return null;
      if (!PRIORITIES.has(raw.priority as Priority)) return null;
      return {
        type: 'task_queued',
        task_id: tid,
        goal: raw.goal,
        priority: raw.priority as Priority
      };
    }
    case 'task_started': {
      if (!isString(tid) || !isNumber(raw.attempt) || !isString(raw.branch)) return null;
      return { type: 'task_started', task_id: tid, attempt: raw.attempt, branch: raw.branch };
    }
    case 'status_changed': {
      if (!isString(tid) || !isNumber(raw.attempt)) return null;
      const st = parseTaskStatus(raw.status);
      if (!st) return null;
      return { type: 'status_changed', task_id: tid, status: st, attempt: raw.attempt };
    }
    case 'log_line': {
      if (!isString(tid) || !isString(raw.line) || !isString(raw.level) || !isString(raw.ts)) return null;
      if (!LOG_LEVELS.has(raw.level as LogLevel)) return null;
      return {
        type: 'log_line',
        task_id: tid,
        line: raw.line,
        level: raw.level as LogLevel,
        ts: raw.ts
      };
    }
    case 'score_updated': {
      if (
        !isString(tid) ||
        !isNumber(raw.test_pass_rate) ||
        !isNumber(raw.lint_errors) ||
        !isNumber(raw.diff_lines)
      ) {
        return null;
      }
      return {
        type: 'score_updated',
        task_id: tid,
        test_pass_rate: raw.test_pass_rate,
        lint_errors: raw.lint_errors,
        diff_lines: raw.diff_lines
      };
    }
    case 'task_completed': {
      if (!isString(tid) || !isNumber(raw.total_attempts)) return null;
      const outcome = parseTaskStatus(raw.outcome);
      if (!outcome) return null;
      return {
        type: 'task_completed',
        task_id: tid,
        outcome,
        total_attempts: raw.total_attempts
      };
    }
    case 'task_cancelled': {
      if (!isString(tid)) return null;
      return { type: 'task_cancelled', task_id: tid };
    }
    case 'pool_stats': {
      if (
        !isNumber(raw.running) ||
        !isNumber(raw.queued) ||
        !isNumber(raw.succeeded) ||
        !isNumber(raw.failed) ||
        !isNumber(raw.uptime_secs)
      ) {
        return null;
      }
      return {
        type: 'pool_stats',
        running: raw.running,
        queued: raw.queued,
        succeeded: raw.succeeded,
        failed: raw.failed,
        uptime_secs: raw.uptime_secs
      };
    }
    case 'turn_metrics': {
      if (
        !isString(tid) ||
        !isNumber(raw.pressure) ||
        !isNumber(raw.activity) ||
        !isNumber(raw.tokens_per_sec) ||
        !isNumber(raw.cost_usd)
      ) {
        return null;
      }
      return {
        type: 'turn_metrics',
        task_id: tid,
        pressure: clamp01(raw.pressure),
        activity: clamp01(raw.activity),
        tokens_per_sec: raw.tokens_per_sec,
        cost_usd: raw.cost_usd,
        // Older emitters omit `tokens` — default to 0 instead of dropping the frame.
        tokens: isNumber(raw.tokens) ? raw.tokens : 0
      };
    }
    default:
      return null;
  }
}

// ── Snapshot parser ───────────────────────────────────────────────────────────
export function parseSnapshot(raw: Record<string, unknown>): SnapshotMessage | null {
  if (raw.type !== 'snapshot') return null;
  if (!Array.isArray(raw.tasks)) return null;
  if (!isObject(raw.stats)) return null;
  const s = raw.stats;
  if (
    !isNumber(s.running) ||
    !isNumber(s.queued) ||
    !isNumber(s.succeeded) ||
    !isNumber(s.failed) ||
    !isNumber(s.uptime_secs)
  ) {
    return null;
  }

  const tasks = raw.tasks.flatMap((t) => {
    if (!isObject(t)) return [];
    if (!isString(t.id) || !isString(t.goal) || !isString(t.created_at)) return [];
    return [
      {
        id: t.id,
        goal: t.goal,
        status: (parseTaskStatus(t.status) ?? (typeof t.status === 'string' ? t.status : 'Queued')) as TaskStatus | string,
        created_at: t.created_at
      }
    ];
  });

  return {
    type: 'snapshot',
    tasks,
    stats: {
      running: s.running,
      queued: s.queued,
      succeeded: s.succeeded,
      failed: s.failed,
      uptime_secs: s.uptime_secs
    }
  };
}

// ── Top-level parser ──────────────────────────────────────────────────────────
export function parseWireMessage(raw: unknown): WireMessage | null {
  if (!isObject(raw)) return null;
  if (raw.type === 'snapshot') return parseSnapshot(raw);
  return parseAgentEvent(raw);
}

// ── Helpers ───────────────────────────────────────────────────────────────────
function clamp01(n: number): number {
  if (n < 0) return 0;
  if (n > 1) return 1;
  return n;
}
