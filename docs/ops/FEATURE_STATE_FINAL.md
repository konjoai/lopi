# FEATURE_STATE_FINAL — Verify-1 live audit master table

**Baseline:** `main` @ `a6e4b5f` (Polish-1 / PR #79), v0.3.2 · **Date:** 2026-07-10
**Method:** live, on-device, real Claude subscription auth (`ANTHROPIC_API_KEY` unset), real billed agent runs on scratch repos. No `?demo=1`, no CI sandbox.
**Supersedes:** `docs/ops/FEATURE_STATE.md` (Ops-2).

**Environmental limitation (applies to every macOS-*visual* row):** the MacBook sat at the **lock screen** for this entire unattended run. GUI apps cannot composite to a locked display, computer-use is blocked on the lock screen, and unlocking (credentials) is out of bounds. macOS rows are therefore classified from **build + test + endpoint-parity + source**, not from an observed render. The web surface renders headlessly via Playwright and is fully observed.

Status legend: **Wired** (real network/DB effect observed) · **Client-only** (renders/computes but no server call) · **Stubbed** (present, no real effect) · **Broken** (errors or wrong result) · **Unverified** (blocked this run, reason given).

---

## A. Concurrency (the centerpiece)

| Surface | Platform | Element | Type | Status | Evidence | Notes |
|---|---|---|---|---|---|---|
| Two single agents, simultaneous | Web+API | Launch 2 tasks at once, distinct repos | Wired | **PASS** | `2a-two-forge-agents-overview.mp4`, disjoint `/logs` | id1(repoA)+id2(repoB) ran concurrently; **0 cross-talk** (id1 40 log lines/0 foreign task_id/0 mentions of id2; id2 24/0/0); independent cost $0.064 vs $0.040. Launched via **API** because the UI single-pane launch is unwired — see F2. |
| Two Loop Stacks, simultaneous | Web | 2 panes × 2 cards, run both | Wired | **PASS** | `2b-two-loop-stacks.mp4` (129s), `2b-hero-both-running.png`, `2b-01..12`,`2b-99` | Both stacks ran one-agent-per-repo, chained card 1→2 in order, reached terminal success; **0 cross-talk** (PANE0 only repoA cards, PANE1 only repoB); 0 console errors; independent per-task cost. |
| Concurrent orbs animate independently | Web | Per-card OrbDot | Client-only | **PASS** | `2b-hero-both-running.png` | Each card's cyan orb reflects its own task's phase; running cards show own "tools · Bash" live state. No shared/synced state. |
| Cost attribution under concurrency | Web+API | per-task `cost` | Wired | **PASS** | `/api/tasks` mid-run snapshot | Two running tasks accrued cost separately ($0.200 vs $0.202); never merged/cross-attributed. Polish-1 bug #3 holds at the task/DB layer. |
| Two concurrent sessions render on native app | macOS | ForgeView / cognition grid | Unverified | **BLOCKED** | build OK, `AgentEventGoldenTests` PASS, endpoint parity | Cannot observe render (locked screen). App builds, event→UI-model decode test passes, client targets the same `/api`+`/ws` proven correct in 2a/2b. Visual concurrency parity **unverified this run**. |

---

## B. Web surface — 6 nav routes (all HTTP 200, 0 console errors)

| Surface | Platform | Element | Type | Status | Evidence | Notes |
|---|---|---|---|---|---|---|
| Loop Stack `/stacks` | Web | tile grid, composer, cards | Wired | OK | `nav-stacks.png` | Primary surface. Composer adds cards; **dock/run appears only at ≥2 cards** (F2). 10 buttons, 2 inputs. |
| Loop `/loop` | Web | Loop Engineering dashboard | Wired | OK | `nav-loop.png` | SUCCESS RATE 100%, RUNS 7, **SPEND $1.33 (correct)**, cost-burn sparkline, per-run trace list. Reads `/api/loop-engineering`. |
| Budget `/budget` | Web | spend/burn/cap tiles | Client-only | **Broken (cost)** | `nav-budget.png` | Caps + stop-all wired; **SPENT $0.0000 / "no spend yet" despite $1.33 real spend** (reads client WS store). F6. |
| Scheduling `/schedules` | Web | cron schedules | Wired | OK | `nav-schedules.png` | Full CRUD verified against `/api/schedules` (create/enable/disable/delete). |
| Overview `/overview` | Web | rollup table + buckets | Wired / mixed | OK / **cost broken** | `nav-overview.png`, `2a-05.png` | **Bucket counts correct** (RUNNING 2·DONE 5·ALL 7 under load — Fix-1 #1 PASS); **COST col $0.0000 all rows** (F6). |
| Configuration `/config` | Web | settings | Wired | OK | `nav-config.png` | 200, no errors. |
| Topbar "N live" | Web | live counter | Broken | **Undercount** | `2a-05.png`, `2b-hero-both-running.png` | Showed "1 live" while **2** agents ran (both 2a and 2b). Same faulty source as `/api/stats.running`. F4. |
| Error banners across nav | Web | banner/alert | — | **PASS (none stick)** | sweep | 0 persistent banners across all 6 routes; none survive navigation. |

---

## C. Backend — 48 routes (route count `grep -c '.route(' = 49`)

| Surface | Platform | Element | Type | Status | Evidence | Notes |
|---|---|---|---|---|---|---|
| 23 GET routes | Backend | health,version,stats,config,spec,tasks,dead-letter,logs,audit,branches,repos,patterns,plans,cache/stats,quality/trend,routing/q-values,loop-engineering(+health,runs),schedules,tools,agents/health/summary,metrics | Wired | **all 200** | route sweep | Every GET route live. |
| `/api/tasks` POST | Backend | create task | Wired | 201 / **422** | Phase-3 checks | Valid→201; empty/whitespace goal→**422** (Fix-1 #5 PASS). |
| `/api/schedules` CRUD | Backend | create/enable/disable/delete | Wired | 200 | mutation sweep | Full cycle succeeds. |
| `/api/loop-engineering/escalation` POST | Backend | toggle | Wired | 200 | mutation sweep | — |
| `/api/loop-engineering/strategy`, `/api/tools` POST | Backend | set strategy / register tool | Wired | 422 on probe | mutation sweep | 422 = input validation on probe payloads (route live). |
| Bogus-id → `/logs` `/stream` `/dag` | Backend | error status | Broken | **200 (want 404)** | bogus-id sweep | main lacks the Ops-2 #8 fix the abandoned fix-branch had. F8. |
| Orphan routes (no UI caller) | Backend | infra + admin/debug | — | expected | cross-ref | `/metrics`,`/sse`,`/api/health` (external); `/api/agents/:id/{checkpoint,dag,health,rate-limit}`,`/api/cache/agent/:agent`,`/api/plans`,`/api/routing/q-values`,`/api/spec`,`/api/tasks/:id/stream` (admin/debug). **No new user-facing orphans.** |

---

## D. macOS surface — 12 `NavSection`s (Forge, Dashboard, Budget, Tasks, Cron, Loop, Dead-Letter, Tools, Health, Patterns, Audit, Config)

| Surface | Platform | Element | Type | Status | Evidence | Notes |
|---|---|---|---|---|---|---|
| App build | macOS | `xcodebuild -scheme Lopi` | — | **BUILD SUCCEEDED** | build log | Clean. |
| Event→UI decode | macOS | `AgentEventGoldenTests` | — | **PASS** | test log (1 test, 0 fail) | Confirms agent-event decode → UI model (the render pipeline's data layer). Coverage is thin (1 test). |
| Networking | macOS | LopiClient / EventStream | Wired | endpoint parity | source | Defaults to `127.0.0.1:<port>` (Settings-configurable) — same endpoints proven in 2a/2b. |
| Compact-orb idle→live morph | macOS | `matchedGeometryEffect` (AgentPaneView) | — | **Unverified** | source present | Cannot observe animation (locked). |
| Dashboard "N active" / COST TODAY, Budget SPENT, Loop SPEND, model label | macOS | dashboard tiles | — | **Unverified** | — | Cannot observe (locked). Web analogues: "N live" undercounts (F4), Budget $0 (F6), Loop $1.33 correct. |
| 12 nav sections (guided visual pass) | macOS | all views | — | **Unverified** | — | No UI-test target exists (stated plainly); no automated macOS UI coverage; visual pass impossible under lock. |

---

## Findings index (detail in `LIVE_UI_STATUS_FINAL.md`)

- **F1** `--config` silently swallows a partial/invalid TOML → falls back to default DB, no warning (violates repo "no silent failures"). Fix-1 #6 works with a *complete* config.
- **F2** *(most significant)* Bare pane (0–1 card) has **no launch control** — `runStack`/`createTask` are dock-only (≥2 cards); `paneSubmitPayload` is tested but has **zero callers**. Single-prompt "Forge" launch is unwired.
- **F3** `/api/stats` state counters wrong (`succeeded:3` vs 7 real; `running` undercounts). Cost/token totals correct.
- **F4** Web topbar "N live" undercounts (1 vs 2). `/overview` buckets are correct.
- **F6** Client-store cost surfaces show $0: `/budget` SPENT, `/overview` COST rows. Server surfaces (`/loop`, `/api/stats`) correct at $1.33.
- **F7** `tier.rs:81` pricing tier still advertises cut "Constellation routing (4 strategies)".
- **F8** Bogus-id 404 (Ops-2 #8) absent on `main` — returns 200.

**Total real cost of this audit: $1.3314** (8 tasks: 7 success + 1 expected-fail probe), 1,532,452 tokens.
