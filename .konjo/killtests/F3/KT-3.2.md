# KT-3.2 — Is the 64-line prune a distinct spike?

**Sprint:** F3 · **Verdict:** PASS (real but modest/noisy signal) — Phase 3 targets it, described honestly.
**Baseline commit:** `6688d7d` (`v0.28.0`, post-F2), same 30-run pre-fix sample as `KT-3.1`.

## Method

Reused `KT-3.1`'s harness. Every `LogLine` is tagged with a `#PB` marker when
it immediately follows a multiple-of-64 insert — the line whose predecessor
triggered the pre-fix `prune_task_logs` call (`event_bridge.rs:66-70`,
pre-fix). Bucketed broadcast latency by that marker across all 30 pre-fix
runs (not a single sample — a single run's ratio is noisy at this event
count) and compared `prune_boundary_p95_ms` against `steady_state_p95_ms`
per run.

## Result

Across the 30-run pre-fix sample (`benchmarks/results/`):

- `prune_boundary_p95_ms`: median 9.37ms (min 1.71, max 21.72)
- `steady_state_p95_ms`: median 6.67ms (min 4.13, max 13.09)
- `prune_boundary_p95_ms > steady_state_p95_ms` in **22 of 30 runs** (73%)
- Median of the per-run difference (`prune_boundary - steady_state`):
  **+2.60ms**; mean difference: **+3.53ms**; range **-6.24ms to +13.68ms**

## Verdict

**Pass, but a real-and-noisy signal, not a clean one — reported as such.**
The prune boundary is higher than the steady state in a clear majority of
runs (22/30) with a positive median difference, so this is not "lost in the
noise" in the sense the brief's fail condition describes — but it is also
not a universal, dramatic spike (8/30 runs show the opposite direction, and
the per-run difference ranges from -6ms to +14ms). At this synthetic event
count, the every-64th-line prune bucket is a small fraction of samples
(~1.5%) per run, which is enough to explain the run-to-run noise without
implying the effect is fake.

**Consequence:** Phase 3 targets the prune specifically (time-based sweep,
off the broadcast hot path — see `event_bridge.rs`'s `drain_persist_logs`),
but this doc does not oversell it as a dramatic isolated spike. The general
p99 tail `KT-3.1` already confirmed is the dominant, more reliable signal;
the prune's contribution to it is real but one contributor among several
(the general per-line `record_task_log` await is itself a constant cost —
see `steady_state_p95_ms` sitting well above `p50_ms` too).
