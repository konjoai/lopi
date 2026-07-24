# dependency-bump

## What it does

Updates one or more dependencies to their latest compatible versions and
refreshes the lockfile, opening a PR only if the full test suite still
passes afterward.

## F0 rationale

The simpler thing this beats is `cargo update` (or `npm update`/`pip-compile
--upgrade`) run by hand, on your own schedule, watched over your shoulder.
That's the right call for a repo you touch daily anyway — you'll notice a
break immediately. This recipe earns its keep once dependency drift becomes
a "the CI cron does it because nobody remembers to" problem: a repo touched
rarely enough that bumps pile up silently, or one with enough dependencies
that reviewing each bump individually is real toil. The test gate is what
makes running this *unattended* defensible — without it, this recipe would
just be `cargo update` with extra steps.

## Principles demonstrated

- **F1 — deterministic oracle**, used twice: `gate` confirms the baseline is
  green *before* the bump (so a pre-existing red test is never blamed on the
  dependency change), and the same command as `until` confirms it's still
  green *after*.
- **F3 — three hard stops**, explicit: `max_iterations = 5`,
  `no_progress_limit = 2`, `[budget] preset = "standard"`.
- **Bonus: F2 (maker ≠ checker) and F6 (earn autonomy in stages).**
  `verifier_required = true` — a passing test suite is necessary but not
  sufficient evidence a bump is safe (a semver-compliant release can still
  change behavior no test happens to cover), so a second pass grades it.
  `promote_after = 5` / `trust_ceiling = "verified_pr"` demonstrate the
  earned-trust ladder concretely: after 5 clean, verifier-passed bumps in a
  row, autonomy climbs one rung — but is capped at `verified_pr` (L3), never
  `auto_merge`. A repo doesn't get to skip review by having a good run of luck.

## Stop conditions

| Field | Value | Why |
|---|---|---|
| `max_iterations` | `5` | A dependency bump is either a clean version-string edit + lockfile refresh, or it surfaces a real breaking change a human needs to see — more retries don't help the second case. |
| `no_progress_limit` | `2` | Two non-improving attempts means the agent is fighting the same breakage repeatedly rather than resolving it. |
| `[budget] preset = "standard"` | $1 / 1M tokens, fan-out denied | One notch above `quick`: a bump can touch a lockfile plus call sites across multiple files if the dependency's API shifted. |

## Expected cost and duration

**TODO — pending live-run measurement.** Will be filled in from a real
`lopi_submit_task` run against a scratch crate with one dependency pinned to
an old exact version, per `recipes/README.md`'s applied-and-run steps,
before this sprint closes.

## When not to use this

- **A dependency has a known CVE and needs to move *now***. This recipe's
  bounded, test-gated loop is for routine hygiene, not incident response —
  handle a security bump by hand with the urgency it deserves.
- **The repo has no meaningful test suite.** `gate`/`until` are only as
  trustworthy as the tests they run; a thin or absent suite means "tests
  pass" tells you almost nothing about whether the bump is safe.
- **A major-version bump is what's actually needed.** "Latest compatible"
  here means within the existing semver constraint — crossing a major
  version is a deliberate migration, not a burndown task.
