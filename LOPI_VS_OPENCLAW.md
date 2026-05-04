# LOPI vs OpenClaw — Feature Comparison

> LOPI is the Rust successor to OpenClaw's Python prototype. This table maps every known OpenClaw capability to its LOPI equivalent and tracks delivery phase.

| Category | OpenClaw Feature | LOPI Equivalent | LOPI Status |
|---|---|---|---|
| **Agent Loop** | Single persistent agent loop | Multi-agent pool — N concurrent `AgentRunner` instances via `tokio::spawn` + `Semaphore` | Phase 1 |
| **Agent Loop** | Goal tracking via in-memory state | `AgentState` enum with typed status: `Planning → Implementing → Testing → Scoring → OpeningPr → RollingBack` | Phase 1 |
| **Agent Loop** | Retry on failure (manual) | Automated retry loop: score below threshold → rollback branch → re-plan with memory context → attempt N+1 | Phase 1 |
| **Agent Loop** | Sequential step execution | Async pipeline with `tokio::timeout` per phase; steps stream to TUI/WebSocket in real time | Phase 1 |
| **Agent Loop** | Single Claude invocation per run | Separate `plan` and `implement` Claude Code CLI invocations; plan prompt fed back into implement | Phase 1 |
| **Agent Loop** | Hard-coded prompt templates | Memory-augmented prompts: prior attempt patterns (success/failure/warning) injected into plan prompt each retry | Phase 4 |
| **Git Isolation** | Basic branch per run | `orka/<task_id>/<attempt_num>` branch naming — fully isolated per attempt, not just per task | Phase 1 |
| **Git Isolation** | Manual branch cleanup | Auto-delete attempt branches on rollback via `git2`; base branch never touched | Phase 1 |
| **Git Isolation** | Hard reset on failure | `git reset --hard` to base branch HEAD on any safety violation or max-retry exceeded | Phase 1 |
| **Git Isolation** | Diff inspection (manual) | Automated `DiffChecker`: off-limits glob patterns, max line cap (default 500), full path scan | Phase 1 |
| **Git Isolation** | No PR automation | Auto-open GitHub PRs via `octocrab` on accepted attempt; PR URL reported to phone + logged to DB | Phase 2 |
| **Multi-Agent** | Single agent at a time | `AgentPool` with configurable `max_agents` (default 4); `tokio::sync::Semaphore` enforces cap | Phase 2 |
| **Multi-Agent** | No agent coordination | `DashMap<Uuid, AgentState>` — lock-free shared state readable by all UI layers simultaneously | Phase 2 |
| **Multi-Agent** | No task-to-agent routing | `mpsc::channel(256)` task queue; pool picks up tasks as agents free up; priority queue planned | Phase 2 |
| **Multi-Agent** | No agent roles | Backlog: specialised agent roles (planner, implementer, reviewer) with different prompt personas | Backlog |
| **Multi-Agent** | No dependency graph | Backlog: task A blocks task B — DAG-style task dependencies with topological scheduling | Backlog |
| **Memory** | File-based JSON attempt logs | SQLite via `sqlx` with WAL mode — `attempts` + `patterns` tables; survives restarts | Phase 1 |
| **Memory** | No cross-run pattern learning | `patterns` table: success/failure/warning patterns extracted after each attempt, weighted by outcome | Phase 4 |
| **Memory** | No prompt augmentation from history | Memory context formatted and injected into every re-plan: `[✓] Score 0.91 achieved with approach X` | Phase 4 |
| **Memory** | No path-scoped history | `get_patterns(target_path)` — patterns are scoped to file/dir path, so `src/auth/` learns independently of `src/db/` | Phase 4 |
| **Memory** | No audit trail | Full attempt history in DB: diff, plan, test output, lint output, score, branch, timestamps — queryable via `lopi history` | Phase 1 |
| **UI** | CLI stdout only | Dual UI: ratatui TUI dashboard (agents panel, task queue, log tail) AND axum web dashboard | Phase 2 |
| **UI** | No live agent state view | TUI re-renders at 10Hz from `DashMap` — shows agent ID, status, branch, attempt, score per agent | Phase 2 |
| **UI** | No web interface | Axum web server on port 7070; HTML/JS dashboard embedded via `include_str!`; no separate deploy needed | Phase 2 |
| **UI** | No live log streaming | WebSocket broadcast (`tokio::sync::broadcast`) pushes state deltas to all connected browser tabs | Phase 2 |
| **UI** | No REST API | REST endpoints: `GET /api/tasks`, `POST /api/tasks`, `GET /api/agents`, `GET /api/runs`, `GET /api/logs/:id` | Phase 2 |
| **Phone Control** | None | Telegram bot via `teloxide`: `/task`, `/status`, `/runs`, `/cancel`, `/approve` commands | Phase 3 |
| **Phone Control** | None | WhatsApp via Twilio webhook — same command parser, same orchestrator injection | Phase 3 |
| **Phone Control** | None | Auth allowlist: Telegram chat IDs + phone numbers — unauthorised callers rejected | Phase 3 |
| **Phone Control** | None | Phase-by-phase push notifications: agent starts, tests running, PR opened, failure alert | Phase 3 |
| **Phone Control** | None | Inline Telegram keyboard buttons for PR approve/reject without leaving the app | Phase 3 |
| **Event Triggers** | Manual task injection only | `lopi task --title "..." --path src/` CLI for manual injection | Phase 1 |
| **Event Triggers** | None | GitHub webhook listener (port 7071) with HMAC-SHA256 signature verification | Phase 3 |
| **Event Triggers** | None | CI failure hook: `workflow_run` event with `conclusion: failure` → auto-inject fix task at `TaskPriority::High` | Phase 3 |
| **Event Triggers** | None | GitHub Actions workflow template to forward CI failures to LOPI webhook | Phase 3 |
| **Event Triggers** | None | Backlog: scheduled recurring tasks (cron-style nightly lint sweep, dependency audit) | Backlog |
| **Self-Modification** | Basic code editing via Claude | Full `Plan → Implement → Test → Score → Retry → PR` loop with every safety gate active | Phase 1 |
| **Self-Modification** | No file-level off-limits list | `off_limits_files` and `off_limits_dirs` glob patterns in config; diff checker blocks violations immediately | Phase 1 |
| **Self-Modification** | No self-protection | `allow_self_modify: false` — LOPI's own `src/` and `crates/` are off-limits by default | Phase 1 |
| **Self-Modification** | Manual guardrails | Automatic hard rollback on any safety violation; no retry allowed after `TouchesOffLimits` | Phase 1 |
| **Self-Modification** | No diff size cap | `max_diff_lines: 500` — configurable; prevents runaway rewrites; scored penalty for large diffs | Phase 1 |
| **Model Routing** | Claude Code only, fixed | Claude Code CLI only (Phase 1–3); API-direct routing planned for Phase 4 | Phase 1 |
| **Model Routing** | No cheap/expensive split | Backlog: route planning step to Haiku (cheap/fast), implementation to Sonnet, review to Opus | Backlog |
| **Model Routing** | No per-task model config | Backlog: `Task.model_hint` field — caller can specify preferred model tier | Backlog |
| **CI Integration** | Basic test runner invocation | `run_tests()` runs `cargo test` (or configured test command) via `tokio::process`; output captured + scored | Phase 1 |
| **CI Integration** | No lint integration | `run_lint()` runs `cargo clippy` or configured linter; clean lint = 0.25 score weight | Phase 1 |
| **CI Integration** | No test output parsing | `parse_test_pass_rate()` extracts passed/failed counts from `cargo test`, nextest, and pytest output | Phase 1 |
| **CI Integration** | No CI webhook inbound | GitHub `workflow_run` webhook auto-injects fix tasks when CI goes red | Phase 3 |
| **CI Integration** | No PR-gated merging | PRs opened by LOPI; human must review and merge — LOPI never merges autonomously | Phase 2 |
| **Observability** | Print statements | `tracing` crate with structured spans; `RUST_LOG` env var controls verbosity | Phase 1 |
| **Observability** | No log streaming | TUI log tail panel (last 50 lines per agent) + WebSocket log stream to browser | Phase 2 |
| **Observability** | No structured output | JSON log mode via `tracing-subscriber` with `format::json()` — machine-readable for log aggregators | Phase 2 |
| **Observability** | No metrics | Backlog: Prometheus `/metrics` endpoint — agent throughput, score distribution, retry rate, queue depth | Backlog |
| **Observability** | No trace IDs | `task_id` + `agent_id` + `attempt_id` on every log line via `tracing::Span` — full request tracing | Phase 1 |
| **Scoring** | Binary pass/fail | Weighted composite score: test pass rate (50%) + lint clean (25%) + diff size score (25%) | Phase 1 |
| **Scoring** | No numeric threshold | `score_threshold: f64` in config (default 0.75) — attempts below threshold trigger retry | Phase 1 |
| **Scoring** | No retry-to-score loop | Score drives the retry decision: accept (≥ threshold), retry (< threshold, attempts remaining), rollback | Phase 1 |
| **Scoring** | No score history | Every attempt's `ScoreBreakdown` persisted to SQLite — trend analysis over time available via `lopi history` | Phase 1 |
| **Scoring** | No diff size penalty | `diff_size_score = max(0, 1 - lines_changed / 500)` — bloated diffs score lower | Phase 1 |
| **PR Workflow** | No automated PR creation | `octocrab` opens a PR from the attempt branch to `base_branch` on accepted score | Phase 2 |
| **PR Workflow** | No PR metadata | PR body includes: task description, attempt number, score breakdown, diff stats, link to LOPI run | Phase 2 |
| **PR Workflow** | No phone approval | Telegram `/approve <run_id>` command; bot posts PR URL as inline button for one-tap open | Phase 3 |
| **PR Workflow** | No PR gating | LOPI never auto-merges; human approval is the final gate regardless of score | Phase 2 |
| **PR Workflow** | No PR URL persistence | PR URL stored in `attempts.outcome_data` JSON — queryable, shareable, linkable | Phase 2 |
| **Configuration** | Env vars only | Dual config: `lopi.toml` file + env var overrides (prefixed `LOPI_`) via the `config` crate | Phase 1 |
| **Configuration** | No hot reload | `arc_swap::ArcSwap<LopiConfig>` — config pointer swapped atomically; agents pick up changes mid-run | Phase 4 |
| **Configuration** | No per-agent config | `AgentConfig.max_agents`, `max_retries`, `timeout_secs`, `score_threshold` all runtime-configurable | Phase 1 |
| **Configuration** | No safety config section | Dedicated `[safety]` TOML section: off-limits globs, max diff lines, test requirement, self-modify toggle | Phase 1 |
| **Configuration** | No multi-remote support | Single remote gateway abstracted behind `RemoteGateway` trait; Telegram + WhatsApp are two impls | Phase 3 |

---

## Summary

| Dimension | OpenClaw | LOPI |
|---|---|---|
| Language | Python | Rust (tokio async) |
| Concurrency | Single agent | N parallel agents (configurable) |
| Memory | File-based JSON | SQLite with pattern learning |
| UI | CLI stdout | TUI (ratatui) + Web (axum + WebSocket) |
| Remote control | None | Telegram bot + WhatsApp (Twilio) |
| Event triggers | Manual only | GitHub webhooks, CI hooks, phone, CLI |
| Scoring | Binary | Weighted composite (tests + lint + diff size) |
| Safety | Manual | Automated diff checker, off-limits globs, auto-rollback |
| PR workflow | None | Auto-open via octocrab, phone approval |
| Model routing | Fixed | Fixed now; multi-model routing in backlog |
| Observability | Print statements | Structured tracing, WebSocket log stream, metrics (backlog) |
| Config | Env vars | TOML + env + hot reload (Phase 4) |
| Distribution | Python runtime required | Single statically-linked binary |

*LOPI is not a rewrite. It's a crossing to the other shore.*
