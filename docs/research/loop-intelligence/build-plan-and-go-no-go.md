# Sequenced build plan + Go/No-Go on A1

Gated on the master kill-test (`probe/results.md`: **PASS**). This is the order to build
A1→A2→A3→B1, each with a rough size and its entry kill-test. Track C stays held (§5 of the
brief).

---

## GO / NO-GO on building A1

**Verdict: GO.**

The honest version of "is the evaluator reliable enough to build on yet": **yes, with a
scoped caveat that A1 must engineer around, not wish away.**

- lopi's judge design (separate model, maker/checker isolated, rubric-graded) scored 100%
  agreement with ground truth and 0% verdict flip across 3 runs on 24 cases, including all 7
  adversarial gaming patterns (`probe/results.md`). The roadmap's load-bearing fear —
  evaluator-optimizer loops going circular because the judge can't tell good from bad — is
  **not** where lopi dies on this evidence.
- **And** the judge is *already built and wired* (`00-current-state.md §2`), so A1 is
  promotion + generalization, not greenfield — the riskiest component already exists and
  works.
- **The caveat that survives:** the judge is only as reliable as the artifact it is handed
  (probe caveat #1) — it catches gaming that is *visible* in its inputs, not gaming hidden
  from them (untouched config, out-of-excerpt diff, tell-free subtle bugs). A1's job is
  therefore to (a) hand the judge the complete signal (full file-scoped diff + raw metric
  readings, not a 6 KB prefix), and (b) push every criterion that *can* be machine-checkable
  to a cheap deterministic tier, spending the judge only where nothing cheaper can decide.
  This is engineering, not research risk — hence GO.

**What would have been a NO-GO:** if M1–M4 had missed, A1's spec would have flipped to "how
to make evaluation reliable" (better acceptance specs, more machine-checkable gates,
ensemble/adversarial judging) and A2/A3/B1 would be blocked. They are not blocked.

---

## Build order (each phase entry-gated by its kill-test in the register)

### Phase A1 — eval executor + goal/acceptance object · size **M** · the keystone
- **Entry kill-test:** A1.1–A1.4 (`kill-test-register.md`).
- **Builds:** `Acceptance` schema (cross-cutting #1), `Evaluator` trait + 4 tier impls +
  `TieredEvaluator` (#2), `EvalOutcome` (#3), `eval_outcomes` persistence + progress query
  scaffold (#4), the `run_loop.rs:363` hook, UI `EvalRef`→`Acceptance` wiring, and the
  fail-open fix. Reuses `VerifierAgent`, `run_guard_command`, `Scorer`.
- **Unblocks:** everything. The four cross-cutting seams are A1's real surface area — get
  them right here and A2/A3/B1 just consume them.

### Phase A2 — reflection routing · size **S–M** · depends on A1
- **Entry kill-test:** A2.1 (visibility) then A2.2 (measured ≥15pp lift).
- **Builds:** route `EvalOutcome.critique` through the *existing* self-prompt strategy
  (`run_loop.rs:440`); surface the per-task critique trail + reflection lessons in
  `gather_seed`; write a reflection lesson at the post-run seam; un-silence the `lessons`
  0.6 gate.
- **Note:** mostly productionizing existing skeletons (self-prompt strategies, fix-hint
  routing). A2.2 is a *measurement* — if the lift isn't there, that's the finding.

### Phase A3 — ratchet + termination + budget · size **M** · depends on A1, ideally A2
- **Entry kill-test:** A3.1–A3.4.
- **Builds, in sub-order:** (a) persist score history + `progress_state` query (unblocks the
  rest), (b) beats-best ratchet on deterministic checks, (c) budget meter + `BudgetExceeded`,
  (d) stochastic re-sampling gate reusing `benchmarks/` stats.
- **Note:** no-progress detection already exists (`finalize.rs:238`) — A3 persists it and
  adds the *reject-regression* half + budget.

### Phase B1 — goal-directed stacks · size **M** · depends on A1 + A3
- **Entry kill-test:** B1.1–B1.3.
- **Builds:** `StackConfig.goal: Acceptance` (reuses A1 schema); sequencer termination becomes
  goal-then-count with `loopTarget` as ceiling; stop reason in `StackRunState`; goal
  evaluated client-side from cards' persisted `EvalOutcome`s (option A). Depends on the
  shipped stack-control dock (PR #68).

---

## Dependency spine (mirrors the roadmap, corrected for what's already built)

```
B1  goal-directed stacks        ← reuses A1 schema + A3 progress query
A3  ratchet + no-progress(persist) + budget   ← reuses finalize.rs:238 + benchmarks stats
A2  reflection (eval critique → prompt)        ← reuses self_prompt.rs + verifier_runner routing
A1  eval executor + Acceptance + 4 cross-cutting seams   ← reuses VerifierAgent (BUILT) + Scorer + run_guard_command
────────────────────────────────────────────────────────
NOW  judge BUILT & reliable (probe PASS) · scorer · until · no-progress streak · self-prompt · trust ladder
```

## The through-line (honest answer to the sprint's real question)

**Can lopi reliably judge its own work?** On the evidence: **yes** — its already-built,
maker/checker-isolated, separate-model judge agreed with ground truth 24/24 and never
flipped. The foundation the roadmap rests on is solid enough to build A1 on **now**. The one
place it can still be fooled — gaming hidden from the judge's inputs — is a bounded
engineering problem A1 addresses by feeding the judge the complete signal and pushing
objective criteria to cheaper deterministic tiers. Build A1 as the four cross-cutting seams
(one schema, one evaluator interface, one result, one score-history store); everything above
the line consumes them. Hold Track C until A/B are proven on real work, exactly as the
roadmap reserves.
