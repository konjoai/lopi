# F4 Session Continuity — small real paired measurement (NOT the T01–T10 corpus)

**Sprint:** F4 · **Date:** 2026-07-27 · **n:** 8 paired trials (target was
10; the container restarted mid-run and killed the process after 8
completed — see below)

## What this is, and what it is NOT

**This is not the 30-run paired Wilcoxon over the T01–T10 corpus the
brief's own Phase 5 and merge gate ask for.** That corpus run has never
happened in this repo (`benchmarks/corpus/README.md`'s "Status (Sprint F0,
2026-07-26)" note is still accurate) and is now blocking F0's own Phase 3,
F1's Phases 5-6, and this sprint's own merge gate simultaneously — see
`NEXT_SESSION_PROMPT.md`.

What this *is*: a small (n=8), **real** (not synthetic) paired sample,
using the actual `claude` CLI on subscription auth (the same live access
used for all five `.konjo/killtests/F4/KT-4.*.md` kill-tests), directly
measuring the mechanism the sprint's hypothesis is about — cold-spawn vs.
resumed cost/cache-share across a `plan → implement` transition — on a
small scratch repo, not this repo's own corpus. This substitutes real CLI
calls for a corpus run the way F3's `event_bridge_bench.rs` substituted a
synthetic harness for a live four-agent measurement — same discipline
(documented, honest about scope), different substitution (real calls vs. a
synthetic harness, because the object under test here *is* the CLI's own
resume behavior — a synthetic stand-in would not answer the question).

## Method

- Scratch git repo (`CLAUDE.md`, a tiny buggy Rust file, a notes file
  describing the bug) with a worktree checked out from it — same worktree/
  no-`--bare`/subscription-auth conditions as KT-4.1.
- Each pair runs the identical two-call sequence twice:
  - **Cold:** independent, unresumed `plan` call, then an independent,
    unresumed `implement` call (given only the plan's text as its prompt —
    see the caveat below on why this makes cold cheaper than a real
    lopi cold-implement call would be).
  - **Resumed:** `plan` call under a fresh `--session-id`, then the
    `implement` call `--resume`s that same id.
  - All tool-mutation (`Write`/`Edit`/`MultiEdit`/`NotebookEdit`/`Bash`/
    `Task`) denied via `--disallowedTools` in both conditions — this
    harness measures token/cost/cache mechanics only, it never lets either
    condition actually modify anything.
  - `--permission-mode acceptEdits` (not `bypassPermissions` — see
    KT-4.1's root-container caveat), `CLAUDE_CODE_SESSION_ID`/
    `CLAUDE_CODE_CHILD_SESSION`/`CLAUDE_CODE_REMOTE_SESSION_ID` scrubbed
    from the child env per the same confound KT-4.1 found.
- Cost/usage read directly from each call's `--output-format json`
  envelope (`total_cost_usd`, `modelUsage`/`usage` token breakdown) — the
  same authoritative post-hoc fields `TurnMetrics` already trusts (F2's
  KT-2.4).
- Paired Wilcoxon signed-rank test, normal approximation with continuity
  correction (no scipy in this sandbox — `wilcoxon.py`, same shape as F3's
  own script).
- **n=8, not the planned 10:** the container this session runs in was
  restarted mid-batch; the 9th and 10th pairs' subprocess was killed along
  with it. The 8 that completed did so cleanly (no partial/corrupted rows)
  and are reported as-is, not padded or re-run to hit a rounder number.

## Results (n=8 paired trials)

| Metric | Cold median | Resumed median | Wilcoxon p (two-sided) | Effect size r |
|---|---|---|---|---|
| Cost per completed plan+implement pair (USD) | $0.1236 | $0.0758 | 0.0143 | 0.87 |
| `cache_read / (cache_read + cache_creation)` | 0.891 | 0.924 | 0.0143 | 0.87 |
| Wall-clock (plan+implement, seconds) | 25.4 | 18.1 | — (not gated) | — |

`W_pos=0, W_neg=36` for cost (all 8 pairs cheaper resumed); `W_pos=36,
W_neg=0` for cache ratio (all 8 pairs a higher cache-hit share resumed) —
**all 8 pairs moved the same direction on both metrics**, the strongest
possible signal at this sample size. Both conditions completed with
`is_error: false` in all 8/8 pairs — no resume-establishment failures
observed in this sample (the deliberate bad-`--resume` repro in KT-4.1's
own write-up is where that failure mode was actually exercised).

## Important caveat — raw tokens fell here, and that is a harness artifact, not the general finding

Raw total tokens (`cache_read + cache_creation`) were **lower** in the
resumed condition in this harness (median 86,187 vs. 114,200 cold) — this
looks like a token *reduction*, which contradicts this sprint's own stated
expectation ("raw tokens are expected to rise... if a summary claims fewer
tokens, either the measurement is wrong or the summary is").

The measurement is not wrong, but the harness's **cold-implement prompt is
not a fair stand-in for lopi's real cold-implement call**: this harness's
cold `implement` call receives only a short prompt (the plan's text,
truncated to 800 chars) with no repository re-exploration forced — it is
artificially cheap. Real lopi (`build_implement_prompt`, `claude_support.rs`)
sends a full TOON-encoded scope plus the complete plan text on every
implement call, cold or not, and a genuinely cold implementation attempt on
a real multi-file task typically re-explores the repo with real tool calls
(`Read`, `Grep`) that this trivial single-file, no-mutation-allowed harness
never triggers. **This measurement's cost and cache-ratio findings are the
meaningful, generalizable result; its raw-token direction is a property of
this specific minimal harness and should not be read as evidence that a
real deployment's raw tokens will fall.** Treat "cost per completed task
fell while cache-read share rose" as the honest summary of this run — not
"tokens fell," which this sprint's own anti-goal specifically warns
against claiming.

## Bearing on the sprint's merge gate

**This measurement does not, by itself, satisfy the brief's merge
criterion** ("cost per completed task down with a meaningful effect, and
pass rate not down" — measured on the 30-run T01–T10 corpus). It is
directionally consistent with the sprint's hypothesis at the mechanism
level (cost down, cache-read share up, same direction in all 8/8 pairs,
large effect size) and gives real, non-synthetic evidence that the
underlying lever works the way KT-4.1/4.3/4.4 found it should. Whoever
runs the actual T01–T10 corpus should treat this as a strong prior to
check against, not a substitute for it — see `NEXT_SESSION_PROMPT.md`.
