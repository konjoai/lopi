# KT-S11.2 — is any dashboard render path a genuine XSS sink for agent-supplied text?

**Sprint:** S11, Phase 2 · **Verdict:** NO GENUINE SINK FOUND — the Svelte SSR/XSS
advisories drop to routine dependency hygiene for this app; the one dynamic-content
path that reaches `{@html}` is already sanitized with DOMPurify, and everything else
that carries agent-supplied text renders through Svelte's auto-escaping text
interpolation, not raw HTML injection.

## Method

Per the brief: search `web/src` for the four sink shapes — `{@html}`, `innerHTML`,
dynamic `<svelte:element>`, and spread attributes on untrusted objects — then, for
every hit, trace whether the rendered data can originate from agent output, log
lines, or repo content delivered over the WebSocket.

```
$ grep -rn "{@html" web/src        # 70 hits across 25 files
$ grep -rn "innerHTML" web/src     # 0 hits
$ grep -rn "<svelte:element" web/src   # 0 hits
$ grep -rn "bind:innerText\|bind:textContent\|contenteditable" web/src  # 1 real contenteditable component
$ grep -rn "{\.\.\." web/src       # 0 real hits (one false-positive: a `{...}` inside a code comment)
```

### `innerHTML`, `<svelte:element>`, spread-on-DOM-element

Zero real hits for all three. `web/src/lib/components/ui/badges.ts:34` matches the
`{\.\.\.` grep but it's inside a comment (`// May be pre-rendered JSON like
{"Failed":{...}} from the history table.`), not a spread attribute — no
`<svelte:element>` exists anywhere in the tree, and no DOM element anywhere
spreads a data-derived object onto its attributes. These three sink shapes are
inapplicable to this codebase; the GHSA-m56q-vw4c-c2cp (`<svelte:element>` tag
validation), GHSA-crpf-4hrx-3jrp / GHSA-f7gr-6p89-r883 / GHSA-pr6f-5x2q-rwfp (SSR
spread-attribute XSS) advisories have no reachable code path here regardless of
the installed Svelte version.

### `contenteditable` / `bind:innerText` / `bind:textContent`

One real `contenteditable` element: `web/src/lib/components/stacks/ChipInput.svelte:168`
(the goal/prompt composer — where the *user* types their own task goal, resolving
`:alias`/`@repo`/`;model` tokens into chips). It does not use Svelte's
`bind:innerText`/`bind:textContent` directive (the exact vector in
GHSA-phwv-c562-gvmh) — the only bind is `bind:this={rootEl}` (`ChipInput.svelte:167`,
an element reference, not a two-way text binding). Content is written with the
native DOM `.textContent` setter (`ChipInput.svelte:86,92`: `rootEl.textContent = ''`,
`span.textContent = seg.text`), which the browser always treats as literal text,
never markup — safe independent of the advisory. This is also the *user's own*
local input, not agent-supplied text, so it's not the sink the brief is asking
about even before considering the API used.

### `{@html}` — 70 call sites, 2 distinct data shapes

Grep found 70 `{@html}` occurrences across 25 components. All but two resolve to
one shape: a lookup into a hardcoded icon table (`ICONS.*` in
`web/src/lib/components/stacks/icons.ts`, `SHELL_ICONS.*` in
`web/src/lib/components/icons.ts`) — static, inline, developer-authored SVG
strings, never interpolated with request/response/event data.
`web/src/lib/components/stacks/icons.ts:1-5` states this in its own doc comment
("Rendered via `{@html}` on static, hardcoded strings only (never interpolated
with data)"); tracing every call site (`AppSidebar.svelte:88,97`,
`Dropdown.svelte:143`, `StackCard.svelte` (dozens), `StackControlDock.svelte`,
`ProposalCard.svelte`, `ProvenanceChips.svelte`, `TemplatesMenu.svelte`,
`GoalPopover.svelte`, etc.) confirms each passes an `ICONS.foo` / `SHELL_ICONS.foo`
key, never a field off an `AgentEvent`, `WireMessage`, tool result, or repo/diff
payload. Not a sink.

The two genuine dynamic sinks, both carrying agent-supplied text:

1. **`web/src/lib/components/transcript/Markdown.svelte:26`** —
   `{@html renderProse(seg.md)}`. `seg.md` comes from `splitMarkdown(source)`
   (`web/src/lib/render/markdown.ts:26`), and `source` is
   `block.text` passed in from `Transcript.svelte:48`
   (`<Markdown source={block.text} .../>`), where `block` is a
   `TranscriptBlock` of kind `assistant_text`
   (`web/src/lib/stores/transcript.ts:27`). Tracing how that block's `text` is
   populated (`web/src/lib/stores/transcript.ts`):
   - `case 'log_line': return reduceLogLine(blocks, ev.line, ev.level, id)` (line 168)
     → `reduceLogLine` (line 111) strips known synthetic-status glyph prefixes
     (`🔧`/`💭`/`●`/`⛔`/`🎯🔬📈📐`) and otherwise falls through to
     `appendText(blocks, t, id)` (line 127) — i.e. **plain Claude Code stdout
     lines, unfiltered**, become `assistant_text.text`. `ev.line` is
     `AgentEvent.log_line.line: string` (`web/src/lib/types.ts:48`), sourced
     server-side from the running agent's own output
     (`crates/lopi-agent/src/claude_events.rs`, referenced in this module's
     own doc comment at `transcript.ts:13`).
   - `case 'plan_proposed': return [...sealOpenText(blocks), { kind: 'assistant_text', ..., text: ev.plan, ... }]`
     (line 175) — the agent's proposed plan text, same sink.

   So `seg.md` is directly agent-controlled: a malicious or compromised agent
   process (or a prompt-injected line in tool output that the agent echoes back
   as assistant text) reaches `renderProse` verbatim. **It is already
   sanitized**: `renderProse` (`web/src/lib/render/markdown.ts:73-77`) runs the
   text through `marked.parse` and then
   `DOMPurify.sanitize(raw, { USE_PROFILES: { html: true } })` before returning
   it — DOMPurify strips `<script>`, inline event handlers (`onerror`, `onload`,
   etc.), `javascript:` URLs, and other injection vectors, only allowing the
   `html` profile's safe markup subset. `dompurify` is a direct runtime
   dependency (`web/package.json`), currently pinned `^3.4.11` and installed at
   `3.4.12` post-upgrade (Phase 2 of this sprint bumped it past
   GHSA-c2j3-45gr-mqc4, a `CUSTOM_ELEMENT_HANDLING`/`afterSanitizeElements`
   bypass — irrelevant here since this call site doesn't configure custom
   elements). No unsanitized agent text reaches the DOM through this path.

2. **`web/src/lib/components/transcript/CodeBlock.svelte:67`** —
   `{@html html}` where `html` is the output of
   `web/src/lib/render/highlight.ts:60-68`'s `highlight(code, lang)`, which
   calls Shiki's `hl.codeToHtml(code, ...)`. `code` is fenced-code text split
   out of the same agent-supplied `source` by `splitMarkdown`
   (`markdown.ts:26-59`) — also agent-controlled. Shiki's `codeToHtml` HTML-escapes
   the source text and wraps it in `<span>` tokens for syntax coloring; it does
   not interpret or pass through HTML embedded in the input code (confirmed by
   reading the call site's own comment, `CodeBlock.svelte:65-66`: "Shiki output
   is generated from agent code; it is escaped HTML wrapping the source text (no
   script execution surface)"). No injection surface.

Everything else that carries agent/tool output renders through plain Svelte text
interpolation (auto-escaped), not `{@html}`:
`web/src/lib/components/transcript/ToolCall.svelte:38,48,51` (`{args}`,
`{shown}` — tool args and tool result preview, both directly off
`AgentEvent.tool_call`/`tool_result`, i.e. attacker-reachable if a tool's output
is adversarial) and `Transcript.svelte:56` (`{block.text}` for `thinking`
blocks). Svelte's default text-node interpolation HTML-escapes automatically
(`{expr}` outside `{@html}` is never raw markup), so these are not sinks
regardless of how untrusted the underlying string is.

## Verdict

**No unsanitized XSS sink exists.** The two `{@html}` call sites that ever
receive agent-supplied text — `Markdown.svelte:26` (Claude's assistant text /
proposed plan, via `log_line`/`plan_proposed`) and `CodeBlock.svelte:67`
(fenced code from that same text) — both pass through a sanitizing layer before
reaching the DOM (DOMPurify with the `html` profile, and Shiki's escaping
`codeToHtml`, respectively) rather than injecting raw markup. Every other
`{@html}` call site in the tree is a static, developer-authored SVG string with
no data interpolation. `innerHTML`, dynamic `<svelte:element>` tag names, and
spread attributes onto DOM elements are entirely absent from this codebase.
`bind:innerText`/`bind:textContent` (the specific contenteditable XSS vector in
GHSA-phwv-c562-gvmh) are also absent — the one `contenteditable` element uses
`.textContent` assignment, which is safe by construction.

Consequently, per the brief's own framing: the six Svelte SSR/XSS advisories
(GHSA-crpf-4hrx-3jrp, GHSA-m56q-vw4c-c2cp, GHSA-f7gr-6p89-r883,
GHSA-phwv-c562-gvmh, GHSA-rcqx-6q8c-2c42, GHSA-pr6f-5x2q-rwfp) describe
vulnerable *capabilities* (SSR attribute spreading, dynamic element tags,
contenteditable bindings, DOM-clobbering of internal Svelte state) that this
app's component tree never exercises. Upgrading past them (this sprint bumped
`svelte` from `^4.2.0` to `^5.56.8`, the first patched line) is warranted as
routine dependency hygiene / defense-in-depth — this app ships zero of these
patterns today, so a future component that *does* add a `<svelte:element>` or a
spread-attribute pattern lands on a patched framework rather than a vulnerable
one — but there was no live exploit path to close in the current tree.

## Not covered

This kill-test traced static data flow (grep + manual read of every call site
and its data source) rather than a runtime fuzz/injection probe against a live
`lopi sail` instance. It did not verify DOMPurify's sanitize call against a
crafted payload in a running browser, nor did it check every SvelteKit
server-rendered route for SSR-specific spread-attribute risk outside
`web/src/lib/components` (the app is a static-adapter SPA —
`@sveltejs/adapter-static` in `web/package.json` — so SSR-specific advisories
are further reduced in relevance: `svelte-kit build` prerenders once at build
time from developer-controlled routes, not per-request from agent-supplied
data, meaning even the SSR-scoped advisories have no per-request untrusted-input
surface in this deployment shape). Both gaps are named rather than assumed
closed.
