# KT-4.3 — Does `--model` still apply on resume? (BLOCKING for the design)

**Verdict: PASS on the flag itself, but with a load-bearing mechanism
finding the brief didn't anticipate — and a clean resolution that needs no
new guard code.**

## What was tested

Resumed a session (`--resume <id>`) while also passing `--model
claude-haiku-4-5-20251001` — a different model from whatever the original
spawn used implicitly — and inspected both the `Init` event's `model`
field and the raw `usage` envelope (`cache_read_input_tokens` /
`cache_creation_input_tokens`) across three chained calls:

1. Cold spawn, default model. `cache_read=70730`, `cache_creation=35669`
   (first-ever call; the nonzero `cache_read` here is the CLI's own
   static system-prompt/tool-definitions cache, shared across sessions,
   not a signal about *this* session specifically).
2. `--resume <id> --model claude-haiku-4-5-20251001` (a model switch).
   `Init.model` reports `claude-haiku-4-5-20251001` — **the flag applies**.
   But: `cache_read_input_tokens=0`, `cache_creation_input_tokens=26944` —
   a **complete cache miss** on this turn, despite the conversation content
   being otherwise unchanged.
3. `--resume <id> --model claude-haiku-4-5-20251001` again (same model as
   call 2, no switch this time). `cache_read_input_tokens=26944` (exactly
   matching call 2's `cache_creation_input_tokens`), `cache_creation=183` —
   a near-total cache **hit**.

## What this means

`--model` does apply on resume — confirmed, the brief's stated pass
condition. But **switching model on a resumed call forces a full cache
miss for that turn**, because Anthropic's prompt cache is keyed by model as
well as content. Call 2 paid full input-token price for the entire
conversation history; call 3 (same model, same history) got it almost
entirely from cache. This is a real, previously undocumented-in-this-repo
mechanism finding: resuming under a *changed* model is not "cheaper, just
with a different model" — it can be as expensive as a cold spawn, or worse,
since the CLI still has to re-tokenize and cache-write the whole history
under the new model's cache namespace.

## Why this doesn't need new guard code

The brief's worry was that `select_model`'s retry-escalation (attempt ≥ 2
routes to Opus, `claude_model.rs::select_model`) might silently "stop
working" under a resumed session, or that resuming across an escalation
boundary might quietly re-pin the model. Reading `run_loop.rs` confirms
neither risk applies here, for a reason that falls out of the *existing*
code, not new code this sprint had to add:

- `select_model(&self.task, attempt)` is called **once per attempt**
  (`run_loop.rs`, top of the attempt loop), and the resulting `model`
  variable is threaded unchanged through plan → implement → fix within
  that attempt.
- Phase 2's own design rule is "new attempt means new session" — a cold
  spawn at exactly the point escalation could change the model.

So a single attempt's session **never** sees a mid-session model switch —
escalation only ever changes `model` at an attempt boundary, and that
boundary is already a cold-spawn boundary by design. The costly case this
kill-test found (switching model *within* a resumed session) simply never
occurs in lopi's actual call pattern. No additional model-pinning check was
added to `apply_cli_caps`/`ClaudeCode` — correctness here is a consequence
of Phase 2's attempt-scoped session design, not a separate guard.

## Bearing on the sprint

Confirms the "Design around this before writing code, not after" mandate
was already satisfied by the attempt-scoped session choice made for other
reasons (Phase 2's "one session per attempt" framing) — this kill-test's
job was to verify that choice actually closes the gap, which it does.
