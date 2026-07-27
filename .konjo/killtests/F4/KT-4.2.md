# KT-4.2 — Does `--session-id` accept lopi's TaskId?

**Verdict: PASS on the mechanism — but the brief's literal proposal
("`--session-id` = the raw `TaskId`") turned out to be unsafe once combined
with lopi's own retry model, so Phase 2 applies the finding differently
than the brief assumed.**

## What was tested

A fresh, never-before-used UUID (`62faafd1-ea12-445a-9961-89ed21a151b8`,
mimicking a lopi `TaskId`'s shape) passed as `--session-id` on a cold spawn
in a worktree, subscription auth, no `--bare`.

## Result

```
$ claude -p "Say hello in one word." --session-id 62faafd1-ea12-445a-9961-89ed21a151b8 ...
init session_id:   62faafd1-ea12-445a-9961-89ed21a151b8
result session_id: 62faafd1-ea12-445a-9961-89ed21a151b8
result: "Hello!"
```

The id round-trips **exactly** into both the `Init` and `Result` events
`StreamEvent::session_id()` already parses — confirmed byte-for-byte, not
approximately. This is the brief's stated pass condition, met.

## Why Phase 2 does NOT set `--session-id` to the raw `TaskId`

The brief's implicit design ("free correlation, use the `TaskId` as the
CLI session id directly") runs into a real collision: `lopi_core::TaskId`
is **stable across every retry attempt** of a task (`run_loop.rs`'s
`for attempt in 0..self.task.max_retries` loop reuses `self.task.id`
unchanged), but Phase 2's own design is "**new attempt means new
session**" (a retried attempt must start cold, per the brief's own Phase 2
constraint). If attempt 1's plan spawn set `--session-id <task_id>` and
attempt 2 (after a failed attempt 1) tried to set `--session-id <task_id>`
again, it would collide with attempt 1's still-addressable session under
the *same* id — untested territory (this kill-test never tried creating
two sessions under one id), and even in the best case it doesn't achieve
what Phase 2 wants (a *fresh*, independent session per attempt).

**Resolution:** `AgentRunner` (via `run_loop.rs`) mints a fresh `Uuid::new_v4()`
**per attempt**, not per task, and uses that as the `--session-id` for the
attempt's plan spawn. Phase 4's correlation is unaffected — the id is still
known before the first spawn even happens (it's lopi's own choice, not
something waited on from the CLI), so it can still be persisted
immediately and still gives `lopi diag`/replay/`transcript_import.rs` a
join key. It's just keyed to `(task_id, most-recent-attempt)` rather than
being the `task_id` itself — `tasks.cli_session_id` is overwritten on each
new attempt, matching `tasks.branch`'s existing "only the most recent
attempt is addressable" precedent (`set_task_branch`).

## Bearing on the sprint

The brief's own contingency line covers this outcome almost exactly: *"Fail:
fall back to capturing the CLI-assigned UUID from `Init`. Costs a
persistence step, not the sprint."* What actually happened is a hybrid:
the mechanism **passed** (no fallback needed for *that*), but the
*application* of it needed one added constraint (per-attempt, not
per-task, id) that the brief didn't anticipate needing. Net effect on
scope: identical to the brief's own stated fallback — one extra design
decision, not a blocked sprint.
