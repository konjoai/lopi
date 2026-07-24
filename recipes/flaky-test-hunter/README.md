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

**TODO — pending live-run measurement.** Will be filled in from a real
`lopi_submit_task` run against a scratch crate whose test fails roughly 1 in
3 runs by design, per `recipes/README.md`'s applied-and-run steps, before
this sprint closes.

## When not to use this

- **The test fails 100% of the time, or 0%.** That's not flaky, that's
  deterministic — use `fix-failing-test` (or nothing, if it's passing).
- **You already know the root cause.** This recipe is for characterization
  when you *don't* — if you already know it's a race on a shared temp file,
  just fix it directly.
- **You need this decided before the next CI run.** Characterizing
  real intermittency takes many repeated runs; this is not a fast recipe.
