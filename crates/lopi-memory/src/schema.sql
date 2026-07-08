CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    goal TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    source TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS attempts (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    attempt_num INTEGER NOT NULL,
    branch TEXT NOT NULL,
    score_test_pass_rate REAL,
    score_lint_errors INTEGER,
    score_diff_lines INTEGER,
    outcome TEXT NOT NULL,
    errors TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS patterns (
    id TEXT PRIMARY KEY,
    goal_keywords TEXT NOT NULL,
    successful_constraints TEXT,
    avg_attempts REAL,
    success_rate REAL,
    last_seen TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS turn_metrics (
    turn_id            TEXT PRIMARY KEY,
    task_id            TEXT NOT NULL,
    session_id         TEXT NOT NULL,
    model              TEXT NOT NULL,
    attempt_number     INTEGER NOT NULL DEFAULT 1,
    input_tokens       INTEGER NOT NULL DEFAULT 0,
    output_tokens      INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens  INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    ttft_ms            INTEGER NOT NULL DEFAULT 0,
    turn_latency_ms    INTEGER NOT NULL DEFAULT 0,
    tool_execution_ms  INTEGER NOT NULL DEFAULT 0,
    context_tokens     INTEGER NOT NULL DEFAULT 0,
    context_pressure   REAL NOT NULL DEFAULT 0,
    evictions          INTEGER NOT NULL DEFAULT 0,
    tool_calls         INTEGER NOT NULL DEFAULT 0,
    tools_parallel     INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd REAL NOT NULL DEFAULT 0,
    timestamp          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_turn_metrics_task ON turn_metrics(task_id);
CREATE INDEX IF NOT EXISTS idx_turn_metrics_ts ON turn_metrics(timestamp);

PRAGMA journal_mode=WAL;
CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON tasks(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_attempts_task_id ON attempts(task_id);
CREATE INDEX IF NOT EXISTS idx_patterns_keywords ON patterns(goal_keywords);
ALTER TABLE patterns ADD COLUMN embedding TEXT;
-- Sprint H: distinguish patterns derived from a failed-run post-mortem
-- (Claude analyzed the error log) from patterns mined from completed-task
-- statistics. The dashboard surfaces these differently.
ALTER TABLE patterns ADD COLUMN derived_from_postmortem INTEGER NOT NULL DEFAULT 0;
-- Sprint H1: user annotation for pattern validation. Values: 'approved', 'rejected', or NULL (unannotated).
ALTER TABLE patterns ADD COLUMN user_annotation TEXT;

-- Backend-1: opaque caller-supplied identity (e.g. a loop-stack card id) so a
-- client can durably map its own concept of "what requested this" onto the
-- TaskId the pool assigns, without lopi needing to understand that concept.
ALTER TABLE tasks ADD COLUMN client_ref TEXT;

-- Sprint I: Layer 5 patch stability ledger.
-- Accumulates empirical data on model-output variance per task class.
-- Drives the research dataset for which task types are safe to self-ship.
CREATE TABLE IF NOT EXISTS stability_ledger (
    id              TEXT PRIMARY KEY,
    task_goal_pfx   TEXT NOT NULL,
    model           TEXT NOT NULL,
    n_samples       INTEGER NOT NULL,
    variance_score  REAL NOT NULL,
    verdict         TEXT NOT NULL CHECK(verdict IN ('stable', 'warning', 'unstable')),
    semantic_flags  TEXT NOT NULL DEFAULT '[]',
    accepted        INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_stability_verdict ON stability_ledger(verdict);
CREATE INDEX IF NOT EXISTS idx_stability_created ON stability_ledger(created_at DESC);

CREATE TABLE IF NOT EXISTS lessons (
    id          TEXT PRIMARY KEY,
    repo_path   TEXT NOT NULL,
    category    TEXT NOT NULL CHECK(category IN ('strategy','recovery','optimization')),
    content     TEXT NOT NULL,
    task_id     TEXT,
    score       REAL NOT NULL DEFAULT 0.0,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_lessons_repo_created ON lessons(repo_path, created_at DESC);

-- Sprint M: KCQF quality check run ledger.
-- Each row = one execution of `lopi gap-fill` or `lopi check`.
-- Drives coverage trend: is the spec getting healthier over time?
CREATE TABLE IF NOT EXISTS quality_check_runs (
    id          TEXT PRIMARY KEY,
    repo_path   TEXT NOT NULL,
    spec_items  INTEGER NOT NULL DEFAULT 0,
    passing     INTEGER NOT NULL DEFAULT 0,
    failing     INTEGER NOT NULL DEFAULT 0,
    gaps        INTEGER NOT NULL DEFAULT 0,
    score       REAL NOT NULL DEFAULT 0.0,
    run_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_quality_repo_run ON quality_check_runs(repo_path, run_at DESC);

-- Sprint O: GitHub App installation ledger.
-- One row per customer per GitHub App installation event.
-- customer_id is derived from the GitHub account/org that installed the App.
CREATE TABLE IF NOT EXISTS github_installations (
    id              TEXT PRIMARY KEY,
    installation_id INTEGER NOT NULL UNIQUE,
    customer_id     TEXT NOT NULL,
    account_login   TEXT NOT NULL,
    account_type    TEXT NOT NULL CHECK(account_type IN ('User', 'Organization')),
    access_token    TEXT,
    token_expires   TEXT,
    status          TEXT NOT NULL CHECK(status IN ('active', 'suspended', 'deleted')) DEFAULT 'active',
    installed_at    TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_installations_customer ON github_installations(customer_id);
CREATE INDEX IF NOT EXISTS idx_installations_login ON github_installations(account_login);

-- P1.3 — Durable agent checkpoints. A snapshot taken before any action
-- that can fail (plan, implement, score, PR). The CLI subcommand
-- `lopi resume --agent-id` reads the most-recent row, and the HTTP
-- endpoint POST /api/agents/{id}/checkpoint writes on demand.
-- `state` mirrors lopi_core::AgentState (planning / implementing /
-- testing / scoring / done / errored).
CREATE TABLE IF NOT EXISTS agent_checkpoints (
    id           TEXT PRIMARY KEY,
    task_id      TEXT NOT NULL,
    attempt      INTEGER NOT NULL DEFAULT 0,
    state        TEXT NOT NULL,
    last_plan    TEXT,
    last_score   TEXT,
    repo_path    TEXT,
    context_hash TEXT,
    created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_checkpoints_task_created
    ON agent_checkpoints(task_id, created_at DESC);

-- P2 — Content-addressed result cache. Skips agent invocation entirely
-- when a previous run produced a result for the same (agent_id, task)
-- pair within TTL. `key` is SHA-256 of (agent_id + canonical task JSON).
-- `created_at` is unix-epoch seconds so TTL math stays integer.
CREATE TABLE IF NOT EXISTS result_cache (
    key          TEXT PRIMARY KEY,
    value        TEXT NOT NULL,
    agent_id     TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    hit_count    INTEGER NOT NULL DEFAULT 0,
    size_bytes   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_result_cache_agent ON result_cache(agent_id);
CREATE INDEX IF NOT EXISTS idx_result_cache_created ON result_cache(created_at);

-- P2 — Rolling hit/miss log for the last hour. Trimmed on each insert
-- so the table never grows beyond ~3600 rows even under heavy load.
CREATE TABLE IF NOT EXISTS result_cache_events (
    ts       INTEGER NOT NULL,
    outcome  TEXT NOT NULL CHECK(outcome IN ('hit', 'miss'))
);
CREATE INDEX IF NOT EXISTS idx_cache_events_ts ON result_cache_events(ts);

-- Sprint P — Add subscription tier to GitHub App installations.
-- ALTER TABLE is wrapped in the idempotent migration guard in apply_schema().
ALTER TABLE github_installations ADD COLUMN tier TEXT NOT NULL DEFAULT 'free';

-- P2 — Dead-letter queue. Tasks that exhaust their retry budget land
-- here so they can be inspected, manually retried, or permanently
-- discarded. The `last_error` column carries the final attempt's
-- failure reason. The `total_attempts` column is the count actually
-- made before giving up.
CREATE TABLE IF NOT EXISTS dead_letter_queue (
    id              TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL,
    goal            TEXT NOT NULL,
    repo_path       TEXT,
    total_attempts  INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    first_failed_at TEXT NOT NULL,
    dead_at         TEXT NOT NULL,
    source          TEXT NOT NULL DEFAULT 'unknown'
);
CREATE INDEX IF NOT EXISTS idx_dlq_dead_at ON dead_letter_queue(dead_at DESC);
CREATE INDEX IF NOT EXISTS idx_dlq_task_id ON dead_letter_queue(task_id);

-- P2 — Append-only audit log. One row per actionable event across the
-- whole orchestrator: task submit/dispatch, DLQ entry, constellation
-- decisions, breaker trips, cache hit/miss. The `payload` column holds
-- JSON whose shape is per-action and intentionally schemaless.
CREATE TABLE IF NOT EXISTS audit_log (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    ts           TEXT NOT NULL,
    action       TEXT NOT NULL,
    subject_type TEXT,
    subject_id   TEXT,
    actor        TEXT,
    payload      TEXT
);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(ts DESC);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action, ts DESC);
CREATE INDEX IF NOT EXISTS idx_audit_subject ON audit_log(subject_type, subject_id, ts DESC);

-- P2 — Per-task log ring buffer. Every `AgentEvent::LogLine` is mirrored
-- here so the SSE stream has a historical tail and the web UI can
-- show progress retroactively. Capped to the most recent 1000 rows
-- per task_id via `prune_task_logs`. The `id` column is the autoincrement
-- cursor for paginated reads.
CREATE TABLE IF NOT EXISTS task_logs (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id   TEXT NOT NULL,
    ts        TEXT NOT NULL,
    level     TEXT NOT NULL,
    line      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_task_logs_task_id ON task_logs(task_id, id);

-- Sprint S — Konjo Verifier verdict ledger.
-- One row per verifier call (task_id + attempt). `passed` is 0 or 1.
-- `gaps_json` / `fix_hints_json` are JSON arrays of strings.
-- `model_used` records which Opus model graded the output.
CREATE TABLE IF NOT EXISTS verifier_verdicts (
    id           TEXT PRIMARY KEY,
    task_id      TEXT NOT NULL,
    attempt      INTEGER NOT NULL,
    passed       INTEGER NOT NULL,
    gaps_json    TEXT NOT NULL DEFAULT '[]',
    fix_hints_json TEXT NOT NULL DEFAULT '[]',
    confidence   REAL NOT NULL DEFAULT 0.0,
    model_used   TEXT NOT NULL,
    ts           TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_verifier_verdicts_task ON verifier_verdicts(task_id, attempt);

-- Sprint T — Q-learning router value table. One row per (task_type, agent
-- config) pair. The q column is the running value estimate in 0..1 and
-- update_count is how many rewards were folded in. The (state, action) pair
-- is the primary key so writes upsert the estimate in place.
CREATE TABLE IF NOT EXISTS routing_q_values (
    state        TEXT NOT NULL,
    action       TEXT NOT NULL,
    q            REAL NOT NULL DEFAULT 0.0,
    update_count INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL,
    PRIMARY KEY (state, action)
);

-- macOS-UI Phase 0 — Durable cron schedules. The static `[[schedules]]`
-- list in `lopi.toml` is loaded once at boot and cannot be edited at
-- runtime. This table backs the OpenClaw-style cron UI: schedules are
-- created/edited/enabled/deleted through `/api/schedules` and survive
-- restarts. `allowed_dirs` / `forbidden_dirs` are JSON arrays of strings.
-- `enabled` is 0 or 1. TOML entries are seeded here on first boot (matched
-- by `name`) so the UI presents a unified view.
CREATE TABLE IF NOT EXISTS schedules (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    cron           TEXT NOT NULL,
    goal           TEXT NOT NULL,
    repo           TEXT,
    priority       TEXT NOT NULL DEFAULT 'normal',
    allowed_dirs   TEXT NOT NULL DEFAULT '[]',
    forbidden_dirs TEXT NOT NULL DEFAULT '[]',
    enabled        INTEGER NOT NULL DEFAULT 1,
    autonomy_level TEXT NOT NULL DEFAULT 'draft_pr',
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_schedules_name ON schedules(name);
-- Phase 16 (Loop Engineering) — trust level governing how far a scheduled loop
-- may act without a human: report_only / draft_pr / verified_pr / auto_merge.
-- ALTER is a no-op once the column exists (handled by apply_schema).
ALTER TABLE schedules ADD COLUMN autonomy_level TEXT NOT NULL DEFAULT 'draft_pr';

-- macOS-UI Phase 0 — Per-schedule run history. One row each time a
-- schedule fires (cron tick or manual run-now). Powers the "last run" /
-- run-history view. The task_id column links to the queued task and the
-- outcome column is a short status string such as queued, duplicate, error.
CREATE TABLE IF NOT EXISTS schedule_runs (
    id          TEXT PRIMARY KEY,
    schedule_id TEXT NOT NULL,
    fired_at    TEXT NOT NULL,
    task_id     TEXT,
    outcome     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_schedule_runs_sched ON schedule_runs(schedule_id, fired_at DESC);

-- Phase 16.7 — Earned-trust ledger. One row per scope (a schedule id or repo
-- path). The level column is the auto-promoted autonomy tag (report_only,
-- draft_pr, verified_pr, or auto_merge). The clean_streak column counts
-- consecutive clean, verifier-passed runs since the last promotion or reset.
-- Trust is earned on a streak and lost on a post-merge revert (the policy lives
-- in lopi-core earned_trust).
CREATE TABLE IF NOT EXISTS trust_ledger (
    scope        TEXT PRIMARY KEY,
    level        TEXT NOT NULL DEFAULT 'draft_pr',
    clean_streak INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL
);

-- Sprint U — DAG-structured execution trace. One row per pipeline stage of a
-- task attempt. status is pending/running/done/failed. depends_on_json is a
-- JSON array of upstream stage names. output_hash memoises a done node so
-- retry can reuse it. idempotency_key records a side-effecting node's external
-- effect (the opened PR URL) so replay reuses it instead of duplicating it.
-- The edge list is derived from depends_on_json, so no separate edges table.
CREATE TABLE IF NOT EXISTS agent_dag_nodes (
    task_id         TEXT NOT NULL,
    kind            TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    depends_on_json TEXT NOT NULL DEFAULT '[]',
    output_hash     TEXT,
    idempotency_key TEXT,
    updated_at      TEXT NOT NULL,
    PRIMARY KEY (task_id, kind)
);
CREATE INDEX IF NOT EXISTS idx_agent_dag_nodes_task ON agent_dag_nodes(task_id);
