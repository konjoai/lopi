# Next Session — deferred from Fix-1 (Ops-2 findings closure)

Fix-1 (PR for `claude/ops-2-findings-closure-*`, CHANGELOG `[0.3.1]`) closed the
concrete Ops-2 bugs. Two items were **deliberately not decided** there and are
flagged here so they don't get silently resolved either direction.

## 1. Orb parity — a real design decision, not yet made

`FEATURE_STATE.md` §D / `LIVE_UI_STATUS.md` "Orb parity finding":

- **Web** (`/stacks`): post-Unify-2, each stack card renders a compact WebGL
  **`OrbDot`** (a small per-card dot) — `web/src/lib/forge/cardOrb.ts` +
  `OrbDot.svelte`.
- **macOS** (Forge): still renders the prominent full-pane **Metal `ForgeOrb`**.

Same concept (`computeOrbState`, same color vocabulary), materially different
prominence. This is a **product/design call**, not a bug — do **not** quietly
converge one onto the other as a side effect of some other change. Resolve it in
a design-focused session that decides, on purpose, whether the two surfaces
should match and which direction wins (compact dot everywhere vs. prominent orb
everywhere vs. intentionally different by surface). Whatever is chosen, update
both `FEATURE_STATE.md` §D and this note.

## 2. Launch-1 — seamless start

Separately sequenced (unchanged by Fix-1). Not started here.

## Still-open finding NOT in Fix-1's scope

- **Cost/token accounting stuck at $0 (bug #3, [Med]).** After real billed runs,
  `/api/stats` reports `total_cost_usd_today: 0` / `total_tokens_today: 0` and
  per-task `cost` is `null`; the web `/budget` and macOS Forge/Budget/Loop cost
  surfaces all read from this, so the entire cost surface shows $0 regardless of
  real spend. Fix-1's finding set (bugs #1, #2, #4, #5, #6, #7, #8, #9 + the
  nav-gap) did not include #3 — it wants its own focused pass tracing where
  per-turn cost is (or isn't) accrued into `daily_token_totals` and the task
  `cost` column.
