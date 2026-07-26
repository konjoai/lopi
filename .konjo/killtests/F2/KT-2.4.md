# KT-2.4 — Is there a Claude-accurate token count without an API key?

**Sprint:** F2 · **Verdict:** FAIL for `estimate_tokens`'s actual use case → **Phase 5 relabels rather than replaces.**

## Method

`estimate_tokens` (`crates/lopi-context/src/tokens.rs`) is called *before* a
message is sent — it estimates the size of content already in hand so
`ContextWindow` can decide whether to evict before adding more, and so
`token_pressure()` can report a live, continuously-updated ratio. Any
replacement has to serve that same pre-send role.

Two candidate keyless (no `ANTHROPIC_API_KEY`) sources were checked:

1. **A published offline Claude tokenizer.** None exists as an installable
   artifact for lopi to vendor — Anthropic's own guidance (`claude-api` skill,
   `shared/token-counting.md`) is explicit: *"Do not use `tiktoken`... any
   estimate from `tiktoken`, `gpt-tokenizer`, or similar is wrong for
   Claude,"* and the only sanctioned accurate path is the
   `POST /v1/messages/count_tokens` API endpoint — which requires a key, the
   exact gap this kill-test exists to check.
2. **A CLI-reported count in stream events.** lopi's `claude` CLI subprocess
   path *does* receive a real, Claude-accurate token count with no
   `ANTHROPIC_API_KEY` of its own (the CLI authenticates via the user's
   subscription) — `claude_events::parse_result_usage`'s `modelUsage`/`usage`
   fields, already flowing into `UsageAccrual` and `TurnMetrics`. This is
   real and keyless. **But it arrives only after a turn completes** — it
   cannot serve `estimate_tokens`'s actual call site, which runs *before* the
   content is sent, to decide what to evict pre-send.

## Verdict

**No keyless path exists for the specific job `estimate_tokens` does** (a live
pre-send estimate for eviction/pressure). A keyless *post-hoc* Claude-accurate
count exists and is already captured in `TurnMetrics`/`UsageAccrual` for
completed turns — but that's a different job than the one `cl100k_base` is
standing in for, and using it wouldn't fix the live gauge.

**Consequence — Phase 5 relabels, per the brief's own fallback branch:**
`token_pressure` in the UI and `turn_metrics.context_pressure` are marked as
estimates with the instrument (`cl100k_base` — OpenAI's GPT-4 BPE, not a
Claude tokenizer) named wherever the number is displayed. F0's TOON benchmark
is not re-run against a new instrument, because there is no new instrument —
the existing `cl100k_base` measurement stands, honestly labeled, same as F0
left it.
