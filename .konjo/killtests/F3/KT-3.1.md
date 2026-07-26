# KT-3.1 — Does lag reproduce? (BLOCKING)

**Sprint:** F3 · **Verdict:** PASS (via the p99≫p50 criterion) — build the fix.
**Baseline commit:** `6688d7d` (`v0.28.0`, post-F2)
**File under test:** `crates/lopi-ui/src/web/event_bridge.rs` (pre-fix: single
`tokio::spawn`ed loop that serializes, re-broadcasts, then `.await`s
`record_task_log`/`prune_task_logs` in the same iteration).

## Environment substitution — stated plainly

The brief calls for "the M3, four concurrent agents, `--include-partial-messages`
on, a real repo and real subscription." This session runs in a remote Linux
container, not the M3 hardware named in the brief, and cannot drive four
concurrent live Claude Code agent sessions for a 30-run paired comparison in
this environment. Rather than skip the measurement or fabricate numbers
against unavailable hardware, this kill-test substitutes a synthetic-load
harness — `crates/lopi-ui/src/web/event_bridge_bench.rs` — that drives the
*actual* `event_bridge::spawn` function against a real disk-backed
`MemoryStore` (same dual-pool WAL configuration as production: single-writer,
8 readers, `journal_mode=WAL`, `synchronous=NORMAL`), with 4 concurrent
synthetic agents each emitting `AgentEvent::LogLine` at a stated rate
(~250 lines/sec/agent, chosen to approximate `--include-partial-messages`
streaming — a documented assumption, not a measurement of a real session).
This substitution is recorded here rather than left implicit, per this
sprint's own "measure, don't overclaim" instruction.

## Method

1. `git checkout` the four touched files back to their pre-fix
   (`6688d7d`) content, keeping only the new bench harness wired in via
   `event_bridge.rs`'s test-only `mod event_bridge_bench;` hook — this
   guarantees the "pre-fix" measurement below is not contaminated by any
   in-flight Phase 1-5 edit.
2. `cargo test -p lopi-ui --release bridge_load_bench -- --ignored --nocapture --test-threads=1`
   (release build, single-threaded test harness so the bench's own
   multi-thread Tokio runtime — `worker_threads = 8` — isn't starved by
   `cargo test`'s own parallelism).
3. Instrumented exactly what the brief asks: count and summed `n` of
   `"serializer bridge lagged"` warnings (via `RecvError::Lagged` on the
   bench's own broadcast subscriber), broadcast latency p50/p95/p99 (wall
   clock from each `LogLine`'s `ts` to the subscriber's receipt), and lines
   sent/received.

## Pre-fix output (verbatim single run, clean, against unmodified `6688d7d` code)

```
BENCH_RESULT {"lines_sent":12000,"lines_received":12000,"lagged_events":0,"lagged_sum_n":0,"p50_ms":0.369,"p95_ms":5.074,"p99_ms":17.106,"prune_boundary_p95_ms":6.793,"steady_state_p95_ms":5.044,"rows_in_db":4255}
```

4 agents × 3,000 lines each = 12,000 total `LogLine` events.

## 30-run confirmation (this run became the paired-test baseline, Phase 6)

Median across all 30 pre-fix runs (`benchmarks/results/20260726T205826Z_f3_bridge/`):
p50 = 0.420ms, p95 = 6.845ms, **p99 = 19.735ms**. `lagged_events` = 0 in
every one of the 30 runs. Consistent with the single run above — the p99≫p50
tail is not a fluke of one sample.

## Verdict

**Pass — via the second criterion, not the first.** No explicit
`RecvError::Lagged` fired at this synthetic volume, in any of the 30 runs
(the bench's single, continuously-reading subscriber never falls behind the
512-capacity broadcast channel outright). But **p99 latency (median 19.7ms
across 30 runs) is ~47× p50 (median 0.42ms)** — a large, consistent tail
entirely explained by the inline `record_task_log`/`prune_task_logs` awaits
landing on some fraction of events. That is a materially-above-p50 tail,
satisfying the brief's explicit alternative pass condition ("lag reproduces,
**or** p99 latency is materially above p50"). Per the brief: **build the
fix, and use these numbers as the paired-test baseline** — see Phase 6's
result in `benchmarks/results/20260726T205826Z_f3_bridge/summary.md`: p99
dropped to a median of 0.059ms post-fix, p<0.000002, effect size 1.0 (all 30
pairs moved the same direction).

Not escalating volume further to try to force an explicit `Lagged` warning
— the brief's own non-goal is clear that inflating load to manufacture a
more dramatic number would contaminate rather than clarify the measurement.
The p99 tail already demonstrates the coupling's real cost, and the fix's
effect size on it is unambiguous.
