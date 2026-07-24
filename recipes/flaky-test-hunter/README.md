# flaky-test-hunter

## What it does

Re-runs a suspect test repeatedly, reports its actual pass rate, and only
touches code if the agent can identify and explain a concrete root cause —
it never trusts a single green run as proof of a fix.

## F0 rationale

The simpler thing this beats is re-running the test in CI a few more times
by hand and shrugging if it goes green — which is exactly the behavior that
lets flaky tests survive for months. It's also simpler than quarantining the
test (`#[ignore]` / skip-list) and moving on, which makes the flakiness
someone else's problem forever. This recipe earns its keep specifically
because "run it again" is *not* a fix, and a loop that doesn't understand
that will happily report false success. The entire design of this recipe —
no `until`, a low `no_progress_limit` — exists to resist that failure mode.

## Principles demonstrated

- **F3 — three hard stops**, with `no_progress_limit = 3` doing the real
  work here. A flaky test can make *every single attempt* look like
  progress (it passed! or: here's a plausible-sounding root cause!) without
  the loop ever converging on something true — no-progress detection is the
  only thing that stops that spin before `max_iterations` is exhausted.
- **F1, inverted.** Every other recipe in this library sets `until` to the
  deterministic pass/fail check. This one deliberately does **not** — for a
  test whose entire premise is intermittent failure, "it passed once" is not
  a deterministic oracle, it's the exact evidence a flaky test is expected
  to produce whether or not anything was fixed. Demonstrating *when not* to
  reach for F1's usual pattern is as much a teaching point as F1 itself.

## Stop conditions

| Field | Value | Why |
|---|---|---|
| `max_iterations` | `8` | Characterizing intermittency needs several repeated runs per attempt, plus a few attempts to test a hypothesis — more headroom than a single deterministic fix. |
| `no_progress_limit` | `3` | The load-bearing stop for this recipe (see above) — three non-improving attempts and it halts rather than spinning on false "progress." |
| `[budget] preset = "quick"` | $1 / 200K tokens, fan-out denied | Repeated local test runs are cheap; no research phase needed. |

## Expected cost and duration

Measured live (2026-07-24) against a scratch crate whose test fails on
every third run by design (a counter persisted to disk), applied and run
per `recipes/README.md`'s steps via `lopi_submit_task`:

- **Outcome:** `success` after 3 attempts — and the 3-attempt shape is
  itself the demonstration, not overhead to explain away:
  - **Attempt 1**: ran the suite 9 times, correctly identified the root
    cause (the persisted counter), but only *reported* it — no file
    changed. Rejected: `no file changes produced, but this goal expects
    file edits`.
  - **Attempt 2**: re-characterized (correctly: "2:1 pass/fail ratio,
    deterministic pattern"), again with zero diff. Rejected for the same
    reason. Two accurate reports in a row, both correctly refused as "not
    yet done" — this is `no_progress_limit` doing exactly its job: neither
    attempt advanced the loop even though both were substantively good
    analysis.
  - **Attempt 3**: applied a real fix removing the counter-driven
    intermittency (verified: the resulting diff is a genuine, minimal fix,
    not a deleted or weakened test). Accepted, loop ends.
- **Wall-clock:** 276.6s end-to-end (task `created_at` → `completed_at`)
- **Cost:** $0.535 over 3 attempts, inside the `quick` preset's $1 cap

The two "rejected but correct" attempts are the point: a scoring/no-progress
mechanism that only looked at whether *something* changed would have
accepted attempt 1's zero-diff report as final and stopped there, no fix
applied. `flaky-test-hunter`'s F3 stop conditions and this recipe's
"goal expects file edits" scoring signal are what pushed it to attempt 3.

## When not to use this

- **The test fails 100% of the time, or 0%.** That's not flaky, that's
  deterministic — use `fix-failing-test` (or nothing, if it's passing).
- **You already know the root cause.** This recipe is for characterization
  when you *don't* — if you already know it's a race on a shared temp file,
  just fix it directly.
- **You need this decided before the next CI run.** Characterizing
  real intermittency takes many repeated runs; this is not a fast recipe.
