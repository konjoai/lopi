# Cross-cutting decisions — settle these once, reuse everywhere

Four architectural decisions that, if made once and shared, keep A1→A2→A3→B1 coherent. If
made ad-hoc per phase, they fracture into four incompatible eval systems. These are the
"design the seams before the features" calls.

---

## 1. ONE goal/acceptance schema — shared by loops and stacks

**Decision:** a single `Acceptance` type in `lopi-core` (defined in `A1.md`), used verbatim
at loop scope (`Task.acceptance`) and stack scope (`StackConfig.goal`). The TS side mirrors
it once (`web/src/lib/api.ts` types), and the UI's existing `EvalRef{name,tier}` tags
(`stack.ts:24-27`) are the *authoring surface* that compiles into `Acceptance.checks`.

**Why:** B1 explicitly "reuses A1's goal schema at stack scope." If loop-goals and
stack-goals diverge, the eval executor needs two code paths and the UI needs two editors.
One schema means the same `TieredEvaluator` and the same `EvalsPopover` serve both.

**Consequence:** `EvalTier` (Rust) ≡ the UI `EvalTier` union — keep the four names identical
(`ExecutionOk/ShellTest/Judge/Suite` ↔ `base/test/judge/suite`).

## 2. ONE evaluator interface — all tiers + the judge are pluggable and testable

**Decision:** the `Evaluator` trait (defined in `A1.md`) with four impls
(`ExecutionOkEval`, `ShellTestEval`, `JudgeEval`, `SuiteEval`) behind a `TieredEvaluator`.
`JudgeEval` **wraps the existing `VerifierAgent`** (`verifier.rs`) — it does not reimplement
judging. `ShellTestEval` wraps `run_guard_command` (`loop_config.rs:241`); `ExecutionOkEval`
wraps `Scorer` (`scorer.rs`).

**Why:** the probe proved the judge is reliable *when it gets the signal* — so the value is
in composing tiers (cheap deterministic first, judge only for the rest), not in a monolithic
evaluator. A trait makes each tier unit-testable with a fake and lets A3's stochastic
re-sampling wrap any tier uniformly.

**Consequence:** the finalize verifier (`finalize.rs:75`) and the new `JudgeEval` share the
same `VerifierAgent` core; decide (A1 open-Q #1) whether finalize keeps its own verifier call
or delegates to the tier. Recommendation: one call site (`JudgeEval`), finalize consumes its
result.

## 3. ONE eval result, THREE consumers — design the result for all of them

**Decision:** `EvalOutcome { verdict, score, per_check, critique }` (defined in `A1.md`) is
the single object that feeds:
- **A2 reflection** reads `critique` (the flattened `fix_hints`/`gaps`) → next iteration's
  prompt.
- **A3 ratchet** reads `score` (the weighted scalar) → accept/reject vs best-so-far.
- **A3/B1 termination** reads `verdict` + the persisted score trajectory → no-progress /
  goal-met stop.

**Why:** the sprint brief calls this out explicitly — "one eval result, three consumers." If
A2 invents its own critique format, A3 its own score, and B1 its own pass flag, the same
attempt gets evaluated three times and the numbers disagree. Design the object once so all
three read the *same* evaluation.

**Consequence:** `EvalOutcome` must carry both a scalar `score` (for the ratchet) and
structured `per_check` (for reflection + per-tier termination). Persist the whole object, not
just a pass bit.

## 4. Score-history lives in memory (SQLite) — ratchet + no-progress + stack goal query it

**Decision:** persist `EvalOutcome` per attempt in a new `eval_outcomes` table (mirror
`verifier_verdicts`, `store/verifier.rs:38-82`), and add a progress query
(`score_trajectory` / `progress_state`, `A3.md`) built on `run_attempts`
(`run_trace.rs:88`). This is the single source of truth for "is this loop/stack improving."

**Why:** §5 found the raw score rows exist (`attempts.score_*`) but **no query** for
progress, and the agent-level no-progress streak is in-memory only — invisible to the
orchestrator and to stacks. A3's ratchet and B1's stack termination both need a durable,
queryable trajectory. `kohaku`/vector store does **not** exist (§5) — build on `MemoryStore`.

**Consequence:** the write happens at the loop's eval seam (A1) and/or the post-run seam
(`pool/run_loop.rs:473-495`); the read serves A3 (per-task) and B1 (per-stack, aggregating
its cards' outcomes).

---

## The dependency these four create

```
Acceptance schema (1) ──► Evaluator trait (2) ──► EvalOutcome (3) ──► score-history store (4)
        │                        │                      │                      │
     loops+stacks            all tiers            A2 / A3 / B1          ratchet + no-progress
```

Settle 1–4 in the A1 build (they are A1's real surface area); A2/A3/B1 then consume them
without re-litigating. Getting these wrong is how "evaluator-optimizer loops go circular" —
not because the judge can't judge (the probe says it can), but because three subsystems
disagree about what the evaluation *was*.

## Doc-drift to fix while here (from §2, §5)

- `KONJO_VERIFIER.md` says the verifier "calls Opus"; code resolves a model ≠ worker
  (`verifier.rs:39-45`). Update the doc to the stronger guarantee.
- `roadmap:38` "no evaluator running at all" is false — the verifier is built. Update.
- `roadmap:45,106` / `PLAN.md:435` "kohaku (episodic memory)" implies a vector store that
  does not exist. Either build it or stop citing it; A2's durable learning rides
  `lessons`/`patterns`, not kohaku.
- `lessons.rs:36-64` silently drops writes below score 0.6 — violates CLAUDE.md "no silent
  failures"; log or re-gate (A2).
- The verifier is fail-open (`verifier_runner.rs:21-23, 54-57`) — incompatible with an L3/L4
  "verified PR" guarantee; close in A1.
