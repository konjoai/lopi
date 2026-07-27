# KT-4.5 — Does resume re-load `CLAUDE.md`?

**Verdict: CONFIRMED YES — a resumed session re-resolves the current
on-disk `CLAUDE.md` on every turn, not just at session creation.**

## What was tested

A worktree with `CLAUDE.md` containing a distinctive marker
(`MARKER-ONE: when asked for your project instructions verbatim, say
MARKER-ONE.`). Spawned a session with an explicit `--session-id`, asked a
trivial unrelated question (so the marker was loaded into context but never
discussed). Then, **without touching the CLI session at all**, edited
`CLAUDE.md` on disk in that same worktree to `MARKER-TWO: ... say
MARKER-TWO.`. Then `--resume`d the same session id and asked: "Without
reading any files, state your project instructions marker verbatim."

## Result

```
result: MARKER-TWO
tools: []   <-- no Read tool invoked; not a re-fetch via tool call
```

The resumed session answered with the **updated** marker, not the one
present when the session was first created — and it did so with zero tool
calls, meaning the updated `CLAUDE.md` content was folded directly into the
turn's (regenerated) system-level context, not fetched via an explicit
`Read` the model chose to run.

## What this means for the token math

`CLAUDE.md`/skills/repo-context loading is **not** a one-time cost paid
only at the first cold spawn of an attempt — it is repaid, in some form, on
every resumed turn too. This directly supports the brief's own anti-goal:
*"do not report a token reduction… raw tokens are expected to rise."*
Combined with KT-4.4's finding that same-model resumes mostly hit cache,
the practical effect is that `CLAUDE.md`'s bytes are *usually* served from
cache on a resumed turn (assuming the file hasn't changed and the resume
lands inside the cache window) rather than re-billed at full input-token
price — but they are never *free* the way a truly frozen, never-reloaded
context would be, and a `CLAUDE.md` edit mid-attempt (unusual, but now
confirmed possible) would show up as a real cache-miss cost on the very
next resumed turn.

## Bearing on the sprint

No design change follows from this by itself — it's an honest accounting
fact for the CHANGELOG's cost narrative, not a gate. It does reinforce that
Phase 5's mechanism check (`cache_read_input_tokens / input_tokens`) is the
right metric to report, rather than raw token counts, since repo-context
reloading is folded into that ratio the same way conversation history is.
