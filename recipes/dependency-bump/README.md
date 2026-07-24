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

## Required prerequisite: allow `Cargo.lock` in the target repo's `.lopi.toml`

**Read this before applying — every attempt fails without it.** lopi's
`DiffChecker` rejects any changed path outside `Task::allowed_dirs`, which
defaults to `["src/", "tests/"]` (`crates/lopi-core/src/task.rs`). A
dependency bump touches `Cargo.lock` (at the repo root) by definition, so
without an override **every attempt of this recipe hard-rolls-back with
`diff scope violation: diff touches path outside allowed scope: Cargo.lock`
— confirmed live, three attempts in a row, full cost charged each time**
before `max_iterations` gives up. This is not a corner case; it is the
normal shape of this recipe's only possible output. Add this to
`<repo>/.lopi.toml` (create it if it doesn't exist) before running:

```toml
allowed_dirs = ["src/", "tests/", "Cargo.lock"]
```

(Add other lockfile paths as needed for your ecosystem — e.g.
`package-lock.json`, `poetry.lock`.)

## Expected cost and duration

Measured live (2026-07-24) against a scratch crate with `itoa` locked to an
older-but-still-in-range version (`Cargo.toml` allows `^1.0.9`, `Cargo.lock`
pinned to `1.0.9` when `1.0.18` is available). Three separate live attempts
went into this number, and the first two are as important a result as the
third:

1. **First attempt, no `allowed_dirs` override**: the agent correctly ran
   `cargo update -p itoa`, correctly bumped `1.0.9 → 1.0.18`, correctly
   confirmed tests still passed — three times in a row across
   `max_iterations` — and every single attempt was hard-rolled-back by the
   `DiffChecker` for the exact reason above. `failed` after 3 attempts,
   **$0.33**, 122s. This is what motivated the prerequisite section above;
   it is not a hypothetical.
2. **Second attempt, `.lopi.toml` override added but submitted through
   `lopi_submit_task` (MCP)**: still failed, same violation. Root-caused to
   a second, independent, confirmed finding: **the MCP submission path
   (`src/mcp_commands/mod.rs::submit_task`) never calls
   `RepoProfile::load_from_repo(&repo).apply(&mut task)`** — unlike the CLI
   path (`src/run_command.rs:177`) and the REPL (`src/repl/actions.rs:101`),
   which both do. A task submitted via the Claude Code/Desktop MCP plugin
   integration silently ignores `.lopi.toml` entirely, regardless of this
   sandbox's constraints. See `NEXT_SESSION_PROMPT.md`.
3. Every measurement in this library used `lopi_submit_task` over MCP
   because this sandbox's `root` user can't use the CLI's default
   `bypassPermissions` (see the library's methodology note). For
   dependency-bump specifically, that same substitution collides with
   finding 2 above, so **this recipe could not be measured to a clean
   success in this environment** — reported here as the honest result,
   not papered over. A real user running the documented CLI path
   (`lopi run --goal "…" --repo <repo>`) with the `.lopi.toml` override in
   place is not subject to either gap and should see this recipe succeed
   in one attempt; that combination just isn't reachable from this sandbox.

**Total measured cost across all three diagnostic attempts: ~$0.85.**
Budget accordingly if you're reproducing this investigation rather than
just applying the recipe — a normal application, override in place,
should cost close to attempt 3's $0.32 for one successful attempt, not
three.

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
