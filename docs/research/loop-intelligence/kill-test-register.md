# Kill-test register — every pre-registered threshold in one place

All thresholds were written **before** running (master) or before building (per-phase). This
register is the contract: no moving a goalpost after seeing a result. The master result is
recorded with its data; per-phase results fill in as each phase is built.

---

## MASTER · Evaluator reliability — **RUN, RESULT: PASS**

Pre-registration: `probe/00-preregistration.md`. Raw data: `probe/results.json`. Analysis:
`probe/results.md`.

| # | Metric | Threshold (pre-registered) | Result | Verdict |
|---|---|---|---|---|
| M1 | Judge agreement — machine-checkable cases | ≥ 90% | 12/12 = **100%** | ✅ PASS |
| M2 | Judge agreement — judgment cases | ≥ 75% | 12/12 = **100%** | ✅ PASS |
| M3 | Verdict stability across 3 reruns | ≥ 90% identical (flip ≤ 10%) | 24/24 = **100%**, 0% flip | ✅ PASS |
| M4 | Adversarial catch rate | ≥ 80% caught | 7/7 = **100%** | ✅ PASS |

**Data:** 24 hand-labelled lopi-style cases, judge = separate Sonnet-class model, isolated,
prompt mirrors `verifier.rs:13`, 3 independent runs. Every verdict matched ground truth on
every run. **Bounded by:** signal-in-artifact caveat (probe caveat #1) — proves the judge
catches *visible* gaming, not gaming hidden from its inputs. N=24 → go/no-go signal, not a
calibrated accuracy. See `results.md`.

**Consequence:** GO for A1. The failure the roadmap feared (circular loops from an
unreliable judge) is not lopi's ceiling. The residual risk is input-completeness → an A1
engineering problem, not a research dead-end.

---

## R-A1 · Eval execution + goal condition — *build kill-test (pre-registered, not yet run)*

| # | Assertion | Threshold |
|---|---|---|
| A1.1 | Loop with a failing `Acceptance` check scores FAIL; passing check scores PASS | 100% on 6-case fixture (2 pass / 2 fail) |
| A1.2 | Judge-tier verdict produced by a model ≠ author model | assert model id ≠ worker |
| A1.3 | Verdict stable across 3 reruns on the fixture | 0 flips |
| A1.4 | Required judge check whose API errors → Error→**blocked** (fail-closed), not passed | 1 error-blocked + 1 wrong-model-rejected case correct |

## R-A2 · Reflection / feedback — *build kill-test (pre-registered)*

| # | Assertion | Threshold |
|---|---|---|
| A2.1 | Iteration 2's prompt provably contains iteration 1's evaluator critique | binary (string contains `fix_hints`) |
| A2.2 | Reflect-and-retry beats blind-retry, paired, on N ≥ 15 first-fail tasks | ≥ **15 pp** higher follow-up pass rate, distinguishable on a paired sign test |

## R-A3 · Progress-gating + termination + budget — *build kill-test (pre-registered)*

| # | Assertion | Threshold |
|---|---|---|
| A3.1 | Attempt with eval score < best is rejected (not promoted) | binary |
| A3.2 | Plateau (flat within EPSILON for `no_progress_limit`) halts before `max_iterations` | binary; iterations used < max |
| A3.3 | Over-budget run stops with `BudgetExceeded` at the crossing attempt | binary; terminal status + index |
| A3.4 | Stochastic metric passing 1/5 samples but not the paired median is rejected | binary (the JG-06 case) |

## R-B1 · Goal-directed stacks — *build kill-test (pre-registered)*

| # | Assertion | Threshold |
|---|---|---|
| B1.1 | Stack goal "metric ≥ X" loops past pass 1 when unmet; stops `done` on the pass X is hit | binary; terminal phase + reason |
| B1.2 | Plateau below X for K passes → stops `stopped(no-progress)`, reason recorded | binary; `stopReason` set |
| B1.3 | Neither goal nor no-progress → stops at `loopTarget` ceiling, reason `ceiling` | binary; `stopReason` set |

---

### Register discipline

- Each build kill-test **entry-gates** its phase: the phase is not "done" until its row is
  green, measured, and recorded here next to the threshold.
- A2.2 and A3.4 are *measured* tests, not binary wiring checks — they can fail even when the
  code works, and that failure is a finding (the feature doesn't earn its place), not a bug
  to paper over.
- If any build kill-test fails, stop and report the honest number before proceeding to the
  next phase — same rule as the master.
