# F3 Bridge Decoupling — 30-run paired measurement

**Sprint:** F3 · **Date:** 2026-07-26 · **Volume regime:** pre-F1 (F1 has not
landed as of this run — see `KT-3.4`)

## Environment

Not the M3 hardware named in the sprint brief — this session runs in a
remote Linux container and cannot drive four concurrent live Claude Code
agent sessions for a 30-run paired comparison. Substituted a synthetic-load
harness (`crates/lopi-ui/src/web/event_bridge_bench.rs`, `bridge_load_bench`)
that drives the real `event_bridge::spawn` function against a real
disk-backed `MemoryStore` (same dual-pool WAL config as production), with 4
concurrent synthetic agents each emitting `AgentEvent::LogLine` at ~250
lines/sec (a documented assumption approximating `--include-partial-messages`,
not a measurement of a real session). 3,000 lines/agent × 4 agents = 12,000
events per run.

## Method

- **Pre-fix sample** (`prefix.jsonl`): `git checkout`ed `event_bridge.rs`,
  `task_logs.rs`, `store/mod.rs`, `run_loop.rs`, `metrics_handlers.rs`, and
  `lopi-ui/Cargo.toml` back to their pre-sprint content (commit `6688d7d`),
  keeping only the bench harness wired in, then ran the release test binary
  30 times.
- **Post-fix sample** (`postfix.jsonl`): restored all Phase 1-5 changes and
  ran the identical binary invocation 30 times.
- Both samples use the *same compiled release binary* per condition, run
  back-to-back, to minimize environment drift between paired runs.
- Paired Wilcoxon signed-rank test (normal approximation, continuity
  corrected, tie-corrected variance) computed with `wilcoxon.py` (no scipy
  in this environment). Effect size: matched-pairs rank-biserial correlation.

## Results (n=30 paired runs each)

| Metric | Pre-fix median | Post-fix median | Wilcoxon p (two-sided) | Effect size r |
|---|---|---|---|---|
| p99 latency (bus.send → subscriber receipt) | 19.735 ms | 0.059 ms | 2×10⁻⁶ | 1.00 |
| p95 latency | 6.845 ms | 0.042 ms | 2×10⁻⁶ | 1.00 |
| p50 latency (secondary — must not regress) | 0.420 ms | 0.017 ms | 2×10⁻⁶ | 1.00 |
| Dropped broadcast events (`RecvError::Lagged`) | 0 (all 30 runs) | 0 (all 30 runs) | n/a — no variance in either condition | n/a |
| Rows landed in `task_logs` vs. lines emitted | ~4,208 / 12,000 (pruned mid-run) | 12,000 / 12,000 | correctness check, not a paired test | — |

`W_pos = 0, W_neg = 465` for every latency metric — **all 30 pairs moved in
the same direction** (post-fix faster), the strongest possible signal at
this sample size.

### Note on the `task_logs` row count

Post-fix landed every one of the 12,000 emitted lines during the ~13s test
window because pruning is now on a 30s sweep timer (Phase 3) that never
fires within a run this short — not because pruning was disabled or broken
(see `event_bridge.rs`'s `prune_sweep_enforces_max_per_task_on_a_timer`
test, which uses a short injected interval and confirms `MAX_PER_TASK` is
still enforced once the sweep does fire). Pre-fix pruned continuously
(every 64 lines), so only the most recent ~1,000 rows per task survived by
the time each run ended. Neither number reflects silent data loss — both
are the correct outcome for each design's cadence.

## KT-3.2 cross-check (prune-boundary vs. steady-state, from the pre-fix sample)

- `prune_boundary_p95_ms` median: 9.37ms; `steady_state_p95_ms` median: 6.67ms
- Prune boundary higher in 22/30 runs; median per-run difference +2.60ms
- See `.konjo/killtests/F3/KT-3.2.md` — real but modest/noisy signal, not a
  dramatic isolated spike.

## Merge decision

**Merges.** Per the brief's stated criterion: dropped events flat (zero in
both conditions, no regression), p99 down by ~2-3 orders of magnitude with
p<0.000002 and effect size 1.0, p50 not worse (also dramatically better),
and no row loss under normal load (100% of emitted lines landed post-fix
within the test window; pre-fix's lower count is deliberate pruning, not
loss). This is not a marginal result requiring judgment calls — every one
of 30 paired runs moved the same direction on every latency metric.
