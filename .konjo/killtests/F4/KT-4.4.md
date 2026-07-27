# KT-4.4 — Where is the cache TTL boundary?

**Verdict: PASS for the delay range actually measured (up to ~150s), with a
real mechanism finding that extends confidence well beyond that range but
does not itself confirm the full range live.**

## What was tested

Three chained calls in the same resumed session, same model throughout
(`claude-haiku-4-5-20251001`, held constant to isolate the delay variable
from KT-4.3's model-switch confound):

1. Cold spawn — establishes the session, reads two files.
2. `--resume`d immediately after (near-zero delay, model held constant this
   time, unlike KT-4.3's deliberate model switch) — near-total cache hit
   (`cache_read=26944`, `cache_creation=183`).
3. `--resume`d again after a **real 150-second wall-clock delay**
   (`sleep 150` between the two calls, not simulated) — `cache_read=27127`,
   `cache_creation=111`. The ratio **did not collapse**: 27127/(27127+111)
   ≈ 99.6% cache hit, statistically indistinguishable from the near-zero-
   delay case.

## The mechanism finding

The CLI's own `usage.cache_creation` envelope breaks cache writes into two
buckets: `ephemeral_5m_input_tokens` and `ephemeral_1h_input_tokens`. In
**every call across every kill-test in this sprint** (KT-4.1 through
KT-4.4, and the Phase 5 benchmark), 100% of cache-creation tokens landed in
`ephemeral_1h_input_tokens` and **zero** landed in
`ephemeral_5m_input_tokens`. This means the `claude` CLI defaults to
Anthropic's **1-hour** prompt-cache tier, not the 5-minute tier — a
directly checkable, mechanism-level fact, not an inference from behavior.

This changes the shape of the risk the brief's framing worried about. The
brief's concern was a `test phase` gap of "minutes apart, cold worktree
build" landing outside the cache window. If the window is actually ~1 hour
(not ~5 minutes), a multi-minute `cargo test` gap is comfortably inside it
for any realistically-sized Rust crate's test suite in this repo (this
session did not time a real `cargo test --workspace` run end-to-end as
part of this kill-test, but `cargo build --workspace`/`cargo test
--workspace` runs performed elsewhere in this same session each completed
in well under two minutes — see the Phase 5 write-up in `CHANGELOG.md`).

## What was NOT verified

This kill-test measured real cache survival at a 150-second delay, not at
delays approaching the full hour. The 1-hour-tier finding is a real,
directly-observed mechanism fact (not a guess), and it is the correct basis
for expecting the ratio to hold much further out than 150s — but "the tier
is 1 hour" and "we confirmed the ratio holds for a full hour" are two
different claims, and only the first is made here. A future session with
more wall-clock budget than this one should confirm the ratio at, say,
30–50 minutes before treating the boundary as fully closed.

## Bearing on the sprint

**Both transitions are candidates for shipping**, not just `plan →
implement`. Phase 2 ships session continuity across `plan → implement →
fix` uniformly (the same `ClaudeCode` value, resumed throughout one
attempt) rather than special-casing `fix` as cold, because nothing measured
here shows the `implement → fix` gap (a `cargo test` run on this repo,
typically well under a minute in this session's own observations) is
anywhere near the ~1-hour boundary this kill-test found. This is the
"good outcome, not partial failure" case the brief names as acceptable —
except the finding here actually supports shipping the *wider* scope, not
the narrower one, which the brief's own framing treated as the less likely
result. Phase 2's resume-failure fallback (cold retry on any
establishment failure, `claude_spawn.rs`) is the safety net if a real
deployment's test phase ever *does* run past the cache boundary in
practice — a stale/expired resume degrades to a cold spawn automatically,
so shipping the wider scope carries no correctness risk even if this
kill-test's TTL estimate turns out optimistic for some repo.
