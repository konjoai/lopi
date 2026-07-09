# Master kill-test — RESULT (the highest-value hour in the sprint)

**Verdict: PASS on all four pre-registered thresholds — with one honest caveat that
shapes A1's tier boundaries. The evaluator is viable to build on. Proceed to A1.**

The probe ran the actual design lopi ships — a *separate* Claude model, maker/checker
isolated, given only artifact + goal + rubric + test output, returning lopi's real
`{passed, gaps, fix_hints, confidence}` schema (`crates/lopi-agent/src/verifier.rs:13`) —
over 24 hand-labelled lopi-style cases, 3 independent runs. Thresholds were fixed in
`00-preregistration.md` before any run.

## Scorecard (see `results.json` for raw per-case verdicts)

| Metric | Result | Threshold | Verdict |
|---|---|---|---|
| Agreement — machine-checkable (12) | **12/12 = 100%** | ≥ 90% | ✅ PASS |
| Agreement — judgment (12) | **12/12 = 100%** | ≥ 75% | ✅ PASS |
| Verdict stability across 3 reruns | **24/24 identical, 0% flip** | ≥ 90% | ✅ PASS |
| Adversarial catch rate (7 gamed cases) | **7/7 = 100%** | ≥ 80% | ✅ PASS |

Every one of the 24 verdicts matched ground truth, on all 3 runs — including all seven
adversarial cases (deleted assertions, `assert!(true)`, blanket `#![allow(warnings)]`,
hardcoded benchmark, cherry-picked best-of-5, tautological tests, max-of-samples instead of
Wilcoxon). The judge's `gaps` text named the exact failure mode each time (e.g. JG-06:
"single cherry-picked best-of-5 run while ignoring 4 lower runs"; MC-05: "suppresses
instead of fixing warnings"). Zero false-positives (good work never failed) and zero
false-negatives (bad work never passed) on this set.

## The honest caveat — what a PASS here does and does not prove

This is a real, informative pass, but it is bounded. State it plainly so A1 is built on the
true result, not a flattering one:

1. **The fixtures carry the signal in the artifact.** Each adversarial case leaves a
   *tell* the judge can see: the comment that admits cherry-picking, the visible
   `assert!(true)`, the literal `return 142.0`. The probe therefore proves the judge
   reliably catches gaming **when the gaming is visible in the diff + test output it is
   handed**. It does **not** prove the judge catches gaming that is hidden from its inputs —
   a benchmark rigged via an untouched config file, a subtle correctness bug with no visible
   tell, or a change outside the **6 000-char diff excerpt** the real verifier truncates to
   (`verifier.rs:188`). That is the judge's true ceiling and it is an input-completeness
   problem, not a judgment problem.
2. **N = 24, hand-authored.** Wide confidence intervals per bucket; this is a go/no-go
   signal, not a calibrated accuracy figure. A "clean" 100% means the cases were within the
   judge's competence, not that the judge is infallible on live traffic. Live calibration is
   an A1 build task (§ below).
3. **Same model class across the 3 stability runs.** 0% flip demonstrates the judge is
   *deterministic enough* to gate a ratchet — the property A3 needs. It does not measure
   cross-model agreement (Opus vs Sonnet as judge); an ensemble check is cheap insurance,
   flagged for A1.
4. **Proxy for the wire path.** The judge here is the Agent-tool Claude, not the production
   `AnthropicClient.complete()` call. Same design, same prompt shape, same schema — but the
   exact API path should be smoke-tested once in A1.

## How this result sets the A1 tier boundaries (the probe's real payload)

The failure modes it *didn't* have to work hard on, and the one it structurally can't see,
draw the tier lines directly:

- **Tier 0/1 (execution-ok + shell test) stays the cheap floor.** Cases MC-02, MC-06,
  MC-08(partial), MC-12, JG-09 were decidable from `test result: FAILED` alone — no judge
  needed. Anything a shell/test gate can decide **must** be decided there; the judge is not
  spent on objectively-checkable facts. This is both cheaper and removes the judge's
  input-completeness risk for those cases.
- **Tier 2 (judge) is viable and reliable for the subtle cases** (JG-01, JG-02, JG-04,
  JG-06, JG-07, JG-11) that no shell check catches — exactly where the probe scored 100%
  and stayed stable. Build the judge tier.
- **Tier boundary rule from caveat #1:** the judge is only as good as the artifact it sees.
  So A1 must (a) hand the judge the *complete* signal (full diff or file-scoped diff, not a
  6 KB prefix; the actual bench/metric output, not a summary), and (b) prefer making a
  criterion machine-checkable over asking the judge, whenever it can be. The **goal/
  acceptance object** is what makes that choice explicit per-criterion.
- **Ensemble/adversarial judging is optional, not required, for launch.** 0% flip means a
  single judge is stable enough for the ratchet. Reserve N-of-M ensemble for high-stakes
  (L4 auto-merge, stack-goal acceptance), not every iteration — a cost decision, not a
  reliability one.

## Bottom line

The roadmap's load-bearing fear — "evaluator-optimizer loops go circular when the judge
can't tell good from bad" — is **not** where lopi dies, on the evidence. lopi's existing
judge design tells good from bad reliably and stably on cases where the signal is in the
artifact. The remaining risk moved from *"can the judge judge?"* (answered: yes) to
*"does the judge get the whole artifact, and is each criterion checked at the cheapest tier
that can decide it?"* — which is an **A1 engineering problem**, not a research dead-end.
**Go for A1.**

> Per §5 of the sprint brief, this probe is a measurement, not a shipped feature. The
> fixtures and judge harness live under `docs/research/loop-intelligence/probe/` as the
> evidence record and are not wired into any build path. Do not let them grow into a
> half-built evaluator.
