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

/**
 * Optional fields mirroring `crates/lopi-ui/src/web/types.rs::CreateTaskRequest`.
 * Types only — no UI binds to these yet. `max_iterations: 0` is the
 * infinite-loop sentinel (matches the Rust-side decision), not "no loop."
 */
export interface CreateTaskOptions {
  verifier_required?: boolean;
  verifier_model?: string;
  verifier_effort?: string;
  report?: string;
  max_iterations?: number;
  model?: string;
  effort?: string;
  /** Guardrail precondition — a shell command that must exit 0 before the loop starts. */
  gate?: string;
  /** Guardrail exit-condition — a shell command; exit 0 ends the loop early as a success. */
  until?: string;
  /** On-fail policy: 'stop' (default, unchanged retry behavior), 'continue' (skip the backoff pause), or 'backoff' (explicit pacing). */
  on_fail?: 'stop' | 'continue' | 'backoff';
}

export const createTask = (
  goal: string,
  repo: string,
  priority = 'normal',
  opts: CreateTaskOptions = {}
) => request<{ id?: string }>('/api/tasks', json('POST', { goal, repo, priority, ...opts }));

// Phase 11 — plan approval gate.
export const approvePlan = (id: string) =>
  request<{ decision?: string }>(
    `/api/tasks/${encodeURIComponent(id)}/plan/approve`,
    { method: 'POST' }
  );
export const rejectPlan = (id: string) =>
  request<{ decision?: string }>(
    `/api/tasks/${encodeURIComponent(id)}/plan/reject`,
    { method: 'POST' }
  );

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

// ── Loop Engineering ──────────────────────────────────────────────────────────
/** One pickable trust level on the L1–L4 autonomy ladder. */
export interface AutonomyOption {
  value: string;
  tag: string;
  label: string;
  opens_pr: boolean;
  requires_verifier: boolean;
  allows_auto_merge: boolean;
}

/** One pickable self-prompting strategy (S1–S4) with a live self-prompt preview. */
export interface SelfPromptOption {
  value: string;
  tag: string;
  label: string;
  description: string;
  preview: string;
}

/** Effective `.lopi/loop.toml` plus validation envelope. */
export interface LoopConfig {
  autonomy_level: string;
  autonomy_tag: string;
  autonomy_label: string;
  self_prompt: string;
  self_prompt_tag: string;
  self_prompt_label: string;
  escalate_strategy: boolean;
  escalation_ladder: { attempt: number; tag: string; label: string }[];
  vision_path: string | null;
  skills_enabled: string[];
  rules_enabled: string[];
  permission_allow: string[];
  permission_deny: string[];
  no_progress_limit: number;
  max_iterations: number;
  budget_tokens: number;
  valid: boolean;
  issues: string[];
}

/** A schedule projected for the Loop screen, carrying its trust level. */
export interface LoopSchedule {
  id: string;
  name: string;
  goal: string;
  cron: string;
  enabled: boolean;
  autonomy_level: string;
  autonomy_tag: string;
  autonomy_label: string;
}

/** A Konjo quality wall surfaced as a loop guardrail gate. */
export interface LoopGate {
  wall: string;
  name: string;
  checks: string;
}

/** The full Loop Engineering snapshot from `GET /api/loop-engineering`. */
export interface LoopSnapshot {
  repo: string;
  config: LoopConfig;
  autonomy_levels: AutonomyOption[];
  self_prompt_strategies: SelfPromptOption[];
  skills: { name: string; description: string }[];
  rules: { name: string }[];
  schedules: LoopSchedule[];
  gates: LoopGate[];
}

/** Headline KPI tiles for the Loop Health view. */
export interface LoopHealthStats {
  runs: number;
  attempts: number;
  success_rate: number;
  verifier_pass_rate: number;
  verifier_total: number;
  spend_usd: number;
  tokens: number;
}

/** One attempt in the score/diff timeline (oldest → newest). */
export interface LoopHealthAttempt {
  task_id: string;
  attempt: number;
  test_pass_rate: number;
  lint_errors: number;
  diff_lines: number;
  outcome: string;
  created_at: string;
}

/** One sample in the token/cost burn series (oldest → newest). */
export interface LoopHealthBurn {
  cost_usd: number;
  tokens: number;
  context_pressure: number;
  timestamp: string;
}

/** The loop-health snapshot from `GET /api/loop-engineering/health`. */
export interface LoopHealth {
  stats: LoopHealthStats;
  attempts: LoopHealthAttempt[];
  outcomes: { label: string; count: number }[];
  burn: LoopHealthBurn[];
}

/** One run (task) summarised for the run picker. */
export interface LoopRun {
  task_id: string;
  goal: string;
  status: string;
  attempts: number;
  best_score: number;
  final_outcome: string;
  last_at: string;
}

/** The verifier verdict grafted onto an attempt in a run trace. */
export interface LoopRunVerifier {
  passed: boolean;
  confidence: number;
  gaps: string[];
  fix_hints: string[];
}

/** One attempt in a run's drill-down trace. */
export interface LoopRunAttempt {
  attempt: number;
  test_pass_rate: number;
  lint_errors: number;
  diff_lines: number;
  outcome: string;
  errors: string[];
  verifier: LoopRunVerifier | null;
  tokens: number;
  cost_usd: number;
  created_at: string;
}

/** A single run's attempt-by-attempt trace. */
export interface LoopRunTrace {
  task_id: string;
  goal: string;
  status: string;
  attempts: LoopRunAttempt[];
}

export const getLoopEngineering = () => request<LoopSnapshot>('/api/loop-engineering');
export const getLoopHealth = () => request<LoopHealth>('/api/loop-engineering/health');
export const getLoopRuns = () => request<{ runs: LoopRun[] }>('/api/loop-engineering/runs');
export const getLoopRunTrace = (id: string) =>
  request<LoopRunTrace>(`/api/loop-engineering/runs/${encodeURIComponent(id)}`);
export const setScheduleAutonomy = (id: string, level: string) =>
  request<{ id: string; autonomy_level: string }>(
    `/api/schedules/${encodeURIComponent(id)}/autonomy`,
    json('POST', { level })
  );

/** Set the repo's self-prompting strategy; persists to `.lopi/loop.toml`. */
export const setLoopStrategy = (strategy: string) =>
  request<{ self_prompt: string; self_prompt_tag: string; self_prompt_label: string }>(
    '/api/loop-engineering/strategy',
    json('POST', { strategy })
  );

/** Toggle adaptive strategy escalation; persists to `.lopi/loop.toml`. */
export const setLoopEscalation = (enabled: boolean) =>
  request<{ escalate_strategy: boolean }>(
    '/api/loop-engineering/escalation',
    json('POST', { enabled })
  );

// ── Repos (launch-control repo dropdown) ──────────────────────────────────────
export const listRepos = () => request<{ repos: string[] }>('/api/repos');

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

// ── Constellation router ──────────────────────────────────────────────────────
// RoutingStrategy is internally tagged: #[serde(tag = "kind", rename_all = "snake_case")]
export type RoutingStrategy =
  | { kind: 'round_robin' }
  | { kind: 'weighted_random' }
  | { kind: 'least_loaded' }
  | { kind: 'tag_match'; required_tags: string[] };

export interface ConstellationMember {
  agent_id: string;
  weight: number;
  tags: string[];
  max_concurrent: number;
}

export interface Constellation {
  name: string;
  agents: ConstellationMember[];
  routing_strategy: RoutingStrategy;
  created_at: string;
}

export interface DispatchDecision {
  agent_id: string;
  strategy: string;
  at: string;
  required_tags?: string[];
}

export interface MemberLoad {
  agent_id: string;
  in_flight: number;
  dispatched_total: number;
  max_concurrent: number;
}

export interface ConstellationStats {
  name: string;
  members: MemberLoad[];
  recent_decisions: DispatchDecision[];
}

export const listConstellations = () =>
  request<{ constellations: Constellation[] }>('/api/constellations');
export const registerConstellation = (c: {
  name: string;
  agents: ConstellationMember[];
  routing_strategy: RoutingStrategy;
}) => request<{ name: string; replaced: boolean }>('/api/constellations', json('POST', c));
export const dispatchConstellation = (name: string, requiredTags: string[] = []) =>
  request<DispatchDecision>(
    `/api/constellation/${encodeURIComponent(name)}/dispatch`,
    json('POST', { required_tags: requiredTags })
  );
export const constellationStats = (name: string) =>
  request<ConstellationStats>(`/api/constellation/${encodeURIComponent(name)}/stats`);

/** Free-form GET for the Debug tab's API console. Returns raw parsed JSON. */
export const rawGet = (path: string) => request<unknown>(path);
