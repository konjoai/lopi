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

Measured live (2026-07-24) against a scratch repo with one `decays: state`
doc (`docs/CURRENT_STATE.md`) stamped 14 commits behind `HEAD`:

- **Outcome:** `failed` after 3 attempts, $0.48. **Same confirmed gap as
  `dependency-bump`, hit for the same underlying reason**: this recipe's
  goal edits `docs/CURRENT_STATE.md`, outside `Task::allowed_dirs`' default
  (`["src/", "tests/"]`), and the required `.lopi.toml` override
  (`allowed_dirs = [..., "docs/"]`) never reached the task because
  `lopi_submit_task` (MCP) doesn't apply `RepoProfile` — see
  `recipes/dependency-bump/README.md`'s full write-up and
  `NEXT_SESSION_PROMPT.md`. Not a second, independent bug; the same one,
  reproduced by a second recipe, which is itself useful confirmation that
  it's general rather than dependency-bump-specific.
- What the attempts actually did, despite the rejection: attempt 1 drafted
  a plan and stopped short of writing, asking for permission (rejected —
  zero diff). Attempt 2 correctly measured "14 commits stale" and again
  stopped short (rejected). Attempt 3 correctly rewrote
  `docs/CURRENT_STATE.md` to describe the repo's actual state and reduced
  the drift to 0 commits — content-correct, and still rejected by the
  `DiffChecker` for the scope reason above, not for anything wrong with the
  edit itself.
- A real repo applying this recipe via the documented CLI path (`lopi run`,
  which *does* apply `.lopi.toml`) with `docs/` added to `allowed_dirs`
  should not hit this; that combination isn't reachable from this sandbox
  any more than `dependency-bump`'s was.

**This recipe also needs a `.lopi.toml` prerequisite**, same shape as
`dependency-bump`'s:

```toml
allowed_dirs = ["src/", "tests/", "docs/"]
```

Note separately: the shipped `until` command's staleness threshold (`10`
commits) was sized for the small scratch repo above — raise it (e.g.
30–50) for a real, actively-developed repo.

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
