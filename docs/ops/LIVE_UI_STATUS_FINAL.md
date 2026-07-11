# LIVE_UI_STATUS_FINAL — Verify-1

**Baseline:** `main` @ `a6e4b5f` (Polish-1 / PR #79), v0.3.2 · **Date:** 2026-07-10
**Discipline:** everything real — live on-device, real Claude subscription auth (`ANTHROPIC_API_KEY` unset), real billed agent runs, scratch repos. Supersedes the Ops-2 `LIVE_UI_STATUS.md`.
**One environmental limitation up front:** the MacBook was at the **lock screen** for the whole unattended run. That makes native-GUI *visual* verification impossible (a locked display can't composite app windows; computer-use is blocked on the lock screen; unlocking is out of bounds), and makes a literal screen-recording of the physical display impossible (it captures only the lock screen). So the video/screenshot evidence is produced **headlessly via Playwright** (Chromium), which is unaffected by the lock, and the concurrency claims are backed by the **per-task event/log/cost data** — which is a *stronger* cross-talk proof than a video anyway. Every macOS-visual item is marked **Unverified (locked)** rather than smoothed over.

---

## HEADLINE — the concurrency tests

**Two agents running at once, and two Loop Stacks running at once, were watched end-to-end for the first time. There is ZERO cross-talk. No concurrency defect was found.**

### §2a — two simultaneous single agents — **PASS**
Two tasks were launched **simultaneously** (`Promise.all` on two `POST /api/tasks`) into two different scratch repos — repoA "summarize what main.rs prints", repoB "list all files and count total lines". They ran concurrently and both reached `success`.
- **Zero cross-talk, proven at the transcript level:** repoA task = 40 log lines, **0** carrying a foreign `task_id`, **0** mentioning the other task's id; repoB task = 24 log lines, **0** foreign, **0** mentions. Fully disjoint transcripts.
- **Independent cost:** $0.0637 vs $0.0396 — never merged.
- Evidence: `docs/videos/verify-1/2a-two-forge-agents-overview.mp4`, `docs/screenshots/verify-1/2a-05.png` (Overview showing **RUNNING 2**), scratch log dumps.
- **Caveat:** this was launched via the **API**, not the pane UI, because of **F2** — the grid cannot launch a single-prompt pane (see below). The concurrency/isolation question is answered; the UI launch path for the single-agent case is broken.

### §2b — two simultaneous Loop Stacks — **PASS**
Two panes, each given **2 cards** routed to repoA / repoB, were run by clicking both "run stack" buttons **simultaneously** (`Promise.all`, `RUNMAIN_COUNT=2`).
- Both stacks ran **one agent per repo concurrently**, then **chained card 1 → card 2 in order**, and all cards reached `success` (5/5 including the calibration task, @123s).
- **Zero cross-talk:** STACK ONE showed only its repoA cards ("count lines in README", "print first word of README"); STACK TWO only its repoB cards ("list files", "print how many files"). Each running card rendered its **own** live orb + "tools · Bash" state. **0 console errors.**
- Evidence: `docs/videos/verify-1/2b-two-loop-stacks.mp4` (129 s), `docs/screenshots/verify-1/2b-hero-both-running.png` (both panes mid-run), `2b-01..2b-12`, `2b-99-final.png`.

### §2c — cross-platform (macOS under concurrency) — **UNVERIFIED (locked)**
Cannot be observed: the machine is locked. What *is* confirmed structurally: macOS app **builds clean**, its **event→UI-model golden test passes**, and its client defaults to the same `127.0.0.1` `/api`+`/ws` endpoints proven correct in 2a/2b. Native visual parity under concurrent load remains **unverified this run** — flagged, not waved through.

**Bottom line on the centerpiece: the thing every prior round deferred now has direct evidence, and it works. No cross-talk, correct independent terminal states, correct per-task cost. The single most important result of this sprint is clean.**

---

## Phase 2 — carried-forward open items

1. **Compact orb (visual).** *Web:* **PASS** — the multipane grid reads correctly (2b footage): compact per-card orbs sit cleanly side-by-side, not cramped/broken; running cards animate their own orb. *macOS `matchedGeometryEffect` idle→live morph:* **Unverified (locked)** — code present in `AgentPaneView.swift`, not observable.
2. **Cost surfaces, for real.** **SPLIT / partially still-open.**
   - `/loop` SPEND = **$1.33** ✓ correct (matches `/api/stats.total_cost_usd_today`); real cost-burn sparkline; 7 runs listed.
   - `/budget` "spent (session)" = **$0.0000**, "no spend yet" ✗ — reads the client WS `agents` store, which carries no cost.
   - `/overview` COST column = **$0.0000** on every row ✗ — same client store.
   - So Polish-1 bug #3 fixed *server-side* cost (real in `/api/stats` and `/loop`), but **client-store-backed surfaces still show $0**. macOS Dashboard/Budget/Loop cost tiles **Unverified (locked)**.
3. **"N active" count.** *Web analogue:* the topbar **"N live" undercounts** — showed "1 live" while 2 agents ran (both 2a and 2b), same faulty source as `/api/stats.running`. `/overview`'s own RUNNING tab was **correct (2)**. *macOS Dashboard cognition-grid header:* **Unverified (locked)** — but the web evidence shows the shared counter is buggy.
4. **`FEATURE_STATE.md` §D caveats.** Resolved into `FEATURE_STATE_FINAL.md`: the "pending on-device confirmation" items are now either confirmed (web) or explicitly **Unverified (locked)** for macOS-visual — not left ambiguous.

---

## Phase 3 — regression-check every prior fix (fresh pass/fail)

| Check | Result | Evidence |
|---|---|---|
| `POST /api/tasks {"goal":""}` → 422 (Fix-1 #5) | **PASS** | empty→422, whitespace→422, valid→201 |
| Real task → correct terminal status, no malformed compound string (Fix-1 #4) | **PASS** | 8 tasks; DB statuses are clean tokens (`success`/`failed`); no `"failed ❌ Cancelled"` |
| `/overview` bucket counts vs real mixed batch (Fix-1 #1) | **PASS** | RUNNING 2 · DONE 5 · ALL 7 under live load (`2a-05.png`) |
| `sail --config <path>` honors custom `db_path` (Fix-1 #6) | **PASS (w/ caveat)** | complete config → `/api/config source:"file"`, scratch DB created. **F1:** a *partial* config is silently swallowed → default DB, no warning |
| Zero "constellation" refs, front + back | **FUNCTIONAL PASS / literal FAIL** | No live integration/UI/routes/views (removal holds). But 22 textual refs remain — all comments/tests/docstrings **except F7:** `tier.rs:81` pricing still sells "Constellation routing (4 strategies)" |
| No sticky banner survives navigation (different trigger) | **PASS (web)** | 6-route sweep: 0 persistent banners, 0 console errors; macOS decoding-banner path gone with Constellation |
| Model label matches running model (Fix-1 #7, macOS) | **Unverified (locked)** | Web stack dock shows "Opus 4.8" consistent with the running model |
| *(bonus)* Bogus-id → 404 for stream/logs/dag (Ops-2 #8) | **FAIL on main (F8)** | all return **200**; main's PR #78 omitted this fix |

---

## Phase 4 — feature/route inventory

- **Web:** all 6 nav routes HTTP 200, **0 console errors**, no dead-ends; classifications in `FEATURE_STATE_FINAL.md §B`. Screenshots `nav-*.png`.
- **macOS:** guided visual pass **not possible (locked)** and **no UI-test target exists** (stated plainly — no automated macOS UI coverage). Build + 1 golden decode test pass; 12 `NavSection`s enumerated from source.
- **Backend:** all 23 GET routes 200; mutation routes Wired (schedules CRUD, escalation; strategy/tools validate); route count confirmed (`grep -c '.route(' = 49`, 48 unique paths).
- **Cross-reference:** the only orphan (no-UI-caller) routes are external interfaces (`/metrics`, `/sse`, `/api/health`) and agent-internal/admin/debug endpoints (`/api/agents/:id/*`, `/api/cache/agent/:agent`, `/api/plans`, `/api/routing/q-values`, `/api/spec`, `/api/tasks/:id/stream`). **No new user-facing orphans.** (Earlier-suspected orphans — plan approve/reject, schedule enable/disable/run-now, dlq retry — are confirmed wired.)

---

## Full finding inventory (severity-ordered)

| # | Sev | Finding | Where |
|---|---|---|---|
| **F2** | **High (UX)** | Bare pane (0–1 card) has **no launch control**; single-prompt "Forge" launch is unwired (`paneSubmitPayload` has 0 callers; run is dock-only, needs ≥2 cards). | web `/stacks` |
| F6 | Med | Client-store cost surfaces show $0 (`/budget` SPENT, `/overview` COST) despite $1.33 real spend. | web |
| F3 | Med | `/api/stats` state counters wrong (`succeeded:3` vs 7; `running` undercounts). Cost/tokens correct. | backend |
| F4 | Med | Topbar "N live" undercounts (1 vs 2). | web + backend stat |
| F1 | Low-Med | `--config` silently swallows partial/invalid TOML → default DB, no warning. | CLI `sail` |
| F8 | Low | Bogus-id → 200 (want 404) on `main` (Ops-2 #8 not shipped). | backend |
| F7 | Low | Pricing tier still lists cut "Constellation routing". | `tier.rs` |

None of these is a **concurrency** defect. F2 is the most consequential for everyday use.

---

## OVERALL VERDICT — **CONDITIONAL GO**

**Is lopi ready for real day-to-day use as-is? Conditional — yes for concurrent multi-agent/multi-stack work, no for the frictionless single-task path.**

- **The concurrency question is a clean PASS.** Two agents and two stacks run genuinely simultaneously with zero cross-talk, correct isolation, correct independent terminal states, and correct per-task cost. Nothing here blocks Launch-1.
- **The blocking day-to-day issue is F2:** from the primary `/stacks` grid you cannot launch a *single* prompt — a bare pane has no run button, and the launch helper is unwired. Users must add a second card (turning it into a stack) or use the API. This is the one finding that meaningfully dents "just works." It is a UI-wiring gap, not an engine defect.
- Secondary polish: cost surfaces on `/budget` + `/overview` and the "N live"/`succeeded` counters read $0/undercount because they draw from the client WS store rather than the (correct) server stats.

**Recommendation:** the honest next step is **Launch-1 (seamless start)** — the concurrency backbone is proven — **with F2 folded in as a launch blocker** (wire `paneSubmitPayload` to a bare-pane run affordance), plus the cost-surface/counter cleanups as fast-follows. No urgent concurrency-defect sprint is required.

---

## Cost

**Total real spend for this audit: $1.3314** across 8 billed tasks (7 success + 1 expected-fail probe on an invalid repo), 1,532,452 tokens. Per-task ≈ $0.04–$0.35 (stack cards with ×25 max-iteration defaults cost more than bare API tasks).

## Evidence
- Videos: `docs/videos/verify-1/2a-two-forge-agents-overview.mp4`, `2b-two-loop-stacks.mp4`
- Screenshots: `docs/screenshots/verify-1/` (30 files — `2a-*`, `2b-*`, `2b-hero-both-running.png`, `nav-*`)
