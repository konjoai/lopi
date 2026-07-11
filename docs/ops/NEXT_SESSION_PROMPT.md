# Next Session — after Fix-2

Fix-2 (CHANGELOG `[0.3.3]`) closed every **code** finding from Verify-1: the
bare-pane launch is wired (F2), cost surfaces read real spend (F6), the stat
counters are correct across repos (F3/F4), a partial `--config` warns (F1),
id-scoped reads 404 on a bogus id (F8), and the cut-feature pricing copy is gone
(F7). Each was re-verified live on-device through the actual UI. Concurrency was
never re-opened — Verify-1 proved it clean.

Two things remain before and around Launch-1.

## 1. macOS visual verification — STILL OPEN, and no code fix can close it

**This is procedural, not a bug.** Verify-1 and Fix-2 both ran with the MacBook
**locked** (unattended), so every macOS-*visual* claim is still unverified:

- Compact-orb `matchedGeometryEffect` idle→live morph animates cleanly; the
  multipane grid reads right on macOS.
- macOS Dashboard "COST TODAY", Budget "SPENT", Loop "SPEND" show real spend
  (web analogues are now fixed in F6 — confirm macOS reads the same corrected
  source and not a stale client tally).
- macOS Dashboard "N active" against a seeded mixed batch (web "N live" is fixed
  in F3/F4 — confirm the native app shares the corrected behavior).
- The 12 `NavSection`s in a guided pass (no macOS UI-test target exists).

**The next live-verification pass must run on an attended, unlocked machine**, or
macOS stays unverified indefinitely — flagging this plainly so it doesn't quietly
drop for a third round. Nothing in a headless/locked run can substitute for it.

## 2. Launch-1 — seamless start [the next real work]

With F2 fixed, the single-prompt path works end-to-end, so nothing structural
blocks Launch-1. Sequence it after (or alongside) the attended macOS pass above.

## 3. Decisions already closed (do not re-litigate)

- Bare-pane launch uses `paneSubmitPayload` (no stack-loop semantics), not the
  dock's `runStack` — a bare prompt stays a bare prompt (Fix-2, `LEDGER.md`).
- Cross-pool stats come from the DB / the local agents map, never a single
  pool's in-memory counters (Fix-2, `LEDGER.md`).
- `stream` on a *malformed* (non-uuid) id is a 400, not a 404 — a client error
  distinct from a well-formed-but-unknown id (Fix-2 F8, documented in the test).
- Orb-parity → compact per-pane orb everywhere; Dashboard kept as a macOS-native
  richer view; server cost/token accrual persisted from `runner/stream.rs`
  (Polish-1, `LEDGER.md`).
