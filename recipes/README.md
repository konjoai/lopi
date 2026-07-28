---
decays: state
verified-against: ca8e980
verified-date: 2026-07-28
---

Verified against: `ca8e980` · 2026-07-28 (re-verified; this document carries no
line-number citations to check for drift — it's a conceptual F-principle-to-`LoopConfig`
mapping, not a status/citation ledger. Confirmed every field the table below names
(`gate`, `until`, `verifier_required`, `verifier_model`, `verifier_effort`,
`max_iterations`, `no_progress_limit`, `[budget]`, `vision_path`, `skills_enabled`,
`rules_enabled`, `isolation`, `autonomy_level`, `promote_after`, `trust_ceiling`,
`permission_allow`, `permission_deny`) still exists on `LoopConfig`
(`crates/lopi-core/src/loop_config.rs`) with the same name. No drift found; neither
Sprint G nor the demo/measurement sprint touched `loop_config.rs` or `recipes/`.)

# lopi recipe library

A recipe is a **worked example of the Konjo Forward loop framework, in lopi's
own configuration language, that runs.** Not a convenience wrapper, not a
tutorial — a `.lopi/loop.toml` you can copy into a real repo today, plus the
README that explains why it's shaped the way it is.

`LoopConfig` (`crates/lopi-core/src/loop_config.rs`) maps almost one-to-one onto
the loop-engineering principles this library calls out as **F0–F7** and **F10**
below. lopi has no `init`/scaffold command and no recipe registry — this
directory *is* the mechanism: copy a `loop.toml`, adjust paths, run.

## The F-principle legend

These labels aren't an external standard — they're this repo's own shorthand,
introduced by this sprint to name the loop-engineering principles
`docs/LOOP_ENGINEERING.md` already describes in prose (Steinberger's five
rules, Cherny's verify-first/write-it-down, Osmani/Greyling's maker-checker
and phased rollout). Every recipe README below cites the ones it exercises,
and through which field.

| # | Principle | The `LoopConfig` field(s) |
|---|---|---|
| **F0** | **Earn the loop.** Don't automate a task until a manual or scripted version has proven it's worth repeating and the check is trustworthy. Every recipe here must name the simpler thing it beats and why that thing falls short. | (a discipline on the *author*, not a field) |
| **F1** | **Deterministic oracle over model judgment.** Prefer a loop whose pass/fail is a real command's exit code — tests, lint, build — never a model grading its own work. | `gate`, `until` |
| **F2** | **Maker ≠ checker.** The agent that writes the change is never the sole judge that it's correct. | `verifier_required`, `verifier_model`, `verifier_effort` |
| **F3** | **Three hard stops.** Every unattended loop needs an explicit ceiling on iterations, on stalling, and on spend. | `max_iterations`, `no_progress_limit`, `[budget]` |
| **F4** | **Anchor the intent.** Ground each planning turn in the repo's actual documented intent, not an assumed one. | `vision_path`, `skills_enabled`, `rules_enabled` |
| **F5** | **Isolate parallel work.** Concurrent or repeated runs must not be able to corrupt each other's working copy. | `isolation` (`Branch` \| `Worktree`) |
| **F6** | **Earn autonomy in stages.** Trust is promoted one rung at a time after a track record of clean, verified runs — never granted at the top from turn one. | `autonomy_level`, `promote_after`, `trust_ceiling` |
| **F7** | **Match model/effort to the task.** A narrow, mechanical, low-stakes loop should spend less per turn than a research-grade one. | `verifier_model`, `verifier_effort` |
| **F10** | **Least privilege — contain by construction.** Tool access is scoped tightly, especially wherever the loop's input includes untrusted content. | `permission_allow`, `permission_deny` |

## Recipe format

Each recipe lives in `recipes/<name>/` and contains exactly two files:

- **`loop.toml`** — a complete, valid `.lopi/loop.toml`, ready to copy into a
  target repo's `.lopi/` directory. It must parse (`lopi loop validate`) and
  set every field explicitly that this contract requires below — never rely
  on a silently-inherited default for the fields that matter to the reader.
- **`README.md`** — the teaching half. **Every one of these sections is
  required**, not a suggestion; a recipe missing one is incomplete:
  1. **What it does** — one sentence.
  2. **F0 rationale** — the simpler thing this recipe beats, and why it falls
     short. A recipe that cannot answer this is not a recipe; it's a script
     someone should write instead.
  3. **Principles demonstrated** — which F-principles above this recipe
     exercises, and through which config field(s).
  4. **Stop conditions** — the concrete `max_iterations`, `no_progress_limit`,
     and budget for this recipe, and the reasoning behind each number.
  5. **Expected cost and duration** — measured from a real run, never
     estimated.
  6. **When not to use this.**

### Constraints every recipe in this library follows

- **`Quick` or `Standard` budget by default.** `Deep` re-enables sub-agent
  fan-out (`Workflow`/`Task`/`Agent`) and must never appear without an
  explicit justification in that recipe's README — see
  `crates/lopi-core/src/budget_preset.rs`'s `FAN_OUT_DENY` comment for why
  fan-out through `Task` turned a $3-capped session into a $6.89 one (and,
  uncapped, $25.79): the `[budget]` USD cap only checks *between* turns, so it
  cannot govern money spent by parallel sub-agents.
- **Every recipe sets `max_iterations`, `no_progress_limit`, and `[budget]`
  explicitly**, even where the default matches — so a reader sees the F3
  numbers and the reasoning, not an invisible inheritance.
- **No recipe ships at `auto_merge`.** Every recipe here starts at
  `report_only` or `draft_pr`; each README says what a human should see
  before ever raising `trust_ceiling`.

## The recipes

**"Live run" reports what was actually measured, not what was hoped for** —
two recipes below ran live and surfaced real, confirmed gaps rather than a
clean success; both are documented in full in their own READMEs and carried
forward in `NEXT_SESSION_PROMPT.md`, not papered over.

| Recipe | Purpose | Budget | Principles | Live run |
|---|---|---|---|---|
| [`fix-failing-test`](./fix-failing-test/) | The canonical loop — a failing test is a deterministic pass/fail oracle | `quick` | F1, F3, F6 | ✅ success — 34.2s, $0.033 |
| [`lint-burndown`](./lint-burndown/) | Scheduled, bounded clippy/fmt burndown — a lint pass doesn't need the biggest model | `quick` | F3, F5, F7 | ✅ success — 60.5s, $0.057 |
| [`dependency-bump`](./dependency-bump/) | Update dependencies, gated on the full test suite passing | `standard` | F1, F2, F3, F5, F6 | ⚠️ ran live, found a real `DiffChecker`/MCP gap — [details](./dependency-bump/) |
| [`flaky-test-hunter`](./flaky-test-hunter/) | Re-run a suspect test N times and characterize intermittency | `quick` | F1 (inverted), F3 | ✅ success after 3 attempts — 276.6s, $0.535 (`no_progress_limit` demonstrated live) |
| [`doc-drift-check`](./doc-drift-check/) | Scan for `decays: state` docs whose `verified-against` has fallen behind `HEAD` | `quick` | F1, F3, F4, F5 | ⚠️ ran live, hit the same gap as `dependency-bump` — [details](./doc-drift-check/) |
| [`triage-issues`](./triage-issues/) | Read incoming issues, label and summarize — untrusted input, tight containment | `quick` | F2, F3, F10 | ✅ success — 27.6s, $0.024 (injection attempt correctly ignored) |

## Applying a recipe

Recipes assume an existing repo with `lopi` already built (or installed) and
the target repo cloned. There is no `lopi init` — you're adjusting an
existing loop config, not scaffolding a project.

1. **Copy the config into your target repo:**
   ```bash
   mkdir -p /path/to/your-repo/.lopi
   cp recipes/<name>/loop.toml /path/to/your-repo/.lopi/loop.toml
   ```
2. **Open the copied file and adjust anything repo-specific** — each recipe's
   README calls out what to check (e.g. `gate`/`until` commands that assume a
   particular test runner, `vision_path` pointing at a doc that may not exist
   in your repo). **Check for a "Required prerequisite" section** —
   `dependency-bump` and `doc-drift-check` both need an `allowed_dirs`
   addition in `<repo>/.lopi.toml` before they can produce an accepted diff
   at all (confirmed live; see each recipe's README for why).
3. **Validate it parses and inspect the effective config:**
   ```bash
   lopi loop validate --repo /path/to/your-repo
   lopi loop show --repo /path/to/your-repo
   ```
4. **Run it:**
   ```bash
   lopi run --goal "<the goal from the recipe's README>" --repo /path/to/your-repo
   ```
   For `lint-burndown` (and any recipe meant to run on a cadence), instead add
   a `[[schedules]]` entry to your `lopi.toml` pointing at the same repo and
   goal — see `lopi.toml.example`.

Every recipe was applied and run against a fresh scratch repo using exactly
these steps — see each recipe's "Expected cost and duration" section for
what that run actually produced. Four of six reached a clean success;
`dependency-bump` and `doc-drift-check` did not, and their READMEs report
the real, root-caused reason rather than a retried-until-clean number. All
measurements used `lopi_submit_task` over MCP (`permission_mode:
"acceptEdits"`/`"auto"`) rather than the CLI shown above, because this
sprint's own sandbox runs as `root` and the CLI's default
`bypassPermissions` refuses to run as root outside a CLI-recognized
sandbox — a constraint of *this development environment*, not of the CLI
path itself, which is what a real user runs and what these recipes'
instructions describe.

## Out of scope (this sprint)

No recipe marketplace/registry, no UI recipe builder, no `lopi init`, and no
changes to `LoopConfig`/budget presets/the runner — see `NEXT_SESSION_PROMPT.md`
for anything a recipe's live run exposed as a real `LoopConfig` gap. This
directory alone was judged sufficient; a `lopi recipes list|show|apply`
command is deferred unless the copy workflow above proves genuinely awkward
in practice.
