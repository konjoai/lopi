# lopi — Feature State (Ops-2 full-state audit)

**Empirically verified** on `main` @ `4e9b390` (Unify-2, PR #76), macOS 15 (Darwin 25.5),
2026-07-09/10. Every row below is backed by an **observed action** — a real request
fired, a click classified by network/DOM delta, or a screenshot — not a code comment.

- **Method:** backend routes hit with `curl` against a live `lopi sail` (dev-mode, auth off);
  web controls classified with a throwaway Playwright script (deleted post-run); macOS via
  non-interactive `screencapture` of the running native app.
- **Status legend:** `Wired` = fires the network call it visually claims + real state change ·
  `Client-only` = changes local/store state, no network · `Stubbed` = nothing observably happens ·
  `Broken` = does the wrong thing (error / wrong result) · `Untested` = not exercised (reason in Notes).
- **Evidence** files live under [`evidence/`](evidence/): `api/sweep.txt`, `web/sweep.txt` + `sweep.json` + PNGs,
  `web/video-orb/*.webm`, `macos/forge-default.png`, `api/cargo-test-full.log`.

Scope note: PR #76 (Unify-2) collapsed the web nav to **4 tabs**; RUNNING.md's older 15-tab
nav list is stale. Routes `/budget` and `/loop` still resolve by URL but are **not in the nav**
(orphans). macOS still ships the older 13-section nav.

---

## A. Backend API — all 49 `.route()` regs (≈55 method+path combos)

Surface = Backend · Platform = both (web + macOS clients share this API) · Type = route.
Full transcript: [`evidence/api/sweep.txt`](evidence/api/sweep.txt).

| Method | Path | Status observed | Sane? | Frontend caller (api.ts) | Notes |
|--------|------|-----------------|-------|--------------------------|-------|
| GET | /api/health | 200 `{status:ok}` | Y | **none** | ops/liveness only |
| GET | /api/tasks | 200 tasks[] | Y | listTasks | |
| POST | /api/tasks | 201 / 422 | Y (⚠) | createTask | **`{"goal":""}`→201 + spawns agent** (validation gap) |
| GET | /api/tasks/:id | 200 / 404 | Y | getTask | 404 on bogus id (wired) |
| DELETE | /api/tasks/:id | 200 | Y | deleteTask | cancel/delete |
| POST | /api/tasks/:id/plan/approve | 409 | Y | approvePlan | 409 "not awaiting approval" (wired) |
| POST | /api/tasks/:id/plan/reject | 409 | Y | rejectPlan | wired |
| GET | /api/repos | 200 repos[] | Y | listRepos | |
| GET | /api/branches | 200 branches[] | Y | **none** | not called by api.ts |
| POST | /api/agents/:id/checkpoint | 422 (missing `state`) | Y | **none** | agent-internal; wired+validates |
| GET | /api/stats | 200 | Y | getStats | running/cost counters (see cost bug) |
| GET | /api/patterns | 200 patterns[] | Y | listPatterns | |
| GET | /api/plans | 200 plans[] | Y | **none** | billing tiers; no UI caller |
| GET | /api/spec | 200 | Y | **none** | KCQF spec surface; no UI caller |
| GET | /api/quality/trend | 200 | Y | qualityTrend | |
| GET | /api/routing/q-values | 200 `{values:[]}` | Y | **none** | RL routing; no UI caller |
| GET | /api/agents/:id/dag | 200 (empty dag on bogus) | Y (permissive) | **none** | returns 200 empty, not 404 |
| GET | /api/tools | 200 tools[] | Y | listTools | |
| POST | /api/tools | 422 (missing `parameters`) | Y | registerTool | wired+validates (my probe used wrong schema) |
| GET | /api/tools/:name | 404 on miss | Y | **none** | `getTool` not in api.ts |
| DELETE | /api/tools/:name | 404 on miss | Y | deleteTool | wired |
| GET | /api/cache/stats | 200 | Y | cacheStats | |
| DELETE | /api/cache | 200 `{deleted:0}` | Y | clearCache | |
| DELETE | /api/cache/agent/:agent | 200 `{deleted:0}` | Y | **none** | no UI caller |
| GET | /api/tasks/dead-letter | 200 dead_letters[] | Y | listDlq | |
| GET | /api/tasks/dead-letter/:id | 404 on miss | Y | **none** | list used, not get-by-id |
| DELETE | /api/tasks/dead-letter/:id | 404 on miss | Y | deleteDlq | wired |
| POST | /api/tasks/dead-letter/:id/retry | 404 on miss | Y | retryDlq | wired |
| GET | /api/audit | 200 events[] | Y | queryAudit | |
| POST | /api/agents/:id/heartbeat | 200 (creates for bogus id) | Y (permissive) | **none** | agent self-register; accepts any id |
| GET | /api/agents/:id/health | 404 on miss | Y | **none** | only summary used by UI |
| GET | /api/agents/health/summary | 200 | Y | healthSummary | |
| GET | /api/tasks/:id/stream | 200 (SSE; `{error:run not found}` body) | ~ | **none** | 200 + error body mismatch; UI uses /ws not this |
| GET | /api/tasks/:id/logs | 200 logs[] | Y | taskLogs | 200 empty on bogus |
| GET | /api/logs | 200 logs[] | Y | recentLogs | |
| GET | /api/agents/:id/rate-limit | 404 on miss | Y | **none** | no UI caller |
| POST | /api/agents/:id/rate-limit | 422 (missing `max_per_minute`) | Y | **none** | wired+validates |
| DELETE | /api/agents/:id/rate-limit | 404 on miss | Y | **none** | wired |
| GET | /api/schedules | 200 schedules[] | Y | listSchedules | |
| POST | /api/schedules | 200 (created) | Y | createSchedule | seeded/deleted a scratch row OK |
| GET | /api/schedules/:id | 200 | Y | getSchedule | |
| PUT | /api/schedules/:id | 200 | Y | updateSchedule | verified update |
| DELETE | /api/schedules/:id | 200 `{deleted:id}` | Y | deleteSchedule | verified |
| POST | /api/schedules/:id/enable | 200 `{enabled:true}` | Y | enableSchedule | verified |
| POST | /api/schedules/:id/disable | 200 `{enabled:false}` | Y | disableSchedule | verified |
| POST | /api/schedules/:id/run-now | (not fired — spawns agent) | — | runScheduleNow | Untested-live: verified wired via code; not clicked to avoid spawn |
| POST | /api/schedules/:id/autonomy | 200 | Y | setScheduleAutonomy | verified |
| GET | /api/loop-engineering | 200 | Y | getLoopEngineering | |
| GET | /api/loop-engineering/health | 200 attempts[] | Y | getLoopHealth | |
| GET | /api/loop-engineering/runs | 200 runs[] | Y | getLoopRuns | |
| GET | /api/loop-engineering/runs/:id | 404 on miss | Y | getLoopRunTrace | wired |
| POST | /api/loop-engineering/strategy | 422 (unknown strategy) | Y | setLoopStrategy | wired+validates |
| POST | /api/loop-engineering/escalation | 200 `{escalate_strategy:true}` | Y | setLoopEscalation | verified |
| GET | /api/config | 200 `{config:null,source:none}` | ~ | getConfig | **--config not surfaced** (see config bug) |
| GET | /api/version | 200 v0.2.0 | Y | getVersion | |
| GET | /metrics | 200 (prometheus) | Y | n/a | infra |
| GET | /sse | 200 (event stream) | Y | **none** | UI uses /ws (WebSocket), not SSE |
| GET | /ws | 400 w/o upgrade | Y | wsClient | correct; real WS used by UI |
| GET | /ws/tasks | 400 w/o upgrade | Y | **none** | legacy compat alias |
| GET | POST `/api/constellations` · `/api/constellation/:n/dispatch` · `/api/constellation/:n/stats` | 200 **HTML** / 405 / 200 **HTML** | **N** | listConstellations, registerConstellation, dispatchConstellation, constellationStats | **BROKEN: routes don't exist** — fall through to SPA static fallback; frontend calls dead |

---

## B. Web frontend — 4 nav routes + 3 orphan routes

Surface = Web · Platform = web · console **clean (0 errors/warnings) on every route**.
Transcript: [`evidence/web/sweep.txt`](evidence/web/sweep.txt); screenshots `evidence/web/*.png`.

### App shell (present on every route)
| Element | Type | Status | Evidence | Notes |
|---------|------|--------|----------|-------|
| Hamburger "Toggle navigation" | button | Client-only | sweep.txt | opens off-canvas sidebar |
| Nav links: Loop Stack / Scheduling / Overview / Configuration | link | Wired(link) | HTTP 200, non-empty | all 4 land on real routes |
| "lopi" wordmark → /stacks | link | Wired(link) | HTTP 200 | |
| "?" / "PRESS ? FOR SHORTCUTS" | button | Client-only | DOM change | shortcuts modal |
| "Close navigation" | button | Client-only* | timeout when sidebar closed | *false-Stubbed when off-canvas; works when open |

### `/stacks` — Loop Stack (default, nav)
| Element | Type | Status | Evidence | Notes |
|---------|------|--------|----------|-------|
| "add a prompt or goal…" input | form | Wired | orb-2cards.png | Enter → creates a loop card |
| Card controls: ⟳×25 / schedule / guardrails / evals(1) / run config | button | Client-only (popovers) | orb-1-card-added.png | open config popovers |
| "add to stack" / "+" | button | Client-only | sweep.txt | adds card |
| **"run stack"** (dock, appears at ≥2 cards) | button | **Wired** | orb-live-t15s.png, video-orb | POST /api/tasks → real agent, live orb, "pause" dock |
| "run until the stack acceptance" | button | Untested (destructive/spawn) | code read | run-until-goal; not clicked |
| "delete" / "delete stack" / "close pane" | button | Untested (destructive) | — | not clicked; wired to store per code |
| "Resize columns" | button | Stubbed | sweep.txt | no observable effect on click |

### `/schedules` — Scheduling (nav)
| Element | Type | Status | Evidence | Notes |
|---------|------|--------|----------|-------|
| initial load | route | Wired | GET /api/schedules on mount | |
| "+ NEW" | button | Untested (mutating) | code read | opens create form (wired to createSchedule) |

### `/overview` — Overview (nav)
| Element | Type | Status | Evidence | Notes |
|---------|------|--------|----------|-------|
| Status filter chips ALL/RUNNING/QUEUED/DONE/DEAD-LETTER | button | Client-only | sweep.txt | filter task list locally |
| **Status bucket counts** | display | **Broken** | overview-counts.png | ALL=20 **RUNNING=20** QUEUED=0 DONE=0 vs real {success:9,failed:6,cancelled:3,queued:2,running:0} |
| Task rows (click) | row/button | Wired | GET /api/repos on click | opens task |

### `/config` — Configuration (nav)
| Element | Type | Status | Evidence | Notes |
|---------|------|--------|----------|-------|
| initial load | route | Wired | GET /api/config, /api/version | note: /api/config returns null (config bug) |
| Theme EMBER / JADE | button | Client-only | sweep.txt | switch theme |
| Theme ICE | button | Client-only* | sweep.txt | *reported Stubbed = active-default no-op |
| tree / raw view toggle | button | Client-only* | sweep.txt | *"tree" active-default no-op |

### `/budget` — orphan (URL-reachable, not in nav)
| Element | Type | Status | Evidence | Notes |
|---------|------|--------|----------|-------|
| $1/$5/$10/$25/$50 cap presets | button | Client-only | budget.png | local cap selection |
| cap number input | form | Stubbed | sweep.txt | fill produced no DOM change |
| "◼ STOP ALL RUNNING (20)" | button | Untested (destructive) | budget.png | "(20)" mirrors the overview miscount |

### `/loop` — orphan (URL-reachable, not in nav)
| Element | Type | Status | Evidence | Notes |
|---------|------|--------|----------|-------|
| initial load | route | Wired | GET /api/loop-engineering (+/health,/runs) | |
| Run rows (expand) | button | Wired | GET /api/loop-engineering/runs/:id | per-run trace |
| Escalation switch | switch | Wired | POST /api/loop-engineering/escalation | + refetch |
| Strategy cards S2/S3/S4 | button | Client-only | sweep.txt | select strategy locally |
| Strategy card S1 (active) | button | Client-only* | sweep.txt | *active-default no-op |

### `/onboard` — hidden
| Element | Type | Status | Evidence | Notes |
|---------|------|--------|----------|-------|
| "🔗 Install GitHub App" | button | Wired(external) | onboard.png | navigates to external GitHub URL (failed in offline headless) |
| "Forge dashboard" → / | link | Wired | verify (final url /stacks) | domcontentloaded snapshot looked empty; **/ client-hydrates to /stacks** — not broken |

---

## C. Native macOS app (`Lopi.app`) — 13 NavSections

Surface = macOS · Platform = macOS · **manual/screenshot pass, NOT scripted** (no `LopiTests`
UITest target exists). Evidence: [`evidence/macos/forge-default.png`](evidence/macos/forge-default.png).

**Build + launch + connect: verified.** `xcodebuild -scheme Lopi` → BUILD SUCCEEDED; app launches,
connects to `lopi sail` on :3000, renders live data and Metal orbs. This resolves Ops-1 Known
Issue #1 (which said the app was Linux-un-buildable and inventoried from source only).

**All 13 sections were interactively swept** (computer-use, second attempt granted). Each was
clicked, observed, and classified. Result: **12 Wired, 1 Broken.**

| Section | Status | Evidence | Notes |
|---------|--------|----------|-------|
| Forge | **Wired** | forge-default.png | 3 panes; 2 show the exact sessions run via web (data parity); Metal `ForgeOrb` renders (green=done, blue=idle); composer w/ model/effort/repo/branch selectors |
| Dashboard | **Wired** | 02-dashboard.png | RUNNING 0 / QUEUED 0 / SUCCEEDED 3 / FAILED 0 (**correct** buckets, unlike web /overview); COST TODAY **$0.00** (cost bug); "AGENT COGNITION" grid = 20 task cards w/ correct per-card status; only the "20 active" label shows the miscount |
| Budget | **Wired** | (inline) | SPENT $0.0000, HOURLY CAP $5.00, cap presets $1–$50, "Stop all running (**20**)" miscount, "no spend yet" (cost bug) |
| Tasks | **Wired** | (inline) | master-detail list, **correct** per-task status (my 2 runs show success); "+" create |
| Cron | **Wired** | 05-cron.png | empty state "No schedules yet"; reads /api/schedules; "+" create |
| Loop | **Wired** | (inline) | SUCCESS 100%, RUNS 11, SPEND **$-0.00** (cost bug renders negative-zero), outcome dist, Recent Runs trace w/ my runs |
| **Constellations** | **BROKEN** | 07-constellations-BROKEN.png | banner **"Decoding error: the data couldn't be read…"** — fetches `/api/constellations` (route doesn't exist → HTML) → JSON decode fails. Native confirmation of the dead constellation integration |
| Dead-Letter | **Wired** | (inline) | real DLQ rows (Cancelled / Max retries exceeded), attempts, timestamps; Retry/Discard per row (destructive — not clicked, wired to retryDlq/deleteDlq) |
| Tools | **Wired** | (inline) | shows registered `test-search` tool + params (matches GET /api/tools); "+" add |
| Health | **Wired** | (inline) | agent-health summary cards 1/1/0/0 (total/healthy/degraded/dead) from /api/agents/health/summary |
| Patterns | **Wired** | (inline) | mined goal-keyword clusters, success bars, avg attempts, "seen" timestamps (incl. today's run) |
| Audit | **Wired** | 12-audit.png | event log: task.dispatch / task.dead_letter w/ payloads, actor, timestamps (/api/audit) |
| Config | **Wired** | 13-config.png | cache stats (0 entries); SERVER CONFIG **"No lopi.toml found on the server — defaults in effect / source: none"** — confirms the `--config`-ignored bug from the native UI |

> **macOS-specific bug (minor):** the Constellations "Decoding error" toast is **sticky** — it
> persists across every subsequent section and overlaps the top of each page's header until
> manually dismissed. Stems from the constellation-route failure above.
>
> **Honest coverage note:** this is still a *manual* pass (no `LopiTests`/UITest target exists) —
> each section was visually classified from its rendered state and safe interactions; destructive
> controls (DLQ Retry/Discard, "Stop all running") were not clicked, classified from api.ts wiring.

---

## D. Orb parity (web vs macOS)

| Aspect | Web (`/stacks`) | macOS (Forge) | Finding |
|--------|-----------------|----------------|---------|
| Renderer | WebGL `OrbDot` (small dot per card) | Metal `ForgeOrb` (large full-pane orb) | **Diverged** post-Unify-2 |
| Live motion | small teal dot on running card | large animated green/blue orb per pane | both animate on real GPU |
| Evidence | video-orb/*.webm, orb-live-t15s.png | forge-default.png | captured on both |

Post-Unify-2 the web's large Forge orb was replaced by a compact per-card orb *dot*; macOS still
renders the prominent Metal orb. Same concept, materially different prominence — a real parity gap.
