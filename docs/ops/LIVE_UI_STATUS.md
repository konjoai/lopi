# lopi — Live UI Status (Ops-2)

**Audit date:** 2026-07-09/10 · **Commit:** `main` @ `4e9b390` (Unify-2, PR #76) ·
**Machine:** MacBook, macOS 15 (Darwin 25.5), Xcode 26.6, real `claude` subscription auth
(no `ANTHROPIC_API_KEY`). Companion data: [`FEATURE_STATE.md`](FEATURE_STATE.md) (the master
table) + [`evidence/`](evidence/). This report is the narrative; the table is the deliverable —
neither substitutes for the other.

---

## Headline — what works, live-verified

lopi is **real and mostly wired**, on a codebase that just went through a large nav collapse.

- **All three targets build clean** on macOS: `cargo build` ✓, web `npm run build` ✓,
  `xcodebuild -scheme Lopi` → **BUILD SUCCEEDED** ✓.
- **Backend test baseline: 1107 passed / 0 failed / 1 ignored** across 47 binaries (`cargo test --workspace`).
- **The full agent loop runs for real.** Seeding a task spawns a genuine `claude` agent
  (model `claude-haiku-4-5`), transitions Planning → Implementation → Testing, uses tools
  (`Bash`), creates a git branch, and completes. Verified end-to-end **twice**: via the REST API
  and via the `/stacks` composer's **"run stack"** button (two connected cards, live "pause" dock).
- **Both UIs are live clients of one backend.** The macOS Forge showed the *exact* sessions I ran
  from the web composer — data parity is real. The living orb renders on both (WebGL dot on web,
  Metal orb on macOS).
- **Backend API is healthy:** every one of ~55 method+path combos returned a real, sane result
  (see table). All 4 web nav routes load with a **clean browser console (0 errors/warnings)**.

The gaps are concentrated, not diffuse: a broken Overview rollup, a dead constellation
integration, zeroed cost accounting, an input-validation hole, and an ignored config key.

---

## Ops-1 "Known issues" — resolved or not

Ops-1 (PR #74) ran on **Linux**; all three of its known issues were Linux/headless artifacts. On
this macOS run:

| Ops-1 known issue | Status now |
|-------------------|-----------|
| macOS app not buildable (Linux) — inventoried from source only | **RESOLVED** — builds, launches, connects to `sail`, renders Metal orbs. |
| `/pulse` couldn't be screenshotted (headless software-GL crash) | **MOOT / RESOLVED** — `/pulse` was **cut** by Unify-2 (PR #76); route no longer exists. No headless crash observed on any surviving route (real GPU). |
| Screenshots used `?demo=1` mock, not live agents | **RESOLVED** — all live captures here are **real `claude` sessions** (running counter, tool use, branches, completion). No `?demo=1` used. |

Plus a doc-accuracy note: RUNNING.md says "`GET /` redirects (307) to `/stacks`" — the server
actually returns **200** and the redirect is client-side (SPA). Minor.

---

## Bug list (Broken-classified, severity-sorted)

1. **[High] `/overview` status buckets are wrong.** Badges show `ALL 20 / RUNNING 20 / QUEUED 0 /
   DONE 0` while the real DB is `{success:9, failed:6, cancelled:3, queued:2, running:0}`. Every
   task is mis-bucketed as *running*; QUEUED and DONE are empty despite real rows. Overview is 1 of
   only 4 nav tabs (it replaced Fleet+Dashboard+Pulse), so the app's primary at-a-glance rollup is
   incorrect. Likely root cause: task `status` strings in the store are malformed (e.g.
   `"failed ❌ Cancelled"` — emoji + duplicated text), so bucketing can't match and defaults to
   running. The same miscount surfaces as **"21 live"/"20 live"** in the web topbar, the web
   `/budget` "STOP ALL RUNNING (20)", and the **macOS** bottom-bar "20 live" — it is cross-platform.
2. **[High] Constellation integration is dead** — and it visibly breaks the macOS app.
   `api.ts` and the native app both call `/api/constellations` (+`/dispatch`, `/stats`) — **none
   exist** in the backend router. GETs fall through to the SPA static fallback (return HTML, not
   JSON); the POST returns 405. The **macOS Constellations screen renders a "Decoding error: the
   data couldn't be read because it isn't in the correct format"** banner
   ([`07-constellations-BROKEN.png`](evidence/macos/07-constellations-BROKEN.png)) — a live,
   user-visible failure, not just a latent one. That error toast is also **sticky**: it persists
   across every other macOS section and overlaps each page header until dismissed.
3. **[Med] Cost/token accounting is stuck at zero.** After ≥3 real billed agent runs,
   `/api/stats` reports `total_cost_usd_today: 0.0` and `total_tokens_today: 0`, and per-task
   `cost` is `null`. The web `/budget` and macOS Forge "Cost $0.0000" both read from this, so the
   entire budget/cost surface shows $0 regardless of real spend.
4. **[Med] Task status is not written back.** Individual `/api/tasks/:id` stays `status: "queued"`
   while the pool reports it `running` and then `succeeded`. This status lag is the likely engine
   behind bug #1's mis-bucketing.
5. **[Med] `POST /api/tasks` accepts an empty goal.** `{"goal":""}` → `201 Created` and **spawns a
   real agent** (`task_started` on `/sse`). Violates the input-validation rule
   (`.claude/rules/security.md`: "max goal length, character-set constraints"). No max-length or
   non-empty check at the boundary.
6. **[Med] `sail` ignores `db_path` from `--config`.** `sail_commands.rs:18` opens
   `crate::util::db_path()` unconditionally; the `LopiConfig.db_path` I set was silently ignored
   (scratch DB stayed 0 bytes; the real `~/.lopi/lopi.db` was used). `GET /api/config` also returns
   `{config:null, source:"none"}`, so the config surface doesn't reflect a loaded file. Data-isolation
   footgun.
7. **[Low] Model label mismatch.** The `/stacks` run dock labels the stack **"Opus 4.8"**; the macOS
   idle Forge pane shows **"Sonnet 4.6"**; the runner actually selected **`claude-haiku-4-5`**. The
   displayed model does not match what runs.
8. **[Low] `GET /api/tasks/:id/stream` returns `200` with an error body** (`{"error":"run not
   found"}`) instead of a 404 — status/body mismatch. (`/api/agents/:id/dag` and `.../logs` are
   similarly permissive: 200-empty on bogus ids rather than 404.)
9. **[Low, not a bug] "Resize columns" (`/stacks`)** produced no observable effect on click —
   classified Stubbed; may be a hover/drag affordance rather than a click target.

## Broken-link / dead-end list

- **None found.** Every nav link and wordmark resolved to a real, non-empty, non-404 destination.
  The one candidate — `/onboard`'s "Forge dashboard → /" rendering empty in a `domcontentloaded`
  snapshot — was **verified not broken**: `/` client-hydrates to `/stacks` (final URL `/stacks`,
  body renders). Recorded here so it isn't re-flagged next audit.

## Unwired / Stubbed inventory (what's "fake", by surface)

- **Web `/stacks`:** `Resize columns` (no-op observed). Card config buttons (schedule/guardrails/
  evals/run-config) are Client-only popovers — real config UI, but their *effect* on a run was not
  live-verified.
- **Web `/config`:** `ICE` theme + `tree` view toggle report as Stubbed **only because they are the
  active default** (clicking the current selection is a no-op). Not real stubs.
- **Web `/budget`:** cap number input (fill produced no visible state change) — likely needs commit;
  otherwise the whole surface reads $0 because of cost-accounting bug #3.
- **Web orphans `/budget`, `/loop`:** fully reachable by URL but **removed from nav** by Unify-2.
  `/loop` is actually well-wired (run traces, escalation switch) — a working surface with no way in.
- **macOS:** per `macos/README.md`, the admin panels beyond Forge/Dashboard/Cron are "Phase 1–2"
  with several stubbed — **not independently verified this run** (see coverage gap below).

## Backend routes with no frontend caller (`api.ts` cross-reference)

Live + healthy, but nothing in the web UI calls them (candidates for pruning or for "backend-only"
documentation): `GET /api/health`, `GET /api/branches`, `GET /api/plans`, `GET /api/spec`,
`GET /api/routing/q-values`, `GET /api/agents/:id/dag`, `GET /api/agents/:id/health`,
`POST /api/agents/:id/checkpoint`, `POST /api/agents/:id/heartbeat`,
`GET|POST|DELETE /api/agents/:id/rate-limit`, `DELETE /api/cache/agent/:agent`,
`GET /api/tools/:name`, `GET /api/tasks/dead-letter/:id`, `GET /api/tasks/:id/stream`, `GET /sse`,
`GET /ws/tasks` (legacy alias). (Several agent-`:id` routes are agent-internal by design; `/ws` —
not `/sse` — is the channel the UI actually uses.)

The inverse also exists and is worse: **4 `api.ts` functions with no backend route** (the
constellation endpoints, bug #2).

---

## Orb parity finding

Both surfaces render the "living orb" against a real GPU. **But they diverged in Unify-2:** the web
`/stacks` now shows a compact WebGL orb **dot** per stack card (`OrbDot.svelte`), while macOS Forge
still renders the prominent full-pane **Metal** orb (`ForgeOrb.metal`). Same concept, materially
different prominence. Captured live on both: web in [`evidence/web/video-orb/`](evidence/web/video-orb/)
and `orb-live-t15s.png`; macOS in [`evidence/macos/forge-default.png`](evidence/macos/forge-default.png).

---

## Total real cost

**lopi's own accounting is unreliable here (bug #3): it reports `$0.00` / `0 tokens`.** Actual
subscription spend was a **handful of small `claude-haiku-4-5` agent runs** — 3 completed sessions
(one API-seeded, two via the composer "run stack") plus one empty-goal no-op and a couple of
cancelled/queued tasks. Order of magnitude: **cents**, not dollars. It could not be metered exactly
because lopi's cost tracking returns zero and no other meter was available.

---

## macOS coverage (complete)

The macOS pass is **manual** (there is no `LopiTests`/UITest target to script), but it is now
**complete** — all 13 `NavSection`s were interactively clicked, observed, and classified in a
second pass after a computer-use grant. **Result: 12 Wired, 1 Broken (Constellations).** Every
non-Forge admin panel (Dashboard, Cron, Dead-Letter, Health, Patterns, Audit, Config) loads real
data from the shared backend and renders correctly; the macOS Dashboard even buckets task status
*correctly* (RUNNING 0), better than the web `/overview`. Destructive controls (DLQ Retry/Discard,
"Stop all running") were classified from `api.ts` wiring, not clicked. Full per-section table in
[`FEATURE_STATE.md`](FEATURE_STATE.md) §C; evidence in [`evidence/macos/`](evidence/macos/).

Cross-platform note: the cost bug (#3), the "N live/active" miscount, and the config-ignored bug
(#6) are all independently visible on the macOS surfaces (Dashboard/Budget/Loop show $0/$-0.00;
"20 active"/"Stop all running (20)"; Config says "No lopi.toml found — source: none").

---

## Out of scope (unchanged)

This sprint **records**; it fixes nothing. Bugs #1–#9 are findings for a future fix sprint. Unify-2
follow-ups and Launch-1 seamless-start are separately sequenced.
