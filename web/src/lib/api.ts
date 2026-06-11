/**
 * Typed REST client for the lopi web API.
 *
 * Every tab beyond the Forge (Tasks, Schedules, Logs, Config, Debug) talks
 * to the backend through this module so response shapes live in one place.
 * All functions throw `ApiError` on non-2xx responses; callers render the
 * message in their local error state.
 */
/** Error thrown for non-2xx API responses — carries the HTTP status. */
export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = 'ApiError';
  }
}

// No `$app/environment` import here — pages only call these from onMount
// (browser-only), and keeping the module SvelteKit-free lets the standalone
// tsx test scripts exercise it with a mocked global fetch.
async function request<T>(path: string, init?: RequestInit): Promise<T> {
  if (typeof fetch !== 'function') throw new ApiError(0, 'fetch unavailable');
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    throw new ApiError(0, 'backend unreachable');
  }
  let body: unknown = null;
  try {
    body = await res.json();
  } catch {
    /* non-JSON body — leave null */
  }
  if (!res.ok) {
    const msg =
      body && typeof body === 'object' && 'error' in body
        ? String((body as { error: unknown }).error)
        : `HTTP ${res.status}`;
    throw new ApiError(res.status, msg);
  }
  return body as T;
}

const json = (method: string, payload: unknown): RequestInit => ({
  method,
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify(payload)
});

// ── Tasks ─────────────────────────────────────────────────────────────────────
export interface TaskRow {
  id: string;
  goal: string;
  status: string;
  created_at: string;
  completed_at: string | null;
}

export const listTasks = () => request<{ tasks: TaskRow[] }>('/api/tasks');
export const getTask = (id: string) => request<TaskRow>(`/api/tasks/${encodeURIComponent(id)}`);
export const deleteTask = (id: string) =>
  request<{ cancelled?: boolean }>(`/api/tasks/${encodeURIComponent(id)}`, { method: 'DELETE' });
export const createTask = (goal: string, repo: string, priority = 'normal') =>
  request<{ id?: string }>('/api/tasks', json('POST', { goal, repo, priority }));

// ── Logs ──────────────────────────────────────────────────────────────────────
export interface LogRow {
  id: number;
  task_id: string;
  ts: string;
  level: string;
  line: string;
}

export const recentLogs = (n = 500) => request<{ logs: LogRow[] }>(`/api/logs?n=${n}`);
export const taskLogs = (id: string, n = 500) =>
  request<{ task_id: string; logs: LogRow[] }>(
    `/api/tasks/${encodeURIComponent(id)}/logs?n=${n}`
  );

// ── Dead-letter queue ─────────────────────────────────────────────────────────
export interface DeadLetterRow {
  id: string;
  task_id: string;
  goal: string;
  repo_path: string | null;
  total_attempts: number;
  last_error: string;
  first_failed_at: string;
  dead_at: string;
  source: string;
}

export const listDlq = (n = 100) =>
  request<{ dead_letters: DeadLetterRow[] }>(`/api/tasks/dead-letter?n=${n}`);
export const retryDlq = (id: string) =>
  request<{ new_task_id: string; queued: boolean }>(
    `/api/tasks/dead-letter/${encodeURIComponent(id)}/retry`,
    { method: 'POST' }
  );
export const deleteDlq = (id: string) =>
  request<{ deleted: string }>(`/api/tasks/dead-letter/${encodeURIComponent(id)}`, {
    method: 'DELETE'
  });

// ── Schedules (cron) ──────────────────────────────────────────────────────────
export interface ScheduleRun {
  id?: number;
  schedule_id?: string;
  task_id?: string | null;
  fired_at?: string;
  outcome?: string;
}

export interface Schedule {
  id: string;
  name: string;
  cron: string;
  goal: string;
  repo: string | null;
  priority: string | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  next_runs: string[];
  last_run: ScheduleRun | null;
  runs?: ScheduleRun[];
}

export interface ScheduleBody {
  name: string;
  cron: string;
  goal: string;
  repo?: string;
  priority?: string;
  enabled?: boolean;
}

export const listSchedules = () => request<{ schedules: Schedule[] }>('/api/schedules');
export const createSchedule = (body: ScheduleBody) =>
  request<Schedule>('/api/schedules', json('POST', body));
export const updateSchedule = (id: string, body: ScheduleBody) =>
  request<Schedule>(`/api/schedules/${encodeURIComponent(id)}`, json('PUT', body));
export const deleteSchedule = (id: string) =>
  request<{ deleted: string }>(`/api/schedules/${encodeURIComponent(id)}`, { method: 'DELETE' });
export const enableSchedule = (id: string) =>
  request<{ id: string; enabled: boolean }>(
    `/api/schedules/${encodeURIComponent(id)}/enable`,
    { method: 'POST' }
  );
export const disableSchedule = (id: string) =>
  request<{ id: string; enabled: boolean }>(
    `/api/schedules/${encodeURIComponent(id)}/disable`,
    { method: 'POST' }
  );
export const runScheduleNow = (id: string) =>
  request<{ schedule_id: string; task_id: string | null; queued: boolean }>(
    `/api/schedules/${encodeURIComponent(id)}/run-now`,
    { method: 'POST' }
  );

// ── Config + version ──────────────────────────────────────────────────────────
export const getConfig = () =>
  request<{ config: Record<string, unknown> | null; source: string }>('/api/config');
export const getVersion = () =>
  request<{ service: string; version: string; uptime_secs: number }>('/api/version');

// ── Stats / health / cache / audit (Debug tab) ────────────────────────────────
export interface PoolStatsResponse {
  running: number;
  queued: number;
  succeeded: number;
  failed: number;
  uptime_secs: number;
  total_tokens_today: number;
  total_cost_usd_today: number;
}

export const getStats = () => request<PoolStatsResponse>('/api/stats');

export interface HealthSummary {
  total: number;
  healthy: number;
  degraded: number;
  dead: number;
}

export const healthSummary = () => request<HealthSummary>('/api/agents/health/summary');
export const cacheStats = () => request<Record<string, unknown>>('/api/cache/stats');
export const clearCache = () =>
  request<{ deleted: number }>('/api/cache', { method: 'DELETE' });

export interface AuditEvent {
  id: number;
  ts: string;
  action: string;
  subject_type: string;
  subject_id: string;
  actor: string;
  payload: unknown;
}

export const queryAudit = (n = 100) =>
  request<{ events: AuditEvent[]; next_cursor: number }>(`/api/audit?n=${n}`);

export interface PatternRow {
  id: number;
  goal_keywords: string;
  avg_attempts: number;
  success_rate: number;
  last_seen: string;
}

export const listPatterns = () => request<{ patterns: PatternRow[] }>('/api/patterns');

export interface QualityRun {
  id: number;
  spec_items: number;
  passing: number;
  failing: number;
  gaps: number;
  score: number;
  run_at: string;
}

export const qualityTrend = (limit = 20) =>
  request<{ repo: string; runs: QualityRun[] }>(`/api/quality/trend?limit=${limit}`);

// ── Tools (durable tool registry) ─────────────────────────────────────────────
export interface ToolSpec {
  name: string;
  description: string;
  parameters: unknown;
  timeout_ms: number;
  retries: number;
  updated_at?: string;
}

export const listTools = () => request<{ tools: ToolSpec[] }>('/api/tools');
export const registerTool = (spec: Omit<ToolSpec, 'updated_at'>) =>
  request<{ registered: string }>('/api/tools', json('POST', spec));
export const deleteTool = (name: string) =>
  request<{ deregistered: string }>(`/api/tools/${encodeURIComponent(name)}`, {
    method: 'DELETE'
  });

/** Free-form GET for the Debug tab's API console. Returns raw parsed JSON. */
export const rawGet = (path: string) => request<unknown>(path);
