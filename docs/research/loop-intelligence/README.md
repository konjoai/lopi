# Research-1 · De-risk & spec Track A + B

Output of the Research-1 sprint: turn the loop-intelligence roadmap's A1→A2→A3→B1 sketches
into buildable, de-risked specs — **design docs + a pre-registered kill-test register + a
go/no-go**, grounded in the actual code (every claim cites `file:line`). No feature code was
written; the only executable artifact is the throwaway evaluator-reliability probe, kept as
the evidence record, wired into nothing.

Reference roadmap: [`../../lopi-loop-intelligence-roadmap.md`](../../lopi-loop-intelligence-roadmap.md).

## Read in this order

1. **[`00-current-state.md`](00-current-state.md)** — the real code behind Track A/B, with the
   headline correction: **the separate-model judge is already built and wired** (the roadmap
   says it isn't).
2. **[`probe/`](probe/)** — the MASTER kill-test (run FIRST): can lopi's judge reliably tell
   good work from bad?
   - [`00-preregistration.md`](probe/00-preregistration.md) — thresholds fixed before running.
   - [`fixtures.json`](probe/fixtures.json) — 24 labelled cases.
   - [`results.json`](probe/results.json) / [`results.md`](probe/results.md) — **PASS** (24/24
     agreement, 0% flip), with the honest caveat.
3. **[`A1.md`](A1.md)** (keystone) → **[`A2.md`](A2.md)** → **[`A3.md`](A3.md)** →
   **[`B1.md`](B1.md)** — one design doc per roadmap phase (current state → prior art mapped
   onto lopi's assets → design + recommendation → integration points → build kill-test →
   biggest risk → open questions).
4. **[`cross-cutting.md`](cross-cutting.md)** — the four seams to settle once (one schema, one
   evaluator interface, one result, one score-history store).
5. **[`kill-test-register.md`](kill-test-register.md)** — every pre-registered threshold in one
   place (master result + per-phase build tests).
6. **[`build-plan-and-go-no-go.md`](build-plan-and-go-no-go.md)** — sequenced A1→B1 build plan
   and the **GO** decision on A1.

## The one-sentence finding

lopi's already-built, maker/checker-isolated, separate-model judge agreed with ground truth
on **24/24** cases and never flipped across 3 runs — so the roadmap's foundation is solid
enough to build A1 **now**; the residual risk moved from *"can the judge judge?"* (yes) to
*"does the judge get the whole artifact?"*, which is an A1 engineering problem, not a
research dead-end.
