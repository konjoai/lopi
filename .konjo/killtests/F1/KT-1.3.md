# KT-1.3 — Can a `--bare` checker still grade?

**Verdict: FAIL — but not the failure mode the brief anticipated.** Ship
without `--bare`. Record why, per the brief's own fallback instruction.

## What the brief expected

"Fail" was written to mean: grading agreement between `--bare` and
non-`--bare` drops below 9/10 pairs because the checker needs project
context (`CLAUDE.md`, skills) `--bare` would skip.

## What was actually found

`--bare` does not degrade grading quality in this session — it never got
that far. Every `--bare` invocation failed **authentication**, 6/6 times
across two separate batches spaced apart (not a transient network blip):

```
$ claude -p "say hi" --bare
Authentication error · This may be a temporary network issue, please try again
$ claude -p "say hi"          # same call, no --bare
Hi! What can I help you with today?
```

`claude --help` documents `--bare` as skipping "hooks, LSP, plugin sync,
attribution, auto-memory, background prefetches, **keychain reads**, ...".
This sandboxed session's credential wiring appears to depend on whatever
`--bare` classifies as a keychain read — with it skipped, the CLI cannot
authenticate at all, before any grading quality question is even reachable.
This is a harder, more fundamental failure than "needs project context," and
it made the brief's intended 10-vs-10 paired-agreement comparison impossible
to run: there is no successful `--bare` leg to pair against.

## Caveat — do not over-generalize this finding

This may be an artifact of *this specific container's* credential proxying
(a remote execution environment routing through a session-scoped credential
mechanism `--bare`'s keychain-skip breaks), not a general property of the
`claude` CLI or of subscription auth on a real user's machine, where
credentials more commonly live in a plain on-disk file under `~/.claude/`
rather than behind a keychain-style read `--bare` would skip. **This needs
re-verification on a real target machine (the brief's own "M3, attended"
setup) before being treated as a universal CLI property** — see
`NEXT_SESSION_PROMPT.md`.

## Design consequence

Phase 1's CLI backend does not pass `--bare`, matching the brief's own
"Fail" fallback ("ship without `--bare`") — reached here for a different and
more serious reason than anticipated. The cost/latency this brings (the
checker session loads hooks/CLAUDE.md/skills like a normal session, per
KT-1.2's cost note) is the accepted tradeoff until a real-machine
re-verification either confirms `--bare` is safe to add back or confirms the
auth failure generalizes.
