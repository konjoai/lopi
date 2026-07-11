# Next Session — after Verify-1

Verify-1 (CHANGELOG `[Unreleased]`) was the first fully-live, on-device audit
(real subscription auth, real billed runs — $1.33). **The concurrency
centerpiece is a clean PASS: two agents and two Loop Stacks run genuinely
simultaneously with zero cross-talk, correct isolation, correct per-task cost.**
No concurrency defect turned up — so, per plan, the honest next step is
**Launch-1**, with one newly-surfaced UI-wiring gap folded in as a blocker. See
`docs/ops/LIVE_UI_STATUS_FINAL.md` + `FEATURE_STATE_FINAL.md`.

## 1. Launch-1 — seamless start [the next real work] — with one blocker folded in

Nothing structural (concurrency, engine) blocks it. But **F2 must be fixed as
part of Launch-1**, because it breaks the most basic "just run one thing" path:

- **F2 — single-prompt launch is unwired in the `/stacks` grid.** A bare pane
  (0–1 card) renders no run control (`StackControlDock`/`RunMenu` are dock-only,
  gated by `paneIsBare` = ≤1 card). The dedicated bare-pane launch payload
  `paneSubmitPayload` (in `stack.ts`, unit-tested) has **zero callers**. So a
  user who types one prompt and hits Enter gets a draft card and **no way to run
  it** — they must add a second card (making it a stack) or use the API. Wire
  `paneSubmitPayload` to a bare-pane run affordance (composer Enter or a run
  button on the single card).

## 2. Fast-follow polish (surfaced by Verify-1, not blockers)

- **Cost surfaces read $0 on client-store views.** `/budget` "spent (session)"
  and `/overview` per-row COST show `$0.0000` while server cost is correct
  (`/loop` SPEND and `/api/stats` = $1.33). These surfaces derive from the client
  WS `agents` store, which carries no cost — either ship cost through the WS
  snapshot or have them read `/api/stats`.
- **State counters undercount.** Topbar "N live" and `/api/stats`
  `running`/`succeeded` are wrong (showed "1 live" while 2 ran; `succeeded:3` vs
  7 real). `/overview`'s own buckets are correct — align the counters to that
  (WS-snapshot + statusMap) source.
- **F1 — `sail --config` silently swallows a partial TOML.** A config missing the
  required `[claude]`/`[git]` tables fails `toml::from_str`, and
  `util::load_config`'s `.ok()` drops it to the default DB with no warning
  (violates the repo "no silent failures" rule). Log a `warn!` on load failure.
- **F8 — bogus-id endpoints return 200 (want 404)** for `/api/tasks/:id/{logs,
  stream}` and `/api/agents/:id/dag`. The Ops-2 #8 fix shipped on the abandoned
  `fix/ops-2-findings` branch but not in main's PR #78 — port it.
- **F7 — `tier.rs:81`** pricing tier still advertises cut "Constellation routing
  (4 strategies)". Drop the line.

## 3. Still-outstanding: macOS on-device *visual* confirmations

Verify-1 **could not** do these — the MacBook was locked for the whole
unattended run, so native-GUI rendering is unobservable. They remain open (the
web analogues were verified and are noted above). Confirm on an unlocked device:

- Compact-orb `matchedGeometryEffect` idle→live morph animates cleanly; multipane
  grid reads right on macOS.
- macOS Dashboard "COST TODAY", Budget "SPENT", Loop "SPEND" — check whether they
  hit the correct server source or the same client-store $0 as web.
- Dashboard cognition-grid "N active" count against a seeded mixed batch (web
  "N live" undercounts — verify whether macOS shares the bug).

## 4. Decisions already closed (do not re-litigate)

- Orb-parity → compact per-pane orb everywhere (Polish-1, `LEDGER.md`).
- Dashboard → kept as macOS-native richer view (Polish-1, `LEDGER.md`).
- Bug #3 (server cost/token accrual) → fixed; verified live at $1.33 in Verify-1.
