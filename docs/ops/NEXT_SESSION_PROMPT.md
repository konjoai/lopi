# Next Session — after Polish-1

Polish-1 (CHANGELOG `[0.3.2]`) closed bug #3 (cost/token accrual), swept the
remaining cut-feature remnants, and **resolved the two decisions Fix-1 left
open**: orb-parity and the Dashboard question. What's left is one real piece of
work (Launch-1) plus a short list of macOS on-device confirmations that a Linux
CI run structurally cannot do.

## 1. Launch-1 — seamless start [the next real work]

Separately sequenced, not started. This is the next substantive sprint once
Polish-1 lands. Nothing structural blocks it.

## 2. macOS on-device confirmations (verification, not new work)

This repo authors macOS on Linux and builds/verifies on the M3. A few Polish-1
changes touched macOS and need a visual/behavioral confirmation on-device — none
are open decisions, just "confirm it looks/behaves right":

- **Compact orb (Phase 4).** The live-pane orb was reduced to a compact status
  size (`AgentPaneView.cornerSize`, now 22–40pt) to match web's per-card
  `OrbDot`. Confirm the multipane grid reads as intended and the
  idle-launcher→live morph (`matchedGeometryEffect`) still animates cleanly.
  Then update `FEATURE_STATE.md` §D (currently records the divergence as open).
- **Cost surfaces (Phase 0).** With `turn_metrics` now persisted on the CLI
  path, run a few real billed sessions and confirm macOS Dashboard "COST TODAY",
  Budget "SPENT", and Loop "SPEND" show real non-zero spend (were `$0` / the
  `$-0.00` artifact), matching `/api/stats`.
- **"N active" count.** Ops-2 saw the Dashboard cognition-grid header read "N
  active" for terminal tasks. The `.active` flag *is* cleared on terminal WS
  events (`AppModel+Live.swift`), so the likely trigger is hydration of tasks
  observed mid-session; confirm against a real seeded batch and, if it recurs,
  trace whether REST-hydrated history rows should seed `active=false`.

## 3. Decisions now closed (do not re-litigate)

- **Orb-parity → compact per-pane orb everywhere.** Resolved in Polish-1 (see
  `LEDGER.md`). Not "intentionally different by surface."
- **Dashboard → kept as a macOS-native richer view.** Overview is web-only and
  can't absorb it for native users (see `LEDGER.md`).
- **Bug #3 → fixed** by persisting `turn_metrics` on the CLI path.
