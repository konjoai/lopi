# KT-2.1 — Does the scorer really report a pass on an unrecognised stack?

**Sprint:** F2 · **Verdict:** CONFIRMED (fail — the defect reproduces as described)
**Baseline commit:** `5760da0` (`v0.27.1`, post-F0)
**File under test:** `crates/lopi-agent/src/scorer.rs`
**Fixed by:** F2 Phase 2 (this sprint) — see `LEDGER.md` entry for the one-way door.

## Method

A pytest-shaped repo wasn't necessary to reproduce the defect — any repo lacking
one of the scorer's two recognized manifests (`Cargo.toml`, `package.json`)
reaches the same fallback branch (`crates/lopi-agent/src/scorer.rs:103-107`,
pre-fix line numbers). Reproduced with a minimal Python-shaped repo:

1. `git init` a scratch repo, commit `app.py` containing `print('hi')`.
2. Modify `app.py` (append a function) — a real, non-docs, non-lockfile source
   change, so `should_skip_build_check` returns `false` and the scorer actually
   runs its detection logic instead of taking the "nothing changed" exit.
3. No `Cargo.toml` or `package.json` anywhere in the repo.
4. Call `Scorer::new(repo_path).score().await` directly (bypassing the full
   agent loop — this isolates the scorer's own detection/fallback behavior).

Automated as `crates/lopi-agent/src/scorer::tests::unrecognized_stack_no_longer_reports_a_perfect_pass`
(originally asserting the pre-fix behavior below; flipped to assert the
post-Phase-2 behavior once the fix landed in this same sprint).

## Pre-fix output (verbatim)

```
KT-2.1 raw score: Score { test_pass_rate: 1.0, lint_errors: 0, diff_lines: 1, errors: ["no test runner detected"] }
KT-2.1 passed(): true
```

`branch = claude/sprint-f2-correctness-fuh8tc`, run against baseline `5760da0`.

## Verdict

**Pass condition met — the defect reproduces exactly as described.** A repo
lopi cannot evaluate (no recognized test runner) receives:

- `test_pass_rate = 1.0` — a perfect score
- `Score::passed()` → `true` — the score that gates `finalize()`/PR creation
- The only signal that anything is wrong is a single string in `errors`,
  `"no test runner detected"`, which nothing downstream inspects to block
  finalize — it is logged, not enforced.

This is the same defect class as "the verifier's `return true`" (F1 Phase 4):
*"I could not evaluate this, so it passes."* Framed identically in `LEDGER.md`
per the brief's instruction, so kiban's K1 G-POLARITY kill-test can grep for
the pattern across both sprints' entries.

## Consumption note for kiban K1

This file **is** K1's must-FAIL fixture for G-POLARITY, not just a record. The
pre-fix output above — captured against a real commit (`5760da0`), not a
synthetic example — is the artifact K1 should assert its gate would have
caught, if run against this exact commit and this exact repro.
