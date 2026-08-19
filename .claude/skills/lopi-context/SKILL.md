---
name: lopi-context
description: Full lopi project context — complete phase plan, current health metrics, standing rules for agents, crate details. Auto-load when working on lopi sprint planning, architecture decisions, or Phase 5+ work.
user-invocable: false
---
# lopi — Full Project Context

## Phase Plan

Phases 1-9 below are the original v0 plan and are all long shipped or superseded.
The live numbering is the **P-series** (review-pipeline sprints). `CHANGELOG.md` and
`LEDGER.md` are authoritative for what shipped and why; `NEXT_SESSION_PROMPT.md`
(newest first) is authoritative for what is next. Do not plan from this table.

| Phase | Status | What Shipped |
|-------|--------|--------------|
| 1 — MVP Core | ✅ v0.1.0 | Cargo workspace, CLI verbs |
| 2 — N Parallel Agents + Live Dashboard | ✅ v0.4.0 | AgentPool, EventBus, ratatui TUI, web dashboard, WebSocket |
| 3 — Remote Control + Pattern Mining | ✅ v0.3.0 | GitHub webhook HMAC, pattern miner (the Telegram transport shipped here and was removed again in S10 Phase 4) |
| 4 — Scheduled Tasks + Repo Profiles | ✅ v0.5.0 | cron scheduler, RepoProfile, `lopi watch --remote` |
| 5 — Self-Improvement Engine | ✅ v0.10.0 / v0.11.0 | `lopi learn`, failure post-mortem, pattern learning |
| 6 — GitHub Webhooks + CI Integration | ◐ partial | `lopi-webhook` end-to-end; GitHub App mode and full-event HMAC still open |
| 7 — Production Web UI | ✅ | SvelteKit dashboard under `web/` |
| 8 — Native Mobile App | ◐ | macOS/iOS app under `macos/`; React Native never started |
| 9 — Intelligence + Evolution | ◐ ongoing | Sprints H-S, then the P-series |

Sprints H through S (v0.10.0-v0.19.0) and the P-series (P0-P4, v0.40.0-v0.45.0) all
postdate this table. See `PLAN.md` for the alphabetic sprint log (itself current only
to v0.19.0) and `docs/LOOP_ENGINEERING_ROADMAP.md` for the freshest verified roadmap.

## Current Health

Re-measure rather than trusting these; they drift. Verified 2026-08-19 at `0.46.0`.

- Crates: 19 under `crates/`, plus the root binary at `src/` and a SvelteKit front end at `web/`
- Tests: ~2196 `#[test]`/`#[tokio::test]` sites across `crates/` + `src/`
- Coverage: 68.34% (`.konjo/coverage-floor.txt`), against a soft 80% gate
- Ratchets at zero margin: function-length 74, indexing floor 211
- CLI: 31 subcommands (see `src/cli.rs`) — `run`, `sail`, `watch`, `learn`, `spec`, `index`, `mcp-serve` and more
- Gate tiers: only `G1 · static` and two `G2 · coverage` steps are BLOCKING; everything
  else is ADVISORY (`LEDGER.md`'s `Gate-Tiering-1`)

### Known unreachable code — real, documented, not hidden

- The direct-Anthropic-API tier (`with_api()`) has no production call site; the CLI
  fallbacks (`verifier_cli.rs`, `postmortem_cli.rs`) cover for it. `--adaptive-retry`
  is inert on the shipped binary.
- `lopi-remote` is unreachable (README says so outright).
- `lopi replay` reports a plan; live re-execution is not wired.

## Standing Rules for Agents on lopi
- Stay inside `crates/` and `src/` — never touch `.github/`, `infra/`, root `Cargo.lock` deliberately
- `cargo build` must stay green — if it goes red, fix before doing anything else
- No `unwrap()` outside tests — use `anyhow::Result` and `?`
- No silent failure — if a fallback path swallows an error, log via `tracing::warn!`
- Async everywhere — Tokio is the only runtime; no blocking I/O on async paths (use `spawn_blocking`)

## Key Architecture Decisions (Load-Bearing)
- `EventBus<TaskStatus>` uses `tokio::broadcast` — subscriber lag drops old events; this is intentional
- `AgentPool` uses `Arc<Semaphore>` for bounded concurrency — no global mutex on the task queue
- `lopi-memory` uses `sqlx` with SQLite + WAL mode — concurrent readers are fine, one writer at a time
- Pattern mining: keyword fingerprint extraction via `find_similar_patterns(goal)` — not embedding-based yet (Phase 9)
- GitHub webhook HMAC: constant-time comparison via `subtle::ConstantTimeEq` — do not simplify to `==`
