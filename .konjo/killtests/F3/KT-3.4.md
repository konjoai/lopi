# KT-3.4 — Does F1 change the volume? (design input, not a blocker)

**Sprint:** F3 · **Type:** design input, not pass/fail

## Method

Checked whether Sprint F1 has landed: `grep -n "Sprint F1" CHANGELOG.md LEDGER.md`
returns nothing. Only F0 and F2 have shipped as of this sprint (`v0.28.0`).
F1 has not landed, so this kill-test cannot measure post-F1 volume directly —
per the brief, it estimates from F1's design instead.

## Estimate

F1's brief (as described in this sprint's own text, since F1's own brief is
not yet in this repo) adds a verifier session per finalize attempt where
there is currently none, streaming through the same `EventBus<AgentEvent>`.
Qualitatively:

- Every `finalize()` call that currently produces zero extra agent-stream
  traffic will, post-F1, produce one additional bounded verification pass —
  its own `LogLine`/`StatusChanged`/`ScoreUpdated` events on the same bus.
- This is a *per-finalize-attempt* addition, not a per-turn multiplier: a
  task that retries N times before finalizing still only pays the verifier
  cost once (at the finalize edge), not N times.
- Expected shape: a modest, bounded increase in aggregate events-per-task
  (order: one additional short agent run's worth of log lines), not a
  multiplicative blowup. It does not change the *shape* of the coupling
  this sprint fixes — it changes the volume the fix is measured against.

No F1 source exists yet in this repo to measure directly, so this is a
qualitative estimate, not a benchmark number — recorded as such rather than
inventing a precise figure.

## Consequence for F3

**F3's fix is valid either way** (KT-3.1 already reproduces the coupling's
cost without any F1 traffic). **F3's baseline is pre-F1** — this sprint's
30-run paired measurement (`benchmarks/results/`) describes a system that
does not yet include F1's verifier-session traffic. Recorded explicitly in
`CHANGELOG.md` and handed off in `NEXT_SESSION_PROMPT.md`: whoever lands F1
should re-run `KT-3.1`'s harness (`crates/lopi-ui/src/web/event_bridge_bench.rs`)
rather than reuse this sprint's numbers, per the brief's own instruction that
"a baseline measured against a system that has since changed is not a
baseline."

## Verdict

**DEFERRED to measurement — not a blocker.** Estimate recorded above;
authoritative post-F1 numbers require F1 to land first.
