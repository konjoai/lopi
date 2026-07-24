# fix-failing-test

## What it does

Points a lopi agent at one named failing test and lets it iterate — plan,
edit, re-run the test — until that exact test passes or the loop's hard
stops are reached.

## F0 rationale

The simpler thing this beats is fixing the test yourself, by hand, in an
editor. That's the right call for a test you already understand. This
recipe earns its keep only once you have *enough* failing tests, or tests
failing *often* enough (a nightly build, a big refactor's fallout), that
babysitting each one individually is the actual bottleneck — and only
because the check here is unimpeachable: a named test either exits 0 or it
doesn't, so there is no plausible-but-wrong output for a human to catch
after the fact. A goal without that kind of oracle (see `triage-issues` for
the opposite case) does not earn this loop; it earns a human doing the
first one by hand until the shape of the fix is obvious.

## Principles demonstrated

- **F1 — deterministic oracle.** `until = "cargo test <TEST_FILTER> -- --exact"`
  is the actual test runner, not a model's opinion of whether it's fixed.
  The loop ends the instant that command exits `0`.
- **F3 — three hard stops**, all set explicitly: `max_iterations = 5`,
  `no_progress_limit = 2`, `[budget] preset = "quick"`.
- **F6 — earn autonomy in stages.** Ships at `draft_pr` (L2), not
  `auto_merge` — even a deterministically-verified fix opens a PR for a
  human to merge, it never merges itself.

## Stop conditions

| Field | Value | Why |
|---|---|---|
| `max_iterations` | `5` | A single, already-isolated failing test is the simplest case lopi handles. If 5 attempts haven't found the fix, the test is telling you something the agent can't see (a design constraint, a flaky underlying dependency) — stop and look, don't keep spending. |
| `no_progress_limit` | `2` | Two attempts in a row with no improvement in score means the agent is repeating itself, not converging. |
| `[budget] preset = "quick"` | $1 / 200K tokens, fan-out denied | One file, one test, no research phase needed — `quick`'s ceiling is already generous for this shape of task. |

## Expected cost and duration

Measured live (2026-07-24) against a scratch two-line Rust crate
(`src/lib.rs` with `fn add(a, b) { a - b }` — a one-character bug — and one
failing `#[test]` asserting `add(2, 2) == 4`), applied and run exactly per
the steps in `recipes/README.md`'s "Applying a recipe" section, via
`lopi_submit_task` (`permission_mode: "acceptEdits"` — this sandbox's `root`
user can't use the CLI's default `bypassPermissions`; see
`NEXT_SESSION_PROMPT.md` for that finding):

- **Outcome:** `success` in 1 attempt
- **Wall-clock:** 34.2s end-to-end (task `created_at` → `completed_at`)
- **Cost:** $0.033 (summed `turn_metrics.estimated_cost_usd`), well inside
  the `quick` preset's $1 cap

A real repo's failing test will usually cost more turns than this
minimal reproduction — treat these numbers as a floor, not a typical case.

## When not to use this

- **The failure isn't isolated to one test.** If the whole suite is red
  (a broken build, a bad merge), fix the build first — this recipe expects
  exactly one named, addressable target.
- **The test itself might be wrong**, not the code. This recipe's prompt
  explicitly tells the agent not to touch the test; if the test is the bug,
  a human needs to make that call, not the loop.
- **The test is flaky, not failing.** A test that sometimes passes without
  any change will falsely report "fixed" the moment `until` gets lucky. Use
  `flaky-test-hunter` instead — it's built specifically to not trust a
  single green run.
