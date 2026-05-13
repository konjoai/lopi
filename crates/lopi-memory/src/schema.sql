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
