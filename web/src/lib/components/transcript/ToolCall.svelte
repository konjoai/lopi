<!--
  ToolCall — a Claude-Code-style tool invocation: one summary line (glyph + tool
  name + short args) that is collapsed by default and expands to show the full
  args and the (truncated, show-more) tool result. An errored result rims red.
-->
<script lang="ts">
  export let tool = '';
  export let args = '';
  export let result: { preview: string; isError: boolean } | undefined = undefined;

  let open = false;
  let showFull = false;

  /** A small glyph per tool family — purely decorative, never load-bearing. */
  const GLYPHS: Record<string, string> = {
    Bash: '$',
    Read: '◰',
    Write: '✎',
    Edit: '✎',
    Glob: '⌕',
    Grep: '⌕',
    WebFetch: '↯',
    WebSearch: '⌕'
  };
  $: glyph = GLYPHS[tool] ?? '🔧';

  const TRUNCATE = 1200;
  $: preview = result?.preview ?? '';
  $: long = preview.length > TRUNCATE;
  $: shown = long && !showFull ? preview.slice(0, TRUNCATE) : preview;
</script>

<div class="tool" class:err={result?.isError}>
  <button type="button" class="tool-head" on:click={() => (open = !open)} aria-expanded={open}>
    <span class="chev" class:open>▸</span>
    <span class="glyph">{glyph}</span>
    <span class="name">{tool}</span>
    {#if args}<span class="args">{args}</span>{/if}
    {#if result}
      <span class="dot" class:bad={result.isError} title={result.isError ? 'error' : 'ok'}></span>
    {:else}
      <span class="spinner" title="running">⟳</span>
    {/if}
  </button>
  {#if open}
    <div class="tool-body">
      {#if args}
        <div class="kv"><span class="k">args</span><code>{args}</code></div>
      {/if}
      {#if result}
        <pre class="result">{shown}{#if long && !showFull}…{/if}</pre>
        {#if long}
          <button type="button" class="more" on:click={() => (showFull = !showFull)}>
            {showFull ? 'show less' : `show more (${preview.length - TRUNCATE} more chars)`}
          </button>
        {/if}
      {:else}
        <div class="pending">running…</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tool {
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    margin: 0.4rem 0;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.02);
  }
  .tool.err {
    border-color: color-mix(in srgb, var(--konjo-rose) 45%, transparent);
  }
  .tool-head {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.35rem 0.6rem;
    font-family: var(--font-mono, monospace);
    font-size: 0.72rem;
    text-align: left;
    transition: background var(--dur-fast) var(--ease-out-expo);
  }
  .tool-head:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .chev {
    opacity: 0.5;
    transition: transform var(--dur-fast) var(--ease-out-expo);
    flex-shrink: 0;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .glyph {
    opacity: 0.8;
    flex-shrink: 0;
  }
  .name {
    font-weight: 600;
    color: var(--konjo-ice);
    flex-shrink: 0;
  }
  .args {
    opacity: 0.6;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--konjo-jade);
    flex-shrink: 0;
    margin-left: auto;
  }
  .dot.bad {
    background: var(--konjo-rose);
  }
  .spinner {
    margin-left: auto;
    opacity: 0.6;
    flex-shrink: 0;
    animation: spin 1.1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }
  .tool-body {
    padding: 0.4rem 0.6rem 0.55rem;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    font-family: var(--font-mono, monospace);
    font-size: 0.7rem;
  }
  .kv {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 0.35rem;
  }
  .k {
    opacity: 0.4;
    text-transform: uppercase;
    font-size: 0.6rem;
    letter-spacing: 0.06em;
    padding-top: 0.1rem;
    flex-shrink: 0;
  }
  .kv code {
    opacity: 0.85;
    word-break: break-all;
  }
  .result {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 18rem;
    overflow-y: auto;
    opacity: 0.85;
    line-height: 1.45;
  }
  .more {
    margin-top: 0.35rem;
    font-size: 0.65rem;
    color: var(--konjo-ice);
    opacity: 0.75;
  }
  .more:hover {
    opacity: 1;
  }
  .pending {
    opacity: 0.5;
    font-style: italic;
  }
</style>
