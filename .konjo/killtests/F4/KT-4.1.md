# KT-4.1 — Does resume survive lopi's actual spawn conditions? (BLOCKING)

**Verdict: PASS, with one environment-specific caveat on `bypassPermissions` itself.**

## What was tested

Live, attended, real `claude` CLI (subscription auth, no `ANTHROPIC_API_KEY`
set), against everything lopi actually sets at once at a spawn site:

- cwd is a git **worktree** (`git worktree add`), not the main checkout —
  matches `run_loop.rs`'s per-attempt branch worktree.
- `--permission-mode` present.
- `--bare` **absent** — worker sessions load repo context (F2 Phase 6),
  confirmed still true post-F1.
- Subscription auth, no API key (`ANTHROPIC_API_KEY` unset in this
  container; `CLAUDE_CODE_SESSION_ID`/`CLAUDE_CODE_CHILD_SESSION`/
  `CLAUDE_CODE_REMOTE_SESSION_ID` explicitly scrubbed from the child env —
  see the environment-confound finding below).

Setup: a scratch git repo (`secret.txt` containing a marker passphrase,
`main.rs`) with a worktree checked out from it. Call 1 spawned cold in the
worktree, asked to read both files and report the passphrase. Call 2
resumed that exact session id (`--resume <id>`) in the same worktree, asked
to report the passphrase **and** what `main.rs` prints, explicitly
instructed not to re-read anything.

## Result

Call 2's tool-call stream (inspected directly from the decoded
`stream-json` events, not by asking the model) shows **zero `tool_use`
events** — it answered both questions correctly from retained context. Call
1's session id was echoed back unchanged in call 2's `Init`/`Result`
events. This is the pass condition the brief specifies verbatim: "the
resumed session retains prior context and does not re-read files the first
session already read," verified against the tool-call stream.

## The one thing that did NOT pass as specified

`--permission-mode bypassPermissions` itself could not be verified in this
sandboxed container: it maps internally to the same check
`--dangerously-skip-permissions` uses, which **refuses to run under root**
("`--dangerously-skip-permissions` cannot be used with root/sudo
privileges for security reasons") — this container runs as root. Every
live call in this kill-test corpus (KT-4.1 through KT-4.5 and the Phase 5
benchmark) substitutes `--permission-mode acceptEdits` to exercise the same
headless, no-prompt-stall mechanics `bypassPermissions` provides.

This is the same class of finding as F1's KT-1.3 (`--bare` failing auth in
this same sandboxed session) — **not assumed to generalize**. A real
non-root deployment (lopi's actual target) should not hit this; whoever
next has a real non-root machine with a logged-in `claude` CLI should
re-run this specific check (`--permission-mode bypassPermissions` in a
worktree, non-root) to confirm resume behaves identically there. Nothing
in this sprint's design depends on `bypassPermissions` specifically over
`acceptEdits` — both are headless-safe, non-stalling modes — so this
caveat affects only the completeness of KT-4.1's own verification, not the
session-continuity mechanism itself.

## A confound found and fixed along the way

The *first* unscrubbed attempt (before adding `CLAUDE_CODE_SESSION_ID` to
the env-scrub list) returned this outer session's own id as the spawned
child's session id — a nested Claude Code session's `CLAUDE_CODE_SESSION_ID`
env var silently overrides the CLI's own fresh-UUID assignment. lopi's
`scrub_inherited_anthropic_env` (`claude_support.rs`) did not scrub this
before Sprint F4; it does now (Phase 1). See `CHANGELOG.md`.

## Raw evidence

```
Call 1 (cold): session_id=35faaa8b-8553-4b16-a67e-348c1fac42ff
  tools: Bash, Read(secret.txt), Read(main.rs)
  result: "PASSPHRASE=BLUEBERRY-42"

Call 2 (--resume 35faaa8b-...): session_id=35faaa8b-8553-4b16-a67e-348c1fac42ff (unchanged)
  tools: []  <-- zero tool calls, confirms retained context
  result: "BLUEBERRY-42\nhi"  (both facts correct, no re-read)
```

## Bearing on the sprint

Phase 1/2 proceed as designed. The `--permission-mode` caveat is recorded
in `NEXT_SESSION_PROMPT.md` for whoever has a real non-root machine to
re-verify.
