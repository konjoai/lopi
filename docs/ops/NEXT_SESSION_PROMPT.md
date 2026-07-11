# Next Session — after Verify-2

Verify-2 (CHANGELOG `[Unreleased]`) was the first **attended, unlocked** on-device
run. It closed the `Unverified (locked)` gap that Verify-1 and Fix-2 both left
open: on the real physical display, the compact-orb `matchedGeometryEffect`
morph, the two-agent concurrency capstone (zero cross-talk), the "N active"
cognition count, and all 12 nav sections are **CONFIRMED**. See the Verify-2
addendum at the top of `docs/ops/LIVE_UI_STATUS_FINAL.md`.

It surfaced exactly one real defect, which is now the next work.

## 1. Fix-3 — macOS stats/cost parity [the next real work]

**F9 + F10 — the macOS Dashboard stat tiles read the wrong source.** On real
billed runs: COST TODAY `$0.00` (real `$0.10`), RUNNING `1` (real 2), SUCCEEDED
`1` (real 3), Budget SPENT `$0.00`. This is the macOS analog of the web F3/F4 +
F6 fixes — Fix-2 fixed **web only**. Concretely:

- **Counts (F10):** `model.stats.running/succeeded` are updated by the WS
  `.poolStats` event (`AppModel+Live.swift`), which carries a **single pool's**
  counters — the multi-repo undercount. Source them from the DB-corrected path
  instead (the WS `pool_stats` event still emits per-pool; either make it carry
  DB `status_counts`, or have macOS count from its own live session map the way
  the cognition grid already does correctly).
- **Cost today (F9):** `stats.totalCostUsdToday` is bound to the correct
  `/api/stats` source but only fetched by `refreshAll()` on connect / pull-to-
  refresh, and the WS `.poolStats` event carries no cost — so it stays stale at
  its connect-time value. Refresh it live (poll, or add cost to the WS stats).
- **Budget SPENT + per-agent cost:** the client per-agent `costUsd` sum is `$0`;
  the `.cost` live-event / snapshot-cost path web F6 added is not delivering on
  macOS. Mirror F6 in the Swift client (parse per-task cost from the snapshot /
  the cost event).
- What's already correct and must not regress: **Loop SPEND** (server
  `/api/loop`), the **cognition-grid "N active"**, and the **Tasks** list.
- Verify by repeating Verify-2 Phase 2/3 on an attended device.

## 2. Launch-1 — seamless start

Sequence after Fix-3. Nothing structural blocks it; the concurrency backbone
(web + native) is now proven on both the data level and the real screen.

## 3. macOS visual verification — the process that works

For any future on-device pass: keep the screen **unlocked and attended**, run
`caffeinate -dimsu` for the session, grant computer-use access to `ai.konjo.lopi`,
and capture with `ffmpeg -f avfoundation -i "1"` + `screencapture`. A locked
screen yields only the lock screen — do not fall back to a headless substitute.

## 4. Decisions already closed (do not re-litigate)

- macOS-visual parity is **confirmed on the real display** (Verify-2) — orb morph,
  concurrency, N-active, 12 sections. Only the stat-tile data path (Fix-3) is open.
- Bare-pane launch uses `paneSubmitPayload`; cross-pool stats come from the DB /
  local session map, never a single pool's counters (Fix-2, `LEDGER.md`).
- Orb-parity → compact per-pane orb everywhere; Dashboard kept as a macOS-native
  richer view (Polish-1, `LEDGER.md`).
