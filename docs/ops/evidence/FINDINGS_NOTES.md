# Ops-2 working notes (raw findings, pre-report)

## Pre-flight
- Local main was 137 commits behind origin/main (PR#41 vs PR#76). Fast-forwarded to `4e9b390` (Unify-2, PR#76). Ops-1's RUNNING.md/nav.ts only exist post-sync.
- Tool versions: rustc/cargo 1.89.0, node v20.19.4, npm 9.5.0, Xcode 26.6, xcodegen 2.45.4, ffmpeg 8.0.1, claude 2.1.153. No ANTHROPIC_API_KEY (subscription auth).
- Backend routes: **49 `.route()` regs** (~55 method+path combos), not 52.
- Web nav (`nav.ts` NAV_ITEMS): **4 items** — /stacks, /schedules, /overview, /config. Unify-2 (PR#76) cut Fleet/Pulse/Constellation/Logs/Tools/Debug/Router/Tasks/Budget from nav.
- Orphan routes (exist, not in nav): /budget, /loop, /onboard(hidden).
- macOS NavSection (RootView.swift): **13** — Forge, Dashboard, Budget, Tasks, Cron, Loop, Constellations, Dead-Letter, Tools, Health, Patterns, Audit, Config.

## Phase 1 — builds + tests
- cargo build --workspace: **clean** (52s). web npm build: **clean**. macOS xcodebuild: **BUILD SUCCEEDED** → resolves Ops-1 Known Issue #1 (was Linux-only limitation).
- cargo test --workspace: **1107 passed, 0 failed, 1 ignored** (47 test binaries). Fully green.
- cargo-nextest NOT installed (CLAUDE.md lists it as preferred; `cargo nextest` errors "no such command").

## Phase 2 — backend sweep (evidence: evidence/api/sweep.txt)
- All GET routes 200 + sane JSON. Mutating schedule/tool/cache/loop routes wired & validated.
- **FINDING (Broken/dead): 4 constellation API calls in api.ts hit non-existent backend routes.**
  - GET /api/constellations → 200 but returns SPA index.html (static fallback), not JSON → listConstellations() gets HTML, would fail JSON parse.
  - POST /api/constellation/:name/dispatch → 405.
  - GET /api/constellation/:name/stats → 200 HTML fallback.
  - No `/api/constellation*` route exists in web/mod.rs. Frontend constellation feature is dead against this backend.
- **FINDING (validation gap): POST /api/tasks {"goal":""} → 201 Created + spawned real agent.** Empty goal accepted, enqueued, task_started on /sse. Contradicts security.md input-validation rule. (Cost stayed $0 — empty goal ran as no-op. Task cancelled/deleted.)
- **FINDING (config ignored): `sail` ignores `db_path` from --config.** sail_commands.rs:18 uses util::db_path() unconditionally; scratch db stayed 0 bytes, real ~/.lopi/lopi.db used. cfg only feeds `schedules`. /api/config also returned {"config":null,"source":"none"}.
- Minor: POST /api/agents/:id/heartbeat with bogus id → 200 (creates heartbeat for non-existent agent; likely by-design self-register).
- Minor: GET /api/tasks/:id/stream bogus id → 200 with `{"error":"run not found"}` body (200 + error body mismatch).
- Bogus-id GETs mostly 404 (task/dlq/run/health/rate-limit) = properly wired. dag & logs return 200 empty (permissive).
- /ws & /ws/tasks → 400 without upgrade header (correct). /metrics 200 prometheus. /sse 200 streams events.

## Phase 3 — web sweep (evidence: evidence/web/sweep.txt + .json + screenshots)
- All 7 routes (4 nav + 3 orphan): HTTP 200, **console clean** (zero errors/warnings on every route).
- Nav links all Wired(link) → real destinations. App-shell controls (Toggle nav, ?, PRESS ? FOR SHORTCUTS) Client-only (correct).
- **FINDING (Broken): /overview status buckets wrong.** Badges ALL=20, RUNNING=20, QUEUED=0, DONE=0, DEAD-LETTER=0. Real DB: {success:9, failed:6, cancelled:3, queued:2, running:0}. Every task mis-bucketed as RUNNING; QUEUED should be 2, DONE should be 9. Overview is 1 of 4 nav items (replaced Fleet+Dashboard+Pulse) → core rollup broken.
- Verified NOT broken (empirical caught false-positive): /onboard "Forge dashboard"→/ renders empty in domcontentloaded snapshot, but / client-hydrates to /stacks (final url /stacks, body renders). Doc note: RUNNING.md says "GET / redirects (307)" but server returns 200 (client-side redirect), minor doc inaccuracy.
- /stacks: composer inputs (goal fields) not-clicked (would spawn); "Resize columns" Stubbed (no-op observed); "+" add-card Client-only.
- /config: theme buttons EMBER/JADE Client-only (switch theme); ICE Stubbed = active-default no-op (false-positive); tree/raw view toggles similar.
- /budget (orphan): $1/$5/$10/$25/$50 cap presets Client-only; cap input Stubbed on fill; "STOP ALL RUNNING (20)" not-clicked (destructive) — note the "(20)" mirrors the overview miscount.
- /loop (orphan): well-wired — run rows expand via GET /api/loop-engineering/runs/:id; escalation switch Wired (POST escalation); strategy cards S2/S3/S4 Client-only select, S1 active no-op.
- **Data-quality note:** task `status` strings in DB are malformed e.g. "failed ❌ Cancelled" (emoji+dup text) — likely root cause of overview mis-bucketing.

## macOS status per RUNNING.md
- macOS-only screens: Dashboard, Cron, Dead-Letter, Health, Patterns, Audit + menu-bar. README: "Phase 1–2 + Cron"; several admin panels stubbed & wired into nav.
- Web-only (not in macOS): Stacks, Overview, Loop, Budget, Schedules-as-UI, Onboard.
