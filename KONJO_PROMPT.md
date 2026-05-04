# KONJO_PROMPT.md — lopi

> The KonjoAI working template. Use this when prompting Claude Code on this repo.

## The Konjo Way

**KONJO — Know, Outline, Nail, Justify, Optimize.**

1. **Know** — read the relevant code and docs before changing anything.
2. **Outline** — propose an approach in plain prose, list the files you'll touch.
3. **Nail** — implement, in small, reviewable diffs.
4. **Justify** — every claim ("tests pass", "no regression") backed by command output.
5. **Optimize** — only after green: tighten, simplify, measure.

## Phase plan for lopi

### Phase 1 — MVP core (this commit)
- Cargo workspace scaffolded
- `lopi-core`: real types (`Task`, `Priority`, `TaskStatus`, `Score`, `Attempt`, `AgentRun`)
- `lopi-git`: real git2-backed `GitManager` (`checkout_new_branch`, `check_diff_scope`, `hard_rollback`, `commit_all`, `open_pr` via `gh`)
- `lopi-agent`: real `AgentRunner` with the Plan→Implement→Test→Score→Retry→PR loop
- `lopi-memory`: real sqlx SQLite store (`save_task`, `save_attempt`, `load_history`, `find_similar_patterns`)
- `lopi-orchestrator`: priority `TaskQueue` with goal-dedup + `AgentPool` (Semaphore-bounded)
- `lopi-ui`: ratatui dashboard + axum JSON API + tiny static dashboard
- `lopi-remote`: teloxide bot (`/help`, `/task`, `/status`, `/approve`) + Twilio WhatsApp webhook
- `lopi-webhook`: GitHub webhook → CI-failure → high-priority fix task
- CLI: `lopi run | watch | tail | dock | sail`
- `cargo build` clean

### Phase 2 — Live concurrency + dashboards
- Wire `AgentPool` into `lopi sail`
- Broadcast `TaskStatus` over WebSocket to the web dashboard
- Live TUI rows update from broadcast channel

### Phase 3 — Remote control
- Persist Telegram authentication
- HMAC-verify GitHub webhook signature
- WhatsApp two-way: outbound status messages on Success/Failure

### Phase 4 — Self-improvement
- After every attempt, mine `attempts` table → write to `patterns`
- On new task, query `find_similar_patterns(goal)` and pre-seed constraints
- Track success-rate per (pattern × constraints) combination

## Standing rules for agents working on lopi

- **Stay inside `crates/` and `src/`.** Never touch `.github/`, `infra/`, root `Cargo.lock` deliberately.
- **`cargo build` must stay green.** If it goes red, fix it before doing anything else.
- **No `unwrap()` outside tests.** Use `anyhow::Result` and `?`.
- **No silent failure.** If a fallback path swallows an error, log it via `tracing::warn!`.
- **Async everywhere.** Tokio is the only runtime; no blocking I/O on async paths (use `spawn_blocking`).
