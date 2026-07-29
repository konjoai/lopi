# Census B — The Streaming Path

The open question: is lopi actually streaming Claude Code's output, or waiting for it
and dumping the result? Short answer: **it is genuinely streaming — invocation, process
read, and the wire are all fine-grained.** The coarseness the human ranked #2
("block-appearing text") is real, but it is introduced deliberately at hop 3, in lopi's
own NDJSON parser, not anywhere downstream. This is the single most load-bearing finding
in this report for planning Sprint U: **it is a one-function fix, not a plumbing sprint.**

## Hop 1 — invocation

Streaming, not buffered. `crates/lopi-agent/src/claude_spawn.rs`, `run_streamed_once`:

```rust
cmd.arg("-p")
    .arg(prompt)
    .arg("--output-format")
    .arg("stream-json")
    .arg("--verbose")
    .arg("--include-partial-messages");
```

`--include-partial-messages` is the CLI flag that asks for incremental `content_block_delta`
events as assistant text is generated, not just the final coalesced blocks. lopi already
asks for the fine-grained stream. What happens to it is hop 3's problem, not hop 1's.

(The one-shot, non-streamed path exists too — `run_once`, used by `fix()` and
`implement_step()` — but the path that backs every `lopi run`/loop-runner implement and
plan step, `stream_plan`/`stream_implement` in `crates/lopi-agent/src/runner/stream.rs`,
is always the streamed one.)

## Hop 2 — process read

Line-buffered, incremental — not `read_to_string`. Same function, a few lines down:

```rust
let mut lines = AsyncBufReader::new(stdout).lines();
...
loop {
    match tokio::time::timeout_at(deadline, lines.next_line()).await {
        Ok(Ok(Some(line))) => {
            for ev in parse_line(&line) { ... }
        }
        ...
    }
}
```

Each NDJSON line is handed to the decoder the moment it arrives on stdout. No buffering
beyond one line. This hop is fine.

## Hop 3 — the event bus, and where the coarseness actually comes from

This is the hop that matters. `crates/lopi-agent/src/claude_events.rs`'s `parse_line`
decodes one line into zero or more `StreamEvent`s, and `parse_stream_event` handles the
CLI's own incremental `stream_event` envelope:

```rust
fn parse_stream_event(v: &Value) -> Vec<StreamEvent> {
    let Some(event) = v.get("event") else {
        return vec![StreamEvent::Other];
    };
    match event.get("type").and_then(Value::as_str) {
        Some("message_delta") => vec![parse_usage(event.get("usage"))],
        _ => Vec::new(), // block deltas are coalesced into the assistant line
    }
}
```

Read that literally: every `stream_event` whose inner type is *not* `message_delta` —
which includes `content_block_delta`, the event that carries the actual incremental
text/thinking characters as the model generates them — is thrown away (`Vec::new()`).
Only the usage numbers survive from the incremental stream. The assistant's visible text
reaches the UI exclusively through `parse_assistant`, which reads the **final, complete**
`assistant` line and its whole `message/content` block array:

```rust
fn parse_assistant(v: &Value) -> Vec<StreamEvent> {
    let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) else {
        return vec![StreamEvent::Other];
    };
    let out: Vec<StreamEvent> = blocks.iter().filter_map(parse_assistant_block).collect();
    ...
}
```

So the granularity actually emitted onto lopi's `AgentEvent` bus is **one event per
complete content block per assistant line** (a `Text`, a `Thinking`, or a `ToolUse` block)
— not per token, not per character-delta, and coarser than the CLI is actually offering.
`--include-partial-messages` is requested at hop 1 and discarded at hop 3. This is not
ambiguous — the comment in the code (`block deltas are coalesced into the assistant
line`) states the decision plainly; it reads as a deliberate simplification (the fuzz
target's own doc calls `parse_line` "the `claude` CLI's `--output-format stream-json`
line parser," built from a real capture, not from the incremental-delta spec), not an
accident, but it is the direct cause of "text arrives in big blocks."

`ToolResult`, `RateLimit`, and the terminal `Result` envelope are each their own event too
(`parse_user`, `parse_rate_limit`, `parse_result`) — same one-shot-per-line shape, not
relevant to the text-coarseness question but confirming the bus's general granularity is
"one event per structurally-complete thing," never a sub-block delta.

## Hop 4 — transport to the browser

No batching, no debounce — every `AgentEvent` is serialized once and rebroadcast
immediately, to every subscriber, the instant it's received. `crates/lopi-ui/src/web/event_bridge.rs`:

```rust
tokio::spawn(async move {
    loop {
        match rx.recv().await {
            Ok(ev) => {
                let ev = redact_log_line(ev);
                if let Ok(json) = serde_json::to_string(&ev) {
                    let _ = tx.send(Arc::from(json.as_str()));
                }
                ...
            }
            ...
        }
    }
});
```

Both the global WS (`streaming::handle_ws`) and the per-task SSE
(`task_stream_handlers::stream_task`) read off this same pre-serialized broadcast (or the
raw bus, filtered by task id, for the per-task SSE) and `send`/write a WS text frame or an
SSE `data:` line per event, synchronously, with no `setInterval`/coalescing anywhere in
this path. Flush granularity: one network message per `AgentEvent`. This hop is fine.

## Hop 5 — client to DOM

`web/src/lib/stores/wsClient.ts`'s `ws.onmessage` parses and dispatches every frame the
instant it arrives — no client-side batching either:

```ts
ws.onmessage = (e) => {
  if (!messageHandler) return;
  try {
    const raw = JSON.parse(e.data);
    messageHandler(raw);
  } catch {
    console.debug('[lopi] dropped non-JSON frame');
  }
};
```

`web/src/lib/stores/transcript.ts` does not append a new DOM node per event for
continuing text — a `log_line` for an already-open text/thinking block **mutates that
block's `.text` field in place** (`open.text + '\n' + line`), and `Transcript.svelte`/
`StackOutput.svelte` key their `{#each blocks as block (block.id)}` list by a stable id,
so Svelte patches the existing node's text content rather than re-rendering the list.
Net effect: the DOM mutation this hop produces per event is a `characterData` (or small
`childList`) change to one existing node, not a full-region re-render — this hop is also
fine, and is not where the "whole log region re-renders" complaint (if real) would come
from; Census C's MutationObserver data (below) is the actual check on that.

## Measured, on S4 (synthetic upstream, real production frontend/backend)

Hops 1–3 above are traced from static code, not live-measured — recon never invoked a
real `claude` subprocess (no API key, no billing, out of scope for a read-only pass). The
`AgentEvent`s Census B measures below are produced by `tools/recon/fixture-server`'s
deterministic pump (see `recon/LEDGER.md`), not a real Claude Code session — but they
flow through the **real, unmodified** `event_bridge`, `streaming::handle_ws`, `wsClient.ts`,
and `transcript.ts`/`StackOutput.svelte` code, so hops 4 and 5 are genuinely measured
against production code, just with a synthetic (documented, fixed-cadence) source instead
of hop 1–3's real CLI. Where this matters: the pump's own chunk sizes are a deliberate
proxy for "coalesced full text-block" content per hop 3's finding above (short natural-
language phrases, not single tokens) — the actual numbers below reflect that proxy
distribution, not a real Claude session's block sizes, which will vary with what the
model actually writes.

- DOM mutations per second during active streaming: `{{S4_MUTATIONS_PER_SEC}}`
- Characters per DOM mutation — median: `{{S4_MEDIAN_CHARS}}`, p95: `{{S4_P95_CHARS}}`,
  max observed: `{{S4_MAX_CHARS}}` (over a {{S4_WINDOW_MS}}ms window,
  {{S4_TOTAL_MUTATIONS}} total mutations)
- Whole-region re-render vs targeted append: `{{S4_RERENDER_FINDING}}`

See `recon/census_layout.json` for the CLS measurement over the same window, and
`recon/shots/S4_streaming_30s.webm` for the actual video.
