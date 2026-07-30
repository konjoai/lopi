# Census B: streaming path

Traced hop by hop from the Claude Code child process to a rendered pixel, with
code quotes from `RECON_REF` (`043ca18`). This is the sprint's stated priority
open technical question: is Sprint U a CSS sprint or a plumbing sprint?

**Answer: both, but the plumbing problem is the bigger one, and it is proven
in code, not inferred.** The CLI is invoked in genuine incremental-streaming
mode and read line-by-line: hop 1 and hop 2 are exactly what you'd want. But
hop 3 throws the incremental granularity away: partial-message deltas are
explicitly discarded, and the UI only ever sees one atomic chunk per complete
assistant turn. No amount of CSS/animation work at hop 4/5 can make text
"stream smoothly" when hop 3 hands it a full paragraph at once.

## Hop 1: invocation

`crates/lopi-agent/src/claude_spawn.rs`, `run_streamed_once` (the path behind
every live-streaming pane on the Loop Stacks page):

```rust
cmd.arg("-p")
    .arg(prompt)
    .arg("--output-format")
    .arg("stream-json")
    .arg("--verbose")
    .arg("--include-partial-messages");
```

This **is** incremental streaming mode, not a buffered call that returns on
completion: `--include-partial-messages` explicitly asks the CLI for
per-token/per-chunk `content_block_delta` events, not just block-complete
messages. Confirmed live in `artifacts/STREAM_CAPTURE.jsonl` (a real capture
checked into the repo): of 44 raw NDJSON lines across 3 turns, **14** are
`stream_event/content_block_delta`, proving the CLI genuinely emits granular
deltas over the wire.

## Hop 2: stdout read

Same function, immediately after spawn:

```rust
let mut lines = AsyncBufReader::new(stdout).lines();
...
loop {
    match tokio::time::timeout_at(deadline, lines.next_line()).await {
        Ok(Ok(Some(line))) => {
            let mut hard_stop = false;
            for ev in parse_line(&line) { ... }
```

Line-buffered (`AsyncBufReader::lines()`), not `read_to_string`. Stdout is
consumed and dispatched line-by-line as it arrives, not accumulated and
processed at process exit. Good.

## Hop 3: event bus granularity, the finding

`crates/lopi-agent/src/claude_events.rs`:

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

`content_block_delta` (the exact per-token/per-chunk event `--include-partial-messages`
was requested to get) is **matched and silently dropped** (`Vec::new()`).
The only non-delta path that produces visible text is the complete envelope:

```rust
fn parse_assistant(v: &Value) -> Vec<StreamEvent> {
    let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) else {
        return vec![StreamEvent::Other];
    };
    let out: Vec<StreamEvent> = blocks.iter().filter_map(parse_assistant_block).collect();
    ...
}
```

`type: "assistant"` messages carry the **complete** content block (arrives
once the model has finished generating that block), and `parse_assistant_block`
turns each complete `text`/`thinking` block into one `StreamEvent::Text`/
`StreamEvent::Thinking`. So:

- **Granularity is per complete assistant turn/block, not per token, not even
  per line.** A four-paragraph response is one `StreamEvent::Text` carrying
  all four paragraphs, emitted only once the model has finished the entire
  block.
- `--include-partial-messages` is requested from the CLI and paid for
  (larger stream, more parsing work) but its entire payload (the granular
  deltas) is thrown away before it ever reaches the event bus.

This is not a CSS problem. No amount of `transition`/animation work at hops
4-5 changes the fact that hop 3 hands the UI one big blob per turn.

## Hop 4: transport to browser, flush granularity

`crates/lopi-ui/src/web/event_bridge.rs`:

```rust
loop {
    match rx.recv().await {
        Ok(ev) => {
            let ev = redact_log_line(ev);
            if let Ok(json) = serde_json::to_string(&ev) {
                let _ = tx.send(Arc::from(json.as_str()));
            }
            ...
```

No batching, no debounce: each `AgentEvent` off the bus is serialized and
broadcast individually, immediately, over the shared `broadcast::Sender`
that both `/ws` and `/sse` subscribers read from. Flush granularity here is
"as fast as hop 3 produces events"; this hop is not the bottleneck, since hop 3
already coarsened everything before this code runs.

## Hop 5: client render, append vs. full re-render

`web/src/lib/stores/transcript.ts`:

```ts
function appendText(blocks: TranscriptBlock[], line: string, id: string): TranscriptBlock[] {
  const open = openText(blocks);
  if (open) {
    const next = blocks.slice();
    next[next.length - 1] = { ...open, text: `${open.text}\n${line}` };
    return next;
  }
  return [...blocks, { kind: 'assistant_text', id, text: line, streaming: true }];
}
```

This is an append/extend reducer: a new `log_line` either extends the
currently-open text block (string concatenation) or opens a new one. Svelte's
keyed `{#each}` over `blocks` means only the changed/new block re-renders:
**not** a full-transcript re-render per event. Hop 5 is fine; it was never
where the problem lives.

## Measurement on S4 (this sprint's synthetic fixture)

The recon harness cannot invoke a live `claude` CLI process (no
`ANTHROPIC_API_KEY`/network egress for a real agent run in this sandboxed
recon environment, and doing so would violate the read-only/no-side-effects
scope of this sprint). Two measurements instead:

**Real capture, `artifacts/STREAM_CAPTURE.jsonl`** (checked into the repo,
built from an actual `claude -p --output-format stream-json` run): 44 lines,
3 assistant turns. Of the 4 `assistant` (complete) messages, one carried a
126-character text block and one an 81-character thinking block (the others
were tool-use blocks). Those 126/81 characters each arrive as **one** DOM
mutation via `appendText`/`appendThinking`, aggregating what were 14 raw
`content_block_delta` events in the same capture. The sample is small (one
real conversation, 3 turns): not enough to responsibly report a p95 across a
distribution; a longer production capture would be needed for that. What the
sample **does** prove unambiguously is the mechanism: N incremental deltas
collapse into 1 mutation, and there is no cap on that mutation's size: a
long unstructured response (the kind Claude produces regularly: a multi-step
plan, a large diff explanation) would arrive as one multi-hundred- or
multi-thousand-character mutation, identically.

**This sprint's S4 fixture** (`tools/recon/fixtures/states.js`): scripted
`log_line` frames average ~35-45 characters, but these are hand-authored
fixture strings for taking a screenshot, not a measurement of production
behavior, and are **not** used as a stand-in for the real number above.

- Characters per DOM mutation: **no upper bound at hop 3**, bounded only by
  how much text the model generates in one block before the CLI's own
  message-complete boundary. Median/p95 **not established** from this
  sprint's evidence (sample too small); the code-level fact (no
  incremental delta ever reaches the UI) is established with certainty.
- DOM mutations per second during active streaming: bounded by *assistant
  turn* frequency, not token frequency. For a model that "thinks" for several
  seconds before emitting a complete text block, this can be **zero visible
  updates for multi-second stretches**, then one large jump.
- Does the whole log region re-render, or only append? **Only append/extend**
  (hop 5, confirmed above); this was never the bottleneck.

## Bottom line

**Hop 1/2 are correct incremental-streaming plumbing. Hop 3 discards the
incremental granularity that hops 1/2 paid to get, and hops 4/5 faithfully
forward whatever coarse granularity hop 3 hands them.** This matches the
"text arrives in big blocks" complaint exactly, and it is a plumbing fix
(stop discarding `content_block_delta` in `parse_stream_event`, thread a
partial-text event through `StreamEvent`, wire a new `AgentEvent` variant,
handle it in `transcript.ts`), not a CSS/animation fix. Per the standing
instruction: if hop 1 had come back buffered, this would already be its own
sprint ahead of the visual work. It didn't (hop 1 is fine), but hop 3's
finding carries the same weight: **Sprint U's smooth-streaming goal is
blocked on a `lopi-agent` change, not a `lopi-ui` CSS change.**
