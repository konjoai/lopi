# LIVE_UI_STATUS_FINAL

> ## ✅ Verify-4 addendum — macOS Loop Stacks: built, tested, watched, **attended** (2026-07-11, baseline `9edca88` / v0.4.0 + PR #84)
>
> Verify-4 is the first time the 4,354-line `macOS-Loop-Stacks-1` code was **compiled anywhere** — it was authored on a Linux host that structurally can't build Swift. Ran on the M3, unlocked + attended, `caffeinate -dimsu`, computer-use full tier, real `claude` CLI 2.1.153, no `?demo=1`, no mocks. **Every phase held; two real first-compile defects were found and fixed.**
>
> **Phase 0 — does it build? ONE real defect, one root cause.** 4,354 never-compiled lines produced exactly **one** compile error (two diagnostics from one cause): `SchedulePopoverView.swift:109` — the cron `TextField` binding's `set:` closure used `$0`, which Swift bound to the inner IIFE `{ … }()` instead of the setter parameter (→ *"missing argument for parameter #1"* + *"closure expects 2 arguments"*). Fixed by naming the setter parameter (`set: { raw in … }`). Clean build after, **zero warnings-as-errors suppressed**. A single structural SwiftUI closure-inference subtlety, not a pile of typos — the Linux authoring was remarkably clean.
>
> **Phase 1 — do the ported tests pass? ONE compile-gap, then 60/60.** `StackRunTests.swift`'s nested `Mock` helper was non-isolated but its `seams()` closures synchronously touch `@MainActor` `StackStore` members — a pure Swift-concurrency porting gap (web TS has no actor model). Marked `Mock` `@MainActor` (mirrors production `AppModel`). After that, **all 60 tests pass on first execution — StackGoal 5, StackRun 19, StackStore 31** (+ 5 pre-existing), **zero behavioral discrepancies** in the 55 never-executed ported assertions. The port is behaviorally faithful to web; the only fixes were compile-level.
>
> **Phase 2 — single-card regression (the non-negotiable bar): HELD.** A bare Forge pane renders as the old single-agent pane (idle orb + model/effort/repo/branch control row + `> type a goal`, matching Verify-2 baseline `p1-01`) and **launches identically** — `> type a goal` → real `claude` run, idle→Planning→isolated branch→streaming transcript, own sidebar session. Re-confirmed against the isolated backend via a 1-card pane (branch `lopi/df76bcb2…`, `p2-01`). *(The first idle-orb frame was captured but lost to the mid-run git-clean incident below; the functional parity is server-logged and was re-captured.)*
>
> **Phase 3 — the multi-card stack, live. Every WIRED claim confirmed by an observed network call.** A 2-card stack (reversed order — bottom card runs first) exercised end-to-end (`p3-01…p3-10`): connector **insert-between** inserts a card at the right index; all **four popovers** work — **Schedule** (the exact view the Phase-0 bug lived in — typing `30 9 * * 1` into the raw CRON field flips freq→custom and recomputes next-runs, proving the fix's *setter* half at runtime), **Guardrails** (gate/until/on-fail/budget/max-iter), **Evals** (loop-validation checklist), **Config** (model/effort/repo/branch/autonomy). The `StackControlDockView` goal toggle carries an honest *"a goal with nothing to check is inert"* disclosure until chain-acceptance evals are set; the stop-reason banner fires on halt.
> The card I set distinctive guardrails on submitted, **observed on the wire**, exactly what the UI showed:
> ```
> create_task  max_iterations=26  on_fail=Continue  gate="test -f Cargo.toml"
>              until="test -d crates"  client_ref=<card-UUID>
>              acceptance_present=false  budget_tokens=None
> ```
> `max_iterations` / `on_fail` / `gate` / `until` / `client_ref` all **WIRED** ✅; `budget_tokens` + `acceptance` **absent** despite budget:auto + 4 evals selected ✅ (deliberately not wired, per the changelog's honesty stance). **Evals is client-only intent** — confirmed: chain acceptance is evaluated by *spawning a real verify task* (`client_ref="s1::stack-eval::0"`, `max_iterations=1`, `acceptance_present=false`), never by wiring an `acceptance` field. **run-until-goal halted with the right reason:** `goal met — stack acceptance passed` (green `goalMet` banner). Real results, no mocks: 17 crates enumerated, 203 `.rs` files counted.
>
> **Phase 4 — two simultaneous multi-card stacks: ZERO cross-talk.** New territory beyond Verify-2's two-*agent* proof. Two 2-card stacks launched together, each ran its own chain independently. Cross-talk ruled out four ways: (1) **distinct isolated git branches** per card (8 agent branches, all in the sandbox clone); (2) **distinct `client_ref`s per stack** — STACK ONE `72B6E6B1`/`39D239AF`/`s1::stack-eval`, STACK TWO `C7105FBC`/`22CE1D71`, never crossed; (3) **divergent mid-run progress** — STACK TWO reached its 2nd card while STACK ONE was still on its 1st (`p4-03`), so no lock-step; (4) **independent completion** — STACK ONE showed the `goalMet` banner (goal-pursuit on), STACK TWO ran plain `run stack` with no banner, correctly differentiated. Real results stayed in their own lanes (STACK TWO: top-level dirs + `.md` count; STACK ONE: crates + `.rs` count).
>
> **One process finding (not a product defect):** pointing a live run at repo `.` while the backend's cwd was the *actual dev working tree* let lopi's `GitManager` check out agent branches (`lopi/<taskid>-attempt-N`) in that tree and `git clean` untracked files — expected engine behavior aimed at the wrong directory. Isolated correctly by running the backend from a throwaway local clone; the product's per-agent branch isolation is working as designed. Standing guidance for future attended macOS runs: **never point runs at the repo you're editing** — run `lopi sail` from a clone.
>
> **Verdict: macOS Loop Stacks is genuinely confirmed, not shipped-on-faith.** It compiles (1 fix), its ported tests pass (1 fix, then 60/60 with zero behavioral drift), the single-card regression held, and the live multi-card + dual-stack-concurrency claims are backed by observed network calls and real Claude runs. Verify-4 live spend ≈ **$1.6** (isolated-clone backend). Evidence: `docs/screenshots/verify-4/` (`p2-01`, `p3-01…10`, `p4-01…04`).

---

> ## ✅ Verify-2 addendum — macOS visual verification, **attended** (2026-07-11, baseline `cf0344f` / v0.3.3)
>
> The whole "Unverified (locked)" column below was the product of two unattended, **locked**-screen runs. Verify-2 ran with the MacBook **unlocked and attended**, driving the real native `Lopi.app` on the physical display (computer-use, full tier) with real `ffmpeg`/avfoundation screen recordings — the exact thing the lock made impossible. Every locked item is now resolved:
>
> | Verify-1 item | Verify-2 result | Evidence |
> |---|---|---|
> | §2c macOS concurrency | **CONFIRMED** — two agents run simultaneously on the native app; two distinct "Implementing" cards, own goal/progress/branch, LIVE-ACTIVITY feed correctly attributed per goal, **zero cross-talk** (also disjoint per-task logs), independent Success terminal states | `docs/videos/verify-2/phase5-concurrency.mp4`; `p5-01/02/03` |
> | Compact-orb `matchedGeometryEffect` morph | **CONFIRMED** — clean idle-large-orb → compact-live-orb morph (no jump); compact orb legible, animates Planning(teal)→Success(green) through real phases to completion | `docs/videos/verify-2/phase1-orb-morph.mp4` (144s); `p1-01/02/03` |
> | "N active" cognition-grid count | **CONFIRMED CORRECT** — "2 active" against 2 running + 5 done; Polish-1's historical-hydration miscount does **not** recur | `p3-02-dashboard-mixed-live.png` |
> | 12-section guided pass | **CONFIRMED** — all 12 sections render, **zero crashes, zero stuck banners**, nothing broken/unfinished | `p4-*.png` |
> | Model label (Fix-1 #7) | macOS pane config shows the model (e.g. Sonnet 4.6) consistent with the run | `p1-*` |
>
> **One real defect found (new): `F9`+`F10` — macOS Dashboard stat tiles are wrong.** `COST TODAY $0.00` (real `$0.10`), `RUNNING 1` (real 2), `SUCCEEDED 1` (real 3), Budget `SPENT $0.00`. Root cause: those tiles read the `model.stats` path — updated by the **per-pool** WS `.poolStats` event (the multi-repo undercount) + a **connect-only** REST fetch — and the client per-agent cost sum, which isn't populated. This is the **macOS analog of the web F3/F4 + F6 fixes, which Fix-2 applied to web only.** What's correct on macOS: **Loop SPEND `$0.10`**, the **cognition-grid "N active"**, the **Tasks** list, and every other section (they read the server `/api/loop` / the agents map). → deferred to **Fix-3 (macOS stats/cost parity)**, scoped separately; Verify-2 itself changes no behavior.
>
> **Verdict: macOS-visual is closed.** Everything that needed eyes-on-screen was confirmed on the real display; the sole finding (F9/F10) is a data-plumbing defect, not a rendering one. Total Verify-2 cost: **$0.3896**, 1.41M tokens.

---

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
