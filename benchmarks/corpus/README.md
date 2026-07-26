# lopi Benchmark Corpus

Ten canonical benchmark tasks (T01–T10) for measuring agent throughput, quality, and cost.

**Status (Sprint F0, 2026-07-26):** the "Expected Pass Rate" column below is
a pre-registered *estimate*, not a measurement — no run of this corpus has
ever been committed (`benchmarks/results/` did not exist before this
sprint). `./benchmarks/run.sh --dry-run` and `--tasks T01` were verified to
work (KT-0.1), confirming the harness itself is not bit-rotted, but running
the full ten-task corpus against a real Claude subscription is an attended,
hardware-required action (real money, real wall-clock, needs a human
watching) that an unattended agent session cannot perform — see
`.claude/rules/benchmarking.md` for the protocol this requires (≥5 warmup
runs, documented hardware, p50/p95/p99, a new timestamped results
directory). **Do not rename this column to "Measured Pass Rate" until a
real run has been committed to `benchmarks/results/` and this note is
removed or updated to cite it.** See NEXT_SESSION_PROMPT.md.

## Task Definitions

| ID  | Goal | Complexity | Expected Pass Rate |
|-----|------|------------|--------------------|
| T01 | Add a unit test for the `jaccard_similarity` function | Low | 100% |
| T02 | Add a `#[derive(PartialEq)]` to `AgentState` and update all match exhaustiveness | Low | 100% |
| T03 | Implement `Display` for `TaskStatus` that produces human-readable output | Medium | 100% |
| T04 | Add `created_at` index to the `patterns` table in `schema.sql` | Low | 100% |
| T05 | Add a `--verbose` flag to `lopi run` that prints raw claude output | Medium | 90% |
| T06 | Refactor `runner.rs` to extract the plan+implement+fix loop into a named method | Medium | 85% |
| T07 | Add `GET /api/metrics` endpoint to the web dashboard returning PoolStats as JSON | Medium | 90% |
| T08 | Implement `retry_with_backoff` in `runner.rs` for transient IO errors | High | 80% |
| T09 | Add `lopi bench` CLI subcommand that runs the T01–T10 corpus sequentially | High | 75% |
| T10 | Integrate `AnthropicLimiter` into `AgentPool` for TPM/RPM enforcement | High | 70% |

## Measurement Protocol

For each task:
1. Start from a clean `main` branch (no uncommitted changes)
2. Run `lopi run --goal "<task goal>" --repo .`
3. Record: attempt count, wall-clock time, final status, token usage from `turn_metrics`
4. Accept: `TaskStatus::Success` on attempt ≤ 3 within 10 minutes

## Success Criteria

- p50 task completion: ≤ 3 minutes
- Overall corpus pass rate: ≥ 80%
- Zero hallucinated APIs in produced code
- All produced code passes `cargo clippy -- -D warnings`
