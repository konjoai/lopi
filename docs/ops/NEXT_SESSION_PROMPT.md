# Next Session — after Fix-3

Fix-3 (CHANGELOG `[0.3.4]`) ports Fix-2's web F3/F4 + F6 corrections to the
native macOS client — the single real defect Verify-2 surfaced. The macOS
Dashboard/Budget stat tiles now read the same corrected sources the web fix
established:

- **F10 (counts):** RUNNING / SUCCEEDED / QUEUED / FAILED count the live session
  map (`liveAgents`) through a new `FleetBucket` mapping — the Swift mirror of
  web's `dbStatusToUiStatus` — instead of the per-pool WS `.poolStats` event,
  which undercounts in multi-repo mode. The event supplies only uptime now.
- **F9 (cost today):** a 5 s background poll of `/api/stats` keeps COST TODAY
  live instead of frozen at its connect-time value; the snapshot no longer
  clobbers the polled cost to `$0` on reconnect.
- **F6 (Budget SPENT):** `applySnapshot` hydrates each freshly-seen task's cost
  from the snapshot's per-task `cost` field (added to the wire by Fix-2 but
  ignored by the Swift client), so already-finished tasks no longer read `$0`.

Build + unit tests are green (`xcodebuild` build/test; 4 new `StatsParityTests`).
The Phase-1 option chosen (session-map count vs. WS-carries-DB-counts) and its
rationale are in `LEDGER.md`.

## 1. ⚠️ Owed: live on-device re-verification of Fix-3

**Fix-3 landed from a sandboxed run — it was _not_ live-re-verified on-device.**
Per the standing split every prior round used (code fix in-sprint, live
confirmation as a follow-up), an attended pass is still owed before Fix-3 is
called closed. Do not mark it verified until this runs. Repeat **Verify-2
Phase 2/3** on an attended device:

- 2 running + N done across **multiple repos** (`sail --repos …`).
- Confirm the four tiles match reality, **live, not just at connect**: COST TODAY
  updates during a billed run; RUNNING / SUCCEEDED match the real cross-repo
  counts; Budget SPENT shows real matching spend.
- Regression check (must stay correct): Loop SPEND (`/api/loop`), cognition-grid
  "N active", Tasks list.

Process that works (Verify-2, proven): keep the screen **unlocked and attended**,
run `caffeinate -dimsu`, grant computer-use access to `ai.konjo.lopi`, capture
with `ffmpeg -f avfoundation -i "1"` + `screencapture`. A locked screen yields
only the lock screen — do not fall back to a headless substitute.

## 2. Launch-1 — seamless start [the next real work]

**Once Fix-3's live re-verification passes, there are no known open findings left
anywhere in the system** — the entire Verify-1 → Fix-2 → Verify-2 → Fix-3 audit
chain closes, and Launch-1 begins with a fully clean slate. Nothing structural
blocks it; the concurrency backbone (web + native) is proven on both the data
level and the real screen, and the stat/cost data paths now match across surfaces.

## 3. Decisions already closed (do not re-litigate)

- macOS-visual parity is **confirmed on the real display** (Verify-2) — orb morph,
  concurrency, N-active, 12 sections.
- Fix-3 counts the local `liveAgents` map for the fleet tiles and polls
  `/api/stats` for COST TODAY; the per-pool `.poolStats` event is uptime-only by
  contract (`LEDGER.md`). Future macOS stats consumers read the session map or
  `/api/stats`, never a single pool's counters.
- Bare-pane launch uses `paneSubmitPayload`; cross-pool stats come from the DB /
  local session map, never a single pool's counters (Fix-2, `LEDGER.md`).
- Orb-parity → compact per-pane orb everywhere; Dashboard kept as a macOS-native
  richer view (Polish-1, `LEDGER.md`).
