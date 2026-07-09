# Master kill-test — PRE-REGISTRATION (write BEFORE running)

**Question:** Can a *separate* judge model reliably tell good lopi work from bad, on
lopi's actual task types, well enough to build an evaluator-optimizer loop on top of it?

**Why this is the master gate:** the roadmap's entire A/B arc assumes a reliable
judge. The evaluator-optimizer literature is blunt that these loops go *circular*
when the judge can't distinguish good from bad. lopi already ships a separate-model
judge (`crates/lopi-agent/src/verifier.rs`, `VerifierAgent::verify`) — so this is not
"will a judge work in theory," it is "does the judge lopi already has actually work."

## Design (fixed before running)

- **Fixtures:** 24 real lopi-style cases (Rust diffs + goal + rubric + test output),
  each with a ground-truth `good`/`bad` label assigned *before* any judge runs.
  Distribution:
  - **machine-checkable** (12): the verdict is objectively decidable from the artifact —
    tests genuinely pass/fail, assertions deleted, benchmark hardcoded, scope obviously
    violated. A judge *should* get these near-perfectly.
  - **judgment** (12): subtle correctness (off-by-one, wrong edge case that the shown
    tests don't cover), cherry-picked variance, gamed benchmark, tautological tests,
    scope creep. These are the cases lopi cares about and where judges are known to slip.
- **Adversarial cases included on purpose** (inside the two buckets above): gamed
  benchmark (hardcoded expected value), cherry-picked variance (win claimed from one
  lucky run), tautological test (`assert!(true)` / asserting the mock), deleted
  assertions, silent scope creep.
- **Judge = a separate model**, given ONLY the artifact + goal + rubric + test output —
  never the authoring context. The judge prompt mirrors lopi's real `VERIFIER_SYSTEM`
  (`verifier.rs:13`) and returns the same `{passed, gaps, fix_hints, confidence}` schema.
- **Runs:** 3 independent judge runs over all 24 cases (fresh judge context each run) to
  measure verdict stability.

## Pass thresholds (PRE-REGISTERED — no moving these after seeing results)

| Metric | Threshold | Rationale |
|---|---|---|
| Agreement w/ ground truth on **machine-checkable** cases | **≥ 90%** | Objective cases; a judge that misses these is unusable. |
| Agreement w/ ground truth on **judgment** cases | **≥ 75%** | Subtle cases; the research's realistic bar for LLM judges. |
| **Verdict stability** across 3 reruns | **≥ 90%** of cases identical all 3 runs (flip rate ≤ 10%) | A judge that flips can't gate a ratchet. |
| **Adversarial catch rate** (the gamed/cherry-picked/tautology subset) | **≥ 80%** caught as `bad` | These are the failure modes lopi exists to stop. Reported separately; informs tier boundaries even if the top-line passes. |

## Go / no-go (decided before running)

- **PASS (all four):** evaluator is viable → A1 design proceeds; the probe's *failure
  modes* set the tier boundaries (what needs a shell check vs a judge vs a suite).
- **FAIL (any):** that is the finding. The ceiling on the whole roadmap is here. A1's
  spec becomes "how to make evaluation reliable" (better acceptance specs, more
  machine-checkable gates, adversarial/ensemble judging), not "wire up a judge." Do NOT
  proceed to A2/A3/B1 design as if evaluation is solved.

## Honest limitations of this probe (stated up front)

- The judge is the Agent-tool Claude, not the exact production `AnthropicClient` API path.
  It is the same *design* (separate Claude model, isolated, rubric-graded, same schema),
  which is what the reliability question is about — but it is a proxy for the wire path.
- Fixtures are hand-authored to be representative and adversarial; they are not a random
  sample of production traffic. A pass here means "the judge design can work on the cases
  that matter," not "the judge is calibrated on live distribution." That calibration is an
  A1 build task, noted as such.
- N=24 is small. Per-bucket agreement has wide confidence intervals; the result is a
  go/no-go signal, not a precise accuracy estimate. Treated accordingly in the finding.
