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
  /** Backend-1 — the caller-supplied identity echoed back by `createTask`, if any. */
  client_ref: string | null;
}

export const listTasks = () => request<{ tasks: TaskRow[] }>('/api/tasks');
export const getTask = (id: string) => request<TaskRow>(`/api/tasks/${encodeURIComponent(id)}`);
export const deleteTask = (id: string) =>
  request<{ cancelled?: boolean }>(`/api/tasks/${encodeURIComponent(id)}`, { method: 'DELETE' });

/**
 * Optional fields mirroring `crates/lopi-ui/src/web/types.rs::CreateTaskRequest`.
 * `max_iterations: 0` is the infinite-loop sentinel (matches the Rust-side
 * decision), not "no loop."
 */
/** One tier-tagged, machine-checkable criterion. Mirrors
 *  `crates/lopi-core/src/acceptance.rs::CheckSpec` (serde-tagged on `kind`,
 *  snake_case). The tiered eval executor decides at the cheapest tier that can
 *  decide — objective criteria route to `execution_ok`/`shell`, the judge is
 *  reserved for genuine judgment. */
export type CheckSpec =
  | { kind: 'execution_ok' }
  | { kind: 'shell'; cmd: string }
  | { kind: 'judge'; rubric: { name: string; criteria: string[] } }
  | { kind: 'suite'; name: string };

/** One acceptance check. Mirrors `acceptance.rs::AcceptanceCheck`. `tier` is the
 *  UI `EvalTier` union and always matches `spec.kind`. */
export interface AcceptanceCheck {
  tier: 'base' | 'test' | 'judge' | 'suite';
  spec: CheckSpec;
  weight: number;
  required: boolean;
}

/** The machine-checkable success condition the executor scores a loop against.
 *  Mirrors `acceptance.rs::Acceptance` — the same schema at loop and stack
 *  scope. This is what finally makes the eval UI execute instead of being an
 *  inert checklist. */
export interface Acceptance {
  checks: AcceptanceCheck[];
}

export interface CreateTaskOptions {
  /** Free-form constraints appended to the agent's system prompt. Mirrors
   *  `crates/lopi-ui/src/web/types.rs::CreateTaskRequest.constraints`. The
   *  unified pane-launch path (Unify-1) surfaces a branch override here — the
   *  same channel `postTask` used before it was retired — since neither
   *  `CreateTaskRequest` nor the task model has a first-class branch column. */
  constraints?: string[];
  verifier_required?: boolean;
  verifier_model?: string;
  verifier_effort?: string;
  /** A1 — operator opt-out of the fail-closed verifier. Omit (or `false`) to
   *  keep the safe default: a verifier/judge error blocks finalize, never a
   *  silent pass. */
  verifier_fail_open?: boolean;
  /** A1 — the machine-checkable goal the tiered eval executor scores against.
   *  Compiled from the card's `evals` (see `stack.ts::evalsToAcceptance`). */
  acceptance?: Acceptance;
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
  /** A3 — per-loop token budget the runner meters against, stopping with
   *  `StopReason::Budget` on exceed. Compiled from the card's budget preset
   *  (see `stack.ts::budgetToTokens`); omitted for the inherit/unlimited
   *  presets. Mirrors `crates/lopi-ui/src/web/types.rs::budget_tokens`. */
  budget_tokens?: number;
  /**
   * Backend-1 — opaque caller identity (e.g. a loop-stack card id), echoed
   * back verbatim and persisted alongside the task. lopi never interprets
   * this string; it exists purely so a caller can durably associate its
   * own concept of "what asked for this" with the `TaskId` the pool
   * assigns.
   */
  client_ref?: string;
}

/** Mirrors `crates/lopi-ui/src/web/types.rs::CreateTaskResponse` exactly. */
export interface CreateTaskResponse {
  /**
   * Id generated for *this* request. When `duplicate_of` is set, this id
   * was never actually queued — see `duplicate_of`.
   */
  id: string;
  goal: string;
  /** `true` if newly queued; `false` if this request deduped against an already-queued identical goal. */
  queued: boolean;
  /** Set when `queued` is false — the id of the task actually running. Callers needing "the real task id" must prefer this over `id` when present. */
  duplicate_of: string | null;
  /** Echoes `CreateTaskOptions.client_ref` verbatim. */
  client_ref: string | null;
}

export const createTask = (
  goal: string,
  repo: string,
  priority = 'normal',
  opts: CreateTaskOptions = {}
) =>
  request<CreateTaskResponse>(
    '/api/tasks',
    // Backend-1: an empty `repo` must be omitted, not sent as `""`. The
    // server's `CreateTaskRequest.repo` is `Option<String>` and falls back
    // to its own configured repo path when the key is absent — but a
    // present empty string deserializes to `Some("")`, which the runner
    // then tries to `git2::Repository::open("")` and fails outright. Every
    // caller here (Tasks page's blank-by-default repo field, and every
    // stack card that hasn't overridden the pane's own blank default) hits
    // this the moment nothing has actually set a repo yet.
    json('POST', { goal, priority, ...(repo ? { repo } : {}), ...opts })
  );

/**
 * The task id a caller should actually track: `duplicate_of` when the
 * create request deduped against an already-queued identical goal (`id` in
 * that case was never queued at all), otherwise `id`. Pure — see
 * `stores/stack.test.ts` for the dedup-collision case this guards against.
 */
export function effectiveTaskId(resp: CreateTaskResponse): string {
  return resp.duplicate_of ?? resp.id;
}

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

// The Constellation router client (listConstellations / registerConstellation /
// dispatchConstellation / constellationStats + its types) was removed in
// Ops-2-findings-closure Phase 4: the backend never registered those routes, so
// every call fell through to the SPA static fallback and returned HTML, which
// broke JSON decoding (High-severity bug #2). It had zero UI callers.

/** Free-form GET for the Debug tab's API console. Returns raw parsed JSON. */
export const rawGet = (path: string) => request<unknown>(path);
