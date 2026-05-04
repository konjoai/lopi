# Changelog

## [0.6.0] — lopi-toon: TOON encoder/decoder + prompt token reduction

### Added
- `crates/lopi-toon` — full TOON v3.0 encoder and decoder per spec (https://toonformat.dev/)
  - `encode(value: &Value) -> String` — encodes JSON data model to TOON
  - `decode(input: &str) -> Result<Value>` — decodes TOON back to JSON with strict validation
  - `encode_task_context(goal, allowed, forbidden, constraints, patterns)` — lopi-specific helper
  - Encoder: tabular arrays (§9.3), inline primitive arrays (§9.1), expanded mixed arrays (§9.4)
  - Encoder: minimal quoting per §7.2 — reserved words, numeric-like strings, special chars
  - Encoder: canonical number format — no exponents, no trailing zeros, -0→0, NaN/Inf→null
  - Decoder: root form discovery (§5), keyed vs root array headers (§5 fix)
  - Decoder: inline arrays, tabular rows, expanded list items, nested objects
  - Decoder: `split_on_delim` respects quoted strings; strict count/width enforcement
  - 29 tests covering: all scalar types, quoting edge cases, flat/nested objects, all array forms,
    spec example round-trip, token efficiency assertion
- `lopi-agent/src/claude.rs` — TOON integrated at all three sites from token analysis:
  - **Site 1** (`plan()`, `implement()`): constraints/allowed_dirs/forbidden_dirs arrays
    encoded as TOON §9.1 inline arrays (~17 tokens/prompt saved, ~14% reduction)
  - **Site 2** (`plan()` via `runner.rs`): pattern memory injected as TOON context
    (~158 tokens/attempt saved, grows linearly with pattern count — the dominant win)
  - **Site 3** (`fix()`): error text is free-form prose — TOON intentionally skipped (no gain)
- At 100 tasks/day, estimated **-1.9M tokens/month** net reduction

### Changed
- `lopi-agent` now depends on `lopi-toon`
- `claude.rs::plan()` prompt uses `encode_task_context()` for structured context block
- `claude.rs::implement()` uses TOON scope block for allowed/forbidden dirs
- `claude.rs::fix()` uses inline TOON array for allowed_dirs (prose errors unchanged)

### Tests
- lopi-toon: 29 new tests — 0 failures
- **Total: 75 tests, 0 failures**

## [0.5.0] — Phase 4: Scheduled Tasks, Repo Profiles, lopi watch --remote

### Added
- `ScheduleEntry` type in `lopi-core` — `name`, `repo`, `goal`, `cron`, `priority`, `allowed_dirs`, `forbidden_dirs`; fully serde-compatible with `[[schedules]]` TOML arrays
- `RepoProfile` type in `lopi-core` — per-repo `.lopi.toml` profile with `allowed_dirs`, `forbidden_dirs`, `test_command`, `lint_command`, `default_constraints`, `max_retries`; `apply(&mut Task)` merges non-empty overrides
- `RepoProfile::load_from_repo(path)` — reads `<repo>/.lopi.toml`, returns `Default` if not found
- `LopiConfig::find_and_load()` — auto-discovers `./lopi.toml` then `~/.lopi/lopi.toml`
- `lopi-orchestrator::scheduler` module — `boot(entries, pool)` registers async cron jobs via `tokio-cron-scheduler`; `next_run_times(cron, n)` computes upcoming fire times
- `lopi schedules list` — prints configured schedules with next UTC run time
- `lopi watch --remote <url>` — connects to a running `lopi sail` WebSocket, injects events into local bus, drives the ratatui TUI from network events
- `lopi watch --local` — original isolated local bus behaviour
- `lopi sail` boots the cron scheduler alongside the agent pool if `[[schedules]]` are configured
- `lopi run` reads per-repo `.lopi.toml` and applies it before submitting the task
- `.lopi.toml.example` — per-repo profile template
- Updated `lopi.toml.example` with commented `[[schedules]]` examples

### Tests
- lopi-core: +6 tests (schedule_entry_deserializes, config_with_schedules, config_empty_schedules, repo_profile_default, repo_profile_apply_overrides, repo_profile_apply_skips_empty) → **20 total**
- lopi-orchestrator: +2 tests (next_run_times_valid_expr, next_run_times_invalid_expr) → **7 total**
- **Total: 46 tests, 0 failures**

## [0.4.0] — Phase 2 Full: live concurrency, ratatui TUI, full dashboard

### Added
- `AgentEvent` enum in lopi-core — rich events replacing plain `TaskStatus` broadcasts:
  `TaskQueued`, `TaskStarted`, `StatusChanged`, `LogLine`, `ScoreUpdated`, `TaskCompleted`, `TaskCancelled`, `PoolStats`
- `LogLevel` enum (`info`, `warn`, `error`, `debug`) with `AgentEvent::info/warn/error` helpers
- `AgentPool`: `DashMap<TaskId, AgentHandle>` tracking live agents with `cancel_tx: oneshot::Sender<()>`
- `AgentPool::cancel(task_id)` — graceful cancel signal to running agent
- `AgentPool::submit(task)` — enqueue + broadcast `TaskQueued` + save to DB
- `AgentPool::stats()` → `PoolStats { running, queued, succeeded, failed, uptime_secs }`
- `AgentPool::with_store()` — attach memory for pattern mining + DB persistence
- `AgentRunner` upgraded: emits `AgentEvent` at every stage (LogLine, StatusChanged, ScoreUpdated, TaskStarted); accepts `cancel_rx: oneshot::Receiver<()>` and polls cancel between stages; integrates `MemoryStore` for attempt persistence and pattern seeding
- `ClaudeCode::with_extra_constraints()` — injects memory patterns into planning prompt
- Full ratatui TUI (`lopi watch`): agent table with 7 columns, log panel (last 20 lines with level color), stats bar, help overlay, keyboard: `q/j/k/↑↓/Enter/l/Esc/?/F1`
- Full web dashboard (`index.html`): dark Konjo purple theme, live agent cards with score bar + elapsed timer + cancel button, sidebar submit form (goal/repo/priority, Ctrl+Enter), log stream, WebSocket reconnect with exponential backoff, state snapshot on connect
- `GET /api/stats` — running/queued/succeeded/failed/uptime_secs
- `DELETE /api/tasks/:id` — cancel task via HTTP (proxied to pool cancel)
- `GET /ws` — WebSocket endpoint with full state snapshot on connect, then `AgentEvent` stream; `/ws/tasks` retained for compat
- `lopi cancel <task-id>` — CLI cancel via HTTP DELETE to running sail server
- `lopi learn [--limit N]` — pretty-print mined patterns table (keywords / avg_attempts / success% / last_seen)
- `lopi dock` — pretty table output (ID / Goal / Status columns)
- `lopi run` — streams live `StatusChanged` + `LogLine` + `ScoreUpdated` events to stdout

### Changed
- `EventBus<T>` remains in lopi-core/event.rs alongside `AgentEvent` and `LogLevel`
- `lopi sail` now passes `Arc<AgentPool>` to web server; pool boots as background task
- `lopi-ui::web::serve()` signature: takes `Arc<AgentPool>` instead of raw bus
- All existing tests pass (38 total, 0 failures)

### Tests
- lopi-core: +2 tests (`agent_event_log_helpers`, `agent_event_serde_round_trip`) → 14 total
- All others unchanged: lopi-git (3), lopi-orchestrator (5), lopi-memory (11), lopi-webhook (5)
- **Total: 38 tests, 0 failures**

## [0.3.0] — Remote control + self-improvement

### Added
- `POST /api/tasks` — inject tasks into the live AgentPool queue with `goal`, `priority`, `allowed_dirs`, `max_retries`; returns `{id, goal, queued, duplicate_of}`
- `GET /api/tasks/:id` — fetch a specific task by full or prefix ID
- `GET /api/patterns` — expose mined patterns ordered by success rate
- Telegram: `/urgent <goal>` command for `Priority::High` tasks; inline keyboard (priority bump / cancel) on every queued task; `CallbackQuery` handler for button responses
- GitHub webhook: HMAC-SHA256 verification via `X-Hub-Signature-256` header; returns 401 on failure; constant-time comparison
- `MemoryStore::mine_patterns()` — extracts sorted keyword fingerprint from goal, upserts running averages into `patterns` table after each completed run
- `MemoryStore::load_patterns(limit)` — returns patterns ordered by `success_rate DESC`
- `AgentPool::with_store(store)` — attaches memory for pattern mining and `mark_completed` after each agent run
- `hmac`, `sha2`, `hex` added as workspace dependencies

### Changed
- `lopi_ui::web::serve()` now takes `TaskQueue` as third argument (task injection)
- `AppState` in `lopi-ui` now holds a `TaskQueue` handle
- `AgentPool::new()` signature unchanged; optional store via `with_store()`
- `main.rs`: `lopi sail` passes queue to both pool and web server; store attached to pool

### Tests
- lopi-memory: +4 tests (mine_patterns insert, upsert dedup, short-word skip, load ordering)
- lopi-webhook: +5 tests (valid HMAC, wrong secret, tampered body, missing prefix, empty sig)
- Total: 36 tests, 0 failures

## [0.2.0] — Live concurrency + test foundation

### Added
- `lopi-core::EventBus<T>` — thin tokio broadcast wrapper for workspace-wide event fanout
- `TaskStatus` is now `Clone + PartialEq` (derived in lopi-core)
- `AgentRunner::standalone()` — creates its own isolated bus for `lopi run`
- `AgentRunner::new()` — takes a shared `EventBus<TaskStatus>` for pool integration
- `AgentPool` now receives and propagates the shared bus to every spawned runner
- `lopi sail` boots the `AgentPool` as a background task; exposes `/ws/tasks` WebSocket endpoint
- WebSocket handler fans out serialized `TaskStatus` JSON to all connected clients; handles lag gracefully
- `lopi run` streams live status events to stdout while the agent executes
- `lopi tail --history` shows past tasks from SQLite; `--task-id` filters by prefix
- `ClaudeCode` upgraded to use `--output-format json` with `ClaudeOutput` struct and transparent fallback for older CLI versions
- `MemoryStore::open_in_memory()` for test isolation
- `MemoryStore::task_count()` helper
- 27 tests across lopi-core (12), lopi-git (3), lopi-orchestrator (5), lopi-memory (7)

### Changed
- `lopi-ui::web::serve()` now takes `EventBus<TaskStatus>` as second argument
- `lopi-orchestrator::AgentPool::new()` now takes `EventBus<TaskStatus>`
- `lopi-core` dependency added to `lopi-ui` and root binary

All notable changes to lopi.

## [0.1.0] — Initial scaffold

### Added
- Cargo workspace with 8 crates: `lopi-core`, `lopi-git`, `lopi-agent`, `lopi-memory`, `lopi-orchestrator`, `lopi-ui`, `lopi-remote`, `lopi-webhook`
- `lopi-core` types: `Task`, `TaskId`, `TaskStatus`, `Priority`, `TaskSource`, `AgentRun`, `Attempt`, `AgentState`, `Score`, `LopiConfig`
- `lopi-git`: `GitManager` (real git2 integration: branch, rollback, commit, PR via `gh`) + `DiffChecker` with allow/forbid glob enforcement
- `lopi-agent`: `AgentRunner` with the full Plan → Implement → Diff-check → Test → Score → Fix → Retry → PR loop
- `lopi-memory`: sqlx SQLite store with `tasks`, `attempts`, `patterns` tables
- `lopi-orchestrator`: priority `TaskQueue` (with goal-dedup) and `AgentPool` (Semaphore-bounded)
- `lopi-ui`: ratatui TUI dashboard + axum JSON API + minimal static dashboard
- `lopi-remote`: teloxide bot (`/help /task /status /approve`) + Twilio WhatsApp webhook
- `lopi-webhook`: GitHub webhook receiver that injects high-priority fix tasks on CI failure
- CLI binary `lopi`: `run | watch | tail | dock | sail`
- Docs: CLAUDE.md, KONJO_PROMPT.md, PLAN.md, README.md, lopi.toml.example
