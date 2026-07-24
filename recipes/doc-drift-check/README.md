# doc-drift-check

## What it does

Scans for docs stamped `decays: state` whose `verified-against` commit has
fallen too far behind `HEAD`, and either reconciles them with current code
or flags exactly what's drifted.

## F0 rationale

The simpler thing this beats is a human periodically re-reading every
"current state" doc in the repo and asking "is this still true?" — which is
exactly what stops happening once a repo has more than a handful of such
docs, because nobody's job is to remember to do it. This recipe is, almost
verbatim, the failure this repo already lived through: `docs/
LOOP_ENGINEERING_ROADMAP.md` asserted capability gaps that were already
closed on `main`, caught by a kill-test rather than any gate (see
`.github/workflows/konjo-gate.yml`'s `doc-staleness` job and its own
commit-message history). This loop is that same check, runnable on demand
against any repo, not only in CI.

## Principles demonstrated

- **F1 — deterministic oracle.** `until` is a self-contained shell scan
  (`grep` + `git rev-list --count`) — no external tool dependency, no model
  judgment about whether a doc "feels" current.
- **F3 — three hard stops**, explicit: `max_iterations = 6`,
  `no_progress_limit = 2`, `[budget] preset = "quick"`.
- **F4, from an unusual angle.** Every other recipe *sets* `vision_path`/
  `skills_enabled`/`rules_enabled` to anchor its own plan in the repo's
  intent. This recipe doesn't need to — its entire job is verifying that the
  repo's *other* intent-anchors (the `decays: state` docs themselves)
  haven't silently drifted from what F4 promises they describe. It
  operationalizes F4 as the thing being checked, not the field being set.

## Stop conditions

| Field | Value | Why |
|---|---|---|
| `max_iterations` | `6` | Reconciling a handful of stale docs against current code is bounded, file-by-file work. |
| `no_progress_limit` | `2` | Two attempts that don't reduce the stale-doc count means the agent needs a human's read on what the doc should now say. |
| `[budget] preset = "quick"` | $1 / 200K tokens, fan-out denied | Reading and rewriting doc prose is token-light compared to code changes. |

## Expected cost and duration

**TODO — pending live-run measurement.** Will be filled in from a real
`lopi_submit_task` run against a scratch repo containing one `decays: state`
doc stamped 10+ commits behind `HEAD`, per `recipes/README.md`'s
applied-and-run steps, before this sprint closes. Note: the shipped
`until` command's staleness threshold (`10` commits) was sized for that
small scratch repo — raise it (e.g. 30-50) for a real, actively-developed one.

## When not to use this

- **Your repo doesn't use the `decays: state` convention at all.** The
  `until` gate above will simply never find a match and always exit `0` —
  harmless, but pointless to schedule. Adopt the convention first (stamp at
  least one doc), or rewrite the goal/gate around whatever staleness
  signal your repo actually uses.
- **The doc is wrong for reasons other than staleness** (it was never
  accurate, or the convention itself was misapplied). This recipe only
  detects *drift from a once-true baseline* — it can't tell you the
  baseline was wrong to begin with.
