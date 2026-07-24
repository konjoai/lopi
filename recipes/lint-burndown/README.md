# lint-burndown

## What it does

Runs clippy and `cargo fmt --check` (or your language's equivalent) and lets
the agent fix everything they flag, on a bounded cadence, without waiting for
a human to notice the warnings first.

## F0 rationale

The simpler thing this beats is `cargo clippy --fix` — the tool clippy
already ships to auto-apply its own suggestions. That's the right call for
the subset of lints with a mechanical, always-safe rewrite. This recipe
earns its keep on everything `--fix` can't touch unattended: lints whose fix
requires understanding intent (a `needless_range_loop` that should become an
iterator adaptor chosen for the actual use, not a generic one), or a
`cargo fmt --check` failure alongside a clippy warning in the same pass. If
your warnings are all auto-fixable today, run `--fix` and skip this recipe;
reach for this loop once the remaining warnings need a decision `--fix`
can't make.

## Principles demonstrated

- **F3 — three hard stops**, all explicit: `max_iterations = 8`,
  `no_progress_limit = 2`, `[budget] preset = "quick"`.
- **F7 — match effort to the task.** A lint pass is the textbook low-stakes,
  mechanical loop: it gets the cheapest budget preset, and deliberately
  skips the verifier (`verifier_required` stays unset/`false`) — the
  deterministic `until` gate is already the only check this task needs, so
  paying for a second model's grading pass buys nothing here.

## Stop conditions

| Field | Value | Why |
|---|---|---|
| `max_iterations` | `8` | Warnings often span several files; a single test fix needs fewer attempts than clearing an entire lint pass, so this recipe budgets a few more than `fix-failing-test`. |
| `no_progress_limit` | `2` | If two attempts in a row don't reduce the warning count, the agent is thrashing, not converging. |
| `[budget] preset = "quick"` | $1 / 200K tokens, fan-out denied | Lint fixes are small, local edits — no research phase, no multi-file investigation needed. |

## Expected cost and duration

**TODO — pending live-run measurement.** Will be filled in from a real
`lopi_submit_task` run against a scratch crate with clippy/fmt warnings,
per `recipes/README.md`'s applied-and-run steps, before this sprint closes.

## When not to use this

- **The lint config itself is wrong** (too strict for the codebase's actual
  style, or flagging something the team has already decided to allow).
  Fix `clippy.toml`/`#[allow(...)]` policy by hand — a loop shouldn't be
  negotiating with its own gate.
- **You want every warning auto-applied instantly, no review.** This recipe
  ships at `report_only` on purpose (see the loop.toml comment) — promote to
  `draft_pr` only after you've read a few nights of its reports and trust
  what it tends to change.
- **The repo has zero warnings.** `until` will pass on iteration 1 with an
  empty diff — harmless, but there's nothing for the loop to do; don't
  schedule it until there's a backlog.
