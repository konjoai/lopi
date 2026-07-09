# A2 · Pre-registration — reflection must beat blind retry (write-before-code)

This document is committed **before** the A2 implementation so the success
margin and go/no-go cannot be moved after seeing results (the §2 discipline).

## Pre-flight map (file:line)

- **Within-run critique routing (the thing A2 extends, not rebuilds):**
  - A1 eval critique → next attempt: `crates/lopi-agent/src/runner/eval_runner.rs:69-73`
    (`outcome.critique` → `self.task.constraints`).
  - Verifier fix-hints → next attempt: `crates/lopi-agent/src/runner/verifier_runner.rs:78-82`.
  - Adaptive-retry error framing: `crates/lopi-agent/src/runner/run_loop.rs:436-452`
    (`last_error` via `SelfPromptStrategy`).
  - Read back into the prompt: `crates/lopi-agent/src/runner/seed.rs`
    (`gather_seed`) → `claude.rs:213` / `prompt.rs:21`.
- **kohaku / episodic memory API** (kohaku is a docs-only name; the substrate is
  `lopi-memory`): `lessons` (`store/lessons.rs:36` `save_lesson` — **silently
  gated at score ≥ 0.6**, `load_lessons:70`), `patterns`
  (`store/patterns.rs:80` `find_similar_patterns`, `:139` `insert_postmortem_pattern`),
  `eval_outcomes` (`store/eval_outcomes.rs:52/84/101`).
- **Pattern mining API:** `store/patterns.rs` (`find_similar_patterns`,
  `mine_patterns`, `keyword_fingerprint`, `jaccard_similarity`) and the
  recurring-failure post-mortem `runner/postmortem.rs`.
- **Rollback timing (capture must precede this):**
  - Finalize / acceptance / verifier reject: `runner/finalize.rs:83`
    (`git.hard_rollback()`), reached from `eval_runner.rs:74` returning `false`.
  - Non-gaining iteration: `runner/run_loop.rs:467`
    (`abort_and_mark_retrying` → `abort_attempt` → `git.hard_rollback()`).
- **Context assembly (injection point):** `runner/seed.rs:25-56` (`gather_seed`
  → `PlanningSeed`), which already loads `load_lessons`.

## Fixed task set

20 fixture tasks (`REFLECTION_FIXTURES`), each a lopi-style retryable task that
fails on the first blind attempt: a candidate-fix pool of size `n` with exactly
one root-cause fix. `learning_relevant` flags whether a durable learning for that
task's failure mode exists in memory (not all tasks — pre-registered mix).

## Three arms

- **blind** — retry, no critique carried; each attempt samples the candidate pool
  uniformly (models today's no-reflection retry).
- **within_run** — a failed attempt's critique eliminates the tried candidate for
  the rest of the run (models today's `constraints` routing — sampling without
  replacement).
- **cross_run** — a relevance-filtered durable learning, when present and
  retrieval hits, points attempt 1 at the root-cause fix; on a retrieval miss it
  degrades to within-run behaviour and pays a context-bloat penalty (models A2's
  bounded injection).

## Pre-registered success margin (fixed before running)

Cross-run reflection ships **on-by-default** only if, on the fixed set, it beats
**blind** retry by **≥ 15 percentage points** of pass-rate **and** does not raise
mean iterations-to-pass. (Aligns with the margin already pre-registered in
`A2.md`.) Otherwise it ships **behind a flag, off by default**, and the number is
recorded as-is — including a null or negative result.

## Pre-registered harness parameters (the knobs, fixed here)

- `retrieval_precision` — P(a retrieved learning points at the true root cause).
  Baseline **0.8**. Also swept over {0.2 … 1.0} to expose the failure mode.
- `bloat_penalty` — P(an imprecise injection wastes the attempt via context
  bloat). Baseline **0.5**.
- `max_attempts` per task: **4**. Seed: fixed per fixture id (deterministic).

## Honesty caveat (pre-registered)

The committed harness is a **deterministic mechanism simulation**, not a live LLM
benchmark. It exercises the real retrieval/dedup/cap pipeline
(`find_relevant_learnings`) and a documented pass/fail model whose parameters are
fixed above. It validates the plumbing, guards against regressions, and — via the
precision sweep — shows *when reflection helps vs. hurts*. The live three-arm run
on real tasks scored by A1's executor requires an API-enabled environment and is
the true ship gate. Because that live run was **not executed in this
environment**, cross-run reflection lands **off-by-default behind a flag** per §2
discipline, regardless of the simulation's (favourable-at-high-precision) numbers.
A simulated lift is evidence the *mechanism* can help when retrieval is precise —
it is **not** evidence the live feature beats blind retry. Flipping the default on
requires the live numbers to clear the margin above.
