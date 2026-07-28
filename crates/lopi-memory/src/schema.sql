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
-- Sprint E, Part 2 — CostEstimator's grouping key needs per-turn stage/
-- effort, not just model. `stage` defaults to 'implement' (the dominant,
-- always-present stage) so pre-Sprint-E rows and any write path that
-- doesn't thread a real stage through yet still land in a sane bucket
-- instead of a NULL that would silently drop out of every GROUP BY.
ALTER TABLE turn_metrics ADD COLUMN stage TEXT NOT NULL DEFAULT 'implement';
ALTER TABLE turn_metrics ADD COLUMN effort TEXT;
CREATE INDEX IF NOT EXISTS idx_turn_metrics_estimate
    ON turn_metrics(stage, model, effort);

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

-- MCPB-App-1 (KT-B1): the running attempt's git branch, the one field
-- `TaskStatus::Success{branch}`/a freeform log line/an in-memory
-- `AgentEvent::TaskStarted` could never answer durably and structurally for
-- an in-flight task. Written by `AgentRunner::persist_branch` the moment
-- `TaskStarted` fires — see LEDGER.md's MCPB-App-1 entry.
ALTER TABLE tasks ADD COLUMN branch TEXT;

-- macOS-Web-Parity-5: the effective repo (task-level override, or the pool
-- default when the task set none) an attempt actually runs against — never
-- persisted before, so every "goal/repo" column on both web and macOS Overview
-- read a permanent "—" placeholder. Same shape and same write-site timing as
-- `branch` above: unresolved until dequeue (pool default vs. task override),
-- so it's written by `AgentRunner::persist_repo` the moment `TaskStarted`
-- fires, not at initial `save_task`.
ALTER TABLE tasks ADD COLUMN repo TEXT;

-- Sprint F4 Phase 4: the CLI's own resumable session id for the task's most
-- recent attempt, so `lopi diag`, replay, and the CLI's own transcripts
-- (`transcript_import.rs`) share a join key. Written by
-- `AgentRunner::persist_cli_session` the moment the id is minted, before the
-- plan phase's first spawn (lopi chooses the id itself — see `run_loop.rs` —
-- rather than waiting for the CLI to echo one back).
ALTER TABLE tasks ADD COLUMN cli_session_id TEXT;

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

-- A2 (reflection): durable, rollback-safe learnings distilled from a failed or
-- rolled-back attempt. Unlike lessons there is NO score gate, because a rejected
-- attempt's lesson is exactly the low-score case that must survive (you learned
-- what does NOT work). goal_keywords holds keyword_fingerprint(goal) for
-- relevance-filtered retrieval (Jaccard vs a new task's goal). critique is why it
-- failed (the evaluator's flattened gaps/fix-hints), attempted a short summary of
-- the approach, outcome the reject reason. Retrieval dedups on (repo_path,
-- critique) and caps injection: bounded and relevant is the whole point, since
-- unbounded or irrelevant context is the failure mode the A2 section 2 test
-- punishes.
CREATE TABLE IF NOT EXISTS learnings (
    id            TEXT PRIMARY KEY,
    repo_path     TEXT NOT NULL,
    goal_keywords TEXT NOT NULL DEFAULT '',
    critique      TEXT NOT NULL,
    attempted     TEXT NOT NULL DEFAULT '',
    outcome       TEXT NOT NULL DEFAULT '',
    task_id       TEXT,
    created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_learnings_repo_created ON learnings(repo_path, created_at DESC);

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

-- Sprint S12, Phase 0: the multi-tenant GitHub App installation ledger is
-- removed as part of the scope lock (lopi is single-operator, single-machine).
-- This statement actively drops the table (and its indexes, dropped
-- implicitly with it) on every existing database the next time it is
-- opened — see LEDGER.md for the retain-vs-drop decision. Safe to delete
-- this line entirely once no database at or before v0.31.0 is expected to
-- exist anymore.
DROP TABLE IF EXISTS github_installations;

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

-- P2 — Append-only audit log. One row per actionable event across the
-- whole orchestrator: task submit/dispatch, breaker trips,
-- cache hit/miss. The `payload` column holds
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

-- Eval-Execution-1 (A1) — the tiered eval executor's outcome ledger + score
-- history (cross-cutting seam #4). One row per (task_id + attempt) eval run.
-- `verdict` is `pass`/`fail`/`error` (fail-closed: `error` is not-passing).
-- `score` is the weighted scalar in 0..1 A3's ratchet reads. `per_check_json`
-- and `critique_json` are JSON: the per-tier results and the flattened
-- gaps+fix_hints A2's reflection reads. The score trajectory over attempts is
-- the single source of truth for "is this loop improving" (A3 no-progress, B1
-- stack termination).
CREATE TABLE IF NOT EXISTS eval_outcomes (
    id             TEXT PRIMARY KEY,
    task_id        TEXT NOT NULL,
    attempt        INTEGER NOT NULL,
    verdict        TEXT NOT NULL,
    score          REAL NOT NULL DEFAULT 0.0,
    per_check_json TEXT NOT NULL DEFAULT '[]',
    critique_json  TEXT NOT NULL DEFAULT '[]',
    ts             TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_eval_outcomes_task ON eval_outcomes(task_id, attempt);

-- Verification gate (Finding #1) — the dead-letter ledger. A task that
-- exhausted its retry budget without meeting its goal (StopReason::
-- MaxIterations / NoProgress / Budget, never GoalMet — see
-- lopi_core::StopReason::parse_from_failure_reason) gets a durable, queryable
-- row here instead of silently becoming an unremarkable TaskStatus::Failed.
-- One row per exhausted task. `detail` is the full failure-reason string the
-- row was parsed from, kept verbatim for operator follow-up.
CREATE TABLE IF NOT EXISTS dead_letters (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL,
    goal        TEXT NOT NULL,
    stop_reason TEXT NOT NULL,
    attempts    INTEGER NOT NULL,
    detail      TEXT NOT NULL,
    ts          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_dead_letters_task ON dead_letters(task_id);
CREATE INDEX IF NOT EXISTS idx_dead_letters_ts ON dead_letters(ts);

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

-- MAXX Phase 0 — Quota headroom tracking. One row per Anthropic account rate
-- limit window (`five_hour` / `seven_day`), upserted every time an agent
-- observes a `rate_limit_event`. limit_type is the primary key so a
-- `five_hour` observation can never clobber a `seven_day` one (they arrive
-- through the same AgentEvent::ApiRetry variant, so this is an easy bug to
-- introduce silently). resets_at is unix seconds, nullable — the CLI does
-- not always report it.
CREATE TABLE IF NOT EXISTS quota_observations (
    limit_type   TEXT PRIMARY KEY,
    status       TEXT NOT NULL,
    utilization  REAL NOT NULL,
    resets_at    INTEGER,
    observed_at  TEXT NOT NULL
);

-- MAXX Phase 1 — Opportunistic backlog dispatch entries. Mirrors `schedules`
-- (same CRUD conventions, `/api/maxx` instead of `/api/schedules`) minus
-- `cron`, plus quiet_hours/headroom_gate/windows_json — a MAXX entry fires on
-- "favorable" conditions (quiet hours or comfortable quota headroom) rather
-- than a fixed cadence. quiet_hours_start/end are 0-23 local hours, both NULL
-- when quiet-hours gating is off. windows_json is a JSON array of the limit
-- types (`five_hour`/`seven_day`) headroom_gate checks.
CREATE TABLE IF NOT EXISTS maxx_entries (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    goal              TEXT NOT NULL,
    repo              TEXT,
    priority          TEXT NOT NULL DEFAULT 'normal',
    allowed_dirs      TEXT NOT NULL DEFAULT '[]',
    forbidden_dirs    TEXT NOT NULL DEFAULT '[]',
    enabled           INTEGER NOT NULL DEFAULT 1,
    autonomy_level    TEXT NOT NULL DEFAULT 'draft_pr',
    report            TEXT,
    quiet_hours_start INTEGER,
    quiet_hours_end   INTEGER,
    headroom_gate     INTEGER NOT NULL DEFAULT 0,
    windows_json      TEXT NOT NULL DEFAULT '[]',
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_maxx_entries_name ON maxx_entries(name);

-- MAXX Phase 1 — Per-entry fire history, mirrors schedule_runs.
CREATE TABLE IF NOT EXISTS maxx_runs (
    id       TEXT PRIMARY KEY,
    maxx_id  TEXT NOT NULL,
    fired_at TEXT NOT NULL,
    task_id  TEXT,
    outcome  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_maxx_runs_entry ON maxx_runs(maxx_id, fired_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_dag_nodes_task ON agent_dag_nodes(task_id);

-- Stack-Chain-1 — server-side whole-stack cron scheduling. Distinct from
-- `schedules` (one row = one goal) because a stack is an ORDERED SEQUENCE of
-- independent goals: `schedule_chains` is the cron header, one row per stack
-- card lives in `schedule_chain_steps`, and `schedule_chain_runs` tracks which
-- step a given fire is currently on so a backend restart mid-chain resumes
-- the in-flight step instead of restarting from step 1 or dropping the rest
-- of the chain (see `ChainScheduleManager`).
CREATE TABLE IF NOT EXISTS schedule_chains (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    cron           TEXT NOT NULL,
    repo           TEXT,
    priority       TEXT NOT NULL DEFAULT 'normal',
    autonomy_level TEXT NOT NULL DEFAULT 'draft_pr',
    -- Mirrors the client-side `OnFail` policy (`web/src/lib/stores/stack.ts`):
    -- stop | continue | backoff.
    on_fail        TEXT NOT NULL DEFAULT 'stop',
    enabled        INTEGER NOT NULL DEFAULT 1,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_schedule_chains_name ON schedule_chains(name);

-- One row per stack card, ordered by `step_order` (0-based).
CREATE TABLE IF NOT EXISTS schedule_chain_steps (
    chain_id       TEXT NOT NULL,
    step_order     INTEGER NOT NULL,
    goal           TEXT NOT NULL,
    allowed_dirs   TEXT NOT NULL DEFAULT '[]',
    forbidden_dirs TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (chain_id, step_order)
);

-- One row per chain-fire attempt (cron tick or manual run-now). `status` is
-- running | completed | failed. `current_step`/`current_task_id` are updated
-- as each step is submitted so `ChainScheduleManager::resume_orphaned` can
-- tell, on boot, exactly which step was in flight when the process died.
CREATE TABLE IF NOT EXISTS schedule_chain_runs (
    id              TEXT PRIMARY KEY,
    chain_id        TEXT NOT NULL,
    fired_at        TEXT NOT NULL,
    current_step    INTEGER NOT NULL DEFAULT 0,
    current_task_id TEXT,
    status          TEXT NOT NULL DEFAULT 'running',
    updated_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_schedule_chain_runs_chain ON schedule_chain_runs(chain_id, fired_at DESC);
CREATE INDEX IF NOT EXISTS idx_schedule_chain_runs_status ON schedule_chain_runs(status);

-- One row per (pattern, keyword) token. idx_patterns_keywords (above) indexes
-- the whole goal_keywords string, which can only accelerate an exact-string
-- match — useless for find_similar_patterns' per-token overlap query, which
-- is why it went unused and that query fell back to scanning every pattern
-- row. This table is the join target: querying by a single keyword hits
-- idx_pattern_keywords_keyword directly instead.
CREATE TABLE IF NOT EXISTS pattern_keywords (
    pattern_id TEXT NOT NULL,
    keyword    TEXT NOT NULL,
    PRIMARY KEY (pattern_id, keyword)
);
CREATE INDEX IF NOT EXISTS idx_pattern_keywords_keyword ON pattern_keywords(keyword);

-- Sprint Successor-1 — lineage columns. `parent_task` links a derived
-- successor task back to the task it came from (NULL for anything not
-- derived by `derive_successor_task`). `chain_depth` is how many successor
-- hops separate it from the root of its chain (0 for anything not derived).
-- Both are no-ops for every task written before this sprint, since
-- ALTER TABLE is idempotent via apply_schema()'s duplicate-column guard.
ALTER TABLE tasks ADD COLUMN parent_task TEXT;
ALTER TABLE tasks ADD COLUMN chain_depth INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_tasks_parent_task ON tasks(parent_task);

-- Onboarding-Import-1 (Phase 0) — toolchain-scoped pattern backfill.
--
-- `toolchain` is a coarse per-project ecosystem label (e.g. "rust", "node",
-- "python") derived by walking a historical session's project directory for
-- manifest files (Cargo.toml, package.json, ...) — see `src/toolchain_detect.rs`.
-- NULL for every pre-existing row and for any live `mine_patterns` row, since
-- no toolchain detection runs on the live-task path (only the onboarding
-- backfill populates it today).
--
-- Deliberately named `toolchain`, not `stack` — `web/src/lib/stores/stack.ts`
-- and the loop-stack/card concept already own that word in this codebase.
-- One-way-door naming decision — see LEDGER.md's Onboarding-Import-1 entry
-- (KT-C) for the confirmation record.
ALTER TABLE patterns ADD COLUMN toolchain TEXT;

-- `source` distinguishes patterns mined from live lopi task runs
-- ('lopi_run' — the default, applied retroactively to every pre-existing
-- row) from patterns backfilled once from historical Claude Code session
-- transcripts ('onboarding_import').
ALTER TABLE patterns ADD COLUMN source TEXT NOT NULL DEFAULT 'lopi_run';

-- Onboarding-Import-1 (Phase 5) — per-session idempotency ledger. One row
-- per historical transcript session already folded into `patterns`, keyed
-- on the JSONL's own sessionId. Onboarding may re-trigger (reinstall or a
-- new machine) — this lets a re-run skip sessions already imported instead
-- of re-blending their stats into existing pattern rows a second time.
CREATE TABLE IF NOT EXISTS onboarding_imports (
    session_id  TEXT PRIMARY KEY,
    project_dir TEXT NOT NULL,
    pattern_id  TEXT,
    imported_at TEXT NOT NULL
);

-- Constraint-Capture-2: how many completed tasks have contributed to this
-- pattern's rolling averages. `mine_patterns` had recorded avg_attempts/
-- success_rate on every completed task but never a `successful_constraints`
-- value, so `seed_from_patterns` was silently reading nothing back for the
-- auto-mined (non-postmortem) path. Now that a constraint is written on a
-- clean success, a single one-off task would otherwise become an
-- equally-weighted "template" the moment it completes. occurrence_count is
-- the promotion-gate signal (`seed_from_patterns`) that keeps a pattern's
-- constraint out of the planning prompt until it has actually recurred.
-- DEFAULT 1 so every pre-existing row (mined before this column existed,
-- necessarily seen at least once) reads as one real occurrence, not zero.
-- No semicolons anywhere in this comment block: apply_schema() splits
-- SCHEMA on the literal semicolon character without comment-aware parsing,
-- so one inside a `--` line would silently fracture this ALTER TABLE
-- statement in two.
ALTER TABLE patterns ADD COLUMN occurrence_count INTEGER NOT NULL DEFAULT 1;

-- Demo-1: generic store-level key/value metadata. Not demo-specific in
-- spirit — any future feature needing a single durable flag on the store
-- itself (rather than a per-row column) can use this table — but today
-- only `lopi demo`'s fixture generator writes to it, setting the
-- `"synthetic"` key so every rendering surface can ask
-- `MemoryStore::is_synthetic` before presenting demo data as real.
CREATE TABLE IF NOT EXISTS store_metadata (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Demo-1: synthetic repo descriptors written by `lopi demo`'s fixture
-- generator. Real repos are discovered by filesystem scan in `lopi-ui`
-- (there is no `repos` table for them) — demo mode must never touch the
-- real filesystem, so its "repo list" is this table instead. Empty on
-- every real store. `sort_order` is caller-assigned so
-- `load_demo_repos` can return rows in a stable, deterministic order
-- (`ORDER BY sort_order`) regardless of SQLite's default row order.
CREATE TABLE IF NOT EXISTS demo_repos (
    name        TEXT PRIMARY KEY,
    stack       TEXT NOT NULL,
    path        TEXT NOT NULL,
    description TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0
);
