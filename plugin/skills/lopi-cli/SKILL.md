---
name: lopi-cli
description: Documents the lopi CLI — a Rust multi-agent orchestrator that runs Claude Code agents concurrently in git-isolated branches. Use when the user asks to run a coding task through lopi, check on running/queued lopi agents, review lopi task history or logs, or interpret lopi's task status/output.
user-invocable: true
---

# lopi CLI

lopi runs Claude Code agents concurrently, each in a git-isolated branch,
with retry loops and persistent SQLite memory. This skill documents the CLI
surface as it ships today — no `;` composer grammar (that's separate,
unshipped work).

If the `lopi_*` MCP tools are available in this session (via the lopi
plugin's bundled MCP server), prefer them over shelling out to the `lopi`
binary for task submission and status checks — they give structured JSON
directly instead of parsed CLI text. This skill's command reference is what
to fall back on when the MCP tools aren't available, or when the user
explicitly asks to run a command.

## Submitting a task

```
lopi run --goal "<goal>" --repo <path>
```

`--repo` defaults to `.`. A good goal string is a single, concrete,
scoped instruction — the same granularity you'd give a human PR reviewer,
not a multi-part epic:

- Good: `"fix the failing test in src/foo.rs"`, `"add input validation to the /api/tasks POST handler"`
- Too broad: `"improve the codebase"`, `"add a bunch of features to the dashboard"`

`lopi run` streams progress to stdout as the agent moves through its
pipeline, one line per phase transition, e.g.:

```
🚢 lopi run
   goal: fix the failing test in src/foo.rs
   repo: .

   task id: a1b2c3d4…
   use `lopi watch` in another terminal for the TUI

  [1] → 📋 planning
  [1] → 🔨 implementing
  [1] → 🧪 testing
  [1] → 📊 scoring

⚓ success ✅ branch=lopi/a1b2c3d4/1, pr=https://github.com/…
```

On failure the final line reads `❌ failed <reason>` instead, and a task
that exhausted its retry budget without ever passing shows `⏪ rolled back`.

Other flags worth knowing: `--dry-run` (print the plan, make no changes),
`--speculative` (apply plan steps as they stream, faster wall-clock),
`--adaptive-retry` (feed the previous attempt's error into the next
planning prompt), `--budget <usd>` / `--budget-tokens <n>` (cap spend for
this run only).

For a permission-bypassed run in a trusted environment (skips
`allowed_dirs`/`forbidden_dirs` policy), use `lopi bypass <goal>` instead —
equivalent to `claude --dangerously-skip-permissions`, so only reach for it
when the user has explicitly asked for unrestricted execution.

## Watching agents run

```
lopi watch              # TUI, connects to a running `lopi sail` server if there is one
lopi watch --local       # TUI against a local-only event bus (no sail server)
```

Interactive full-screen dashboard — not useful to drive from inside a
Claude Code session (nothing to parse from a raw terminal UI). Prefer
`lopi tail`/`lopi dock` or the MCP tools for anything Claude needs to read
back.

## Reading status

```
lopi tail --history                # recent tasks, one line each
lopi tail --task-id <id-or-prefix>  # same, filtered to one task
lopi dock                           # full task table
```

`lopi dock` output:

```
⚓ lopi dock — 3 task(s)

  ID        Goal                                                Status
  ──────────────────────────────────────────────────────────────────
  a1b2c3d4  fix the failing test in src/foo.rs                  ✅ success
  e5f6a7b8  add input validation to the POST handler            🔨 implementing
  c9d0e1f2  refactor the auth middleware                        ❌ failed
```

`lopi tail --history` output is one line per task in the same
`[status] id… — goal` shape. Both commands read directly from the local
SQLite store — no running `lopi sail` server required.

```
lopi cancel <task-id-or-prefix>
```

Cancels a running task via a local `lopi sail` server on `:3000` (fails
with a clear message if none is running — this one *does* need `sail` up,
unlike `tail`/`dock`).

## Starting the dashboard

```
lopi sail [--port 3000] [--host 127.0.0.1] [--max-agents 4] [--repo .] [--repos a,b,c]
```

Starts the web dashboard and the agent pool's dispatch loop in one process.
`--repos` (multi-repo mode) dispatches tasks to more than one repo from the
same pool, routed by each task's `repo_path`. Startup banner:

```
🚢 lopi sail
   agents:    up to 4 concurrent
   repo:      .
   dashboard: http://127.0.0.1:3000
   api:       http://127.0.0.1:3000/api/tasks
   ws:        ws://127.0.0.1:3000/ws
```

A task submitted through `lopi run`, the dashboard, or the MCP tools while
`lopi sail` is running all show up in the same `lopi dock`/`lopi tail`
history — they all read the one shared SQLite store — but **live dispatch
is per-process**: a task is only actually executed by the specific
`AgentPool` it was submitted to (whichever process's `submit` call queued
it), not by every process that happens to be running.

## Task status — what the states mean

The status string you see in `lopi dock`, `lopi tail`, the dashboard, and
`GET /api/tasks` is `TaskStatus` (`crates/lopi-core/src/task.rs`). Read it
as this pipeline, left to right:

```
queued → planning → [awaiting plan approval]* → implementing → testing → scoring → success
                                                                              ↓
                                                                    retrying (attempt N+1)
                                                                              ↓
                                                                    failed | rolled back | conflict
```

- **queued** — waiting for a free agent slot.
- **planning** — the agent is generating an implementation plan.
- **awaiting plan approval** *(only if the task set `require_plan_approval`)* — paused; a human must approve or reject the plan before implementation starts.
- **implementing** — applying code changes on an isolated `orka/<task_id>/<attempt>` branch.
- **testing** — running the repo's test suite.
- **scoring** — evaluating test/lint results against the pass threshold.
- **retrying (attempt N)** — the score fell short; branch rolled back, re-planning with the failure fed into the prompt.
- **success** — passed threshold; carries the branch name and, if `auto_pr` is on, a PR URL.
- **failed** — exhausted the retry budget; carries a reason string.
- **rolled back** — abandoned and reset to the base branch (e.g. a safety-policy violation).
- **conflict** — the diff touched paths outside the task's allowed set; carries the offending paths.

**Drift note:** `LOPI_VS_OPENCLAW.md`'s feature-comparison table cites a
different, older set of transitions (`Planning → Implementing → Testing →
Scoring → OpeningPr → RollingBack`) attributed to an `AgentState` enum. That
enum (and its `AgentRun` companion) has since been removed from
`crates/lopi-core/src/agent.rs` as dead scaffolding — confirmed via git blame
to date to the crate's initial commit, never constructed outside its own
test. `TaskStatus` above is the real, live status type, and always was;
`LOPI_VS_OPENCLAW.md`'s table is still stale and worth correcting separately.
