<!--
  Transcript — the chat body. Renders an ordered block list (assistant markdown,
  collapsible thinking, tool-call accordions, status chips) and keeps the view
  pinned to the tail while streaming, pausing auto-scroll the moment the user
  scrolls up to read back (a "jump to latest" affordance returns them).
-->
<script lang="ts">
  import { afterUpdate, tick } from 'svelte';
  import type { TranscriptBlock } from '$lib/stores/transcript';
  import Markdown from './Markdown.svelte';
  import ToolCall from './ToolCall.svelte';
  import StatusChip from './StatusChip.svelte';

  export let blocks: TranscriptBlock[] = [];
  export let streaming = false;
  /** Reserved bottom-right inset (px) so text never collides with the orb. */
  export let orbInset = 0;

  let scroller: HTMLDivElement;
  let pinned = true;
  let thinkingOpen = false;

  function onScroll() {
    if (!scroller) return;
    const gap = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight;
    pinned = gap < 48;
  }

  async function toTail() {
    pinned = true;
    await tick();
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
  }

  afterUpdate(() => {
    if (pinned && scroller) scroller.scrollTop = scroller.scrollHeight;
  });
</script>

<div class="transcript" bind:this={scroller} on:scroll={onScroll}>
  {#if blocks.length === 0}
    <div class="empty">— waiting for output —</div>
  {/if}

  {#each blocks as block (block.id)}
    {#if block.kind === 'assistant_text'}
      <div class="row assistant">
        <Markdown source={block.text} streaming={block.streaming && streaming} />
      </div>
    {:else if block.kind === 'thinking'}
      <div class="row thinking">
        <button type="button" class="think-toggle" on:click={() => (thinkingOpen = !thinkingOpen)}>
          <span class="chev" class:open={thinkingOpen}>▸</span> thinking
        </button>
        {#if thinkingOpen}
          <pre class="think-body">{block.text}</pre>
        {/if}
      </div>
    {:else if block.kind === 'tool_call'}
      <div class="row"><ToolCall tool={block.tool} args={block.args} result={block.result} /></div>
    {:else if block.kind === 'status'}
      <div class="row"><StatusChip tier={block.tier} label={block.label} /></div>
    {/if}
  {/each}

  <!-- A float at the tail of the flow gives the bottom-right orb a circular
       footprint for text to wrap around (shape-outside). Height = orb + margin. -->
  {#if orbInset > 0}
    <div class="orb-wrap" style:--orb-inset={`${orbInset}px`}></div>
  {/if}
</div>

{#if !pinned}
  <button type="button" class="jump" on:click={toTail} title="Jump to latest">↓ latest</button>
{/if}

<style>
  .transcript {
    position: relative;
    height: 100%;
    overflow-y: auto;
    padding: 0.85rem 1rem 1rem;
    scroll-behavior: smooth;
  }
  @media (prefers-reduced-motion: reduce) {
    .transcript {
      scroll-behavior: auto;
    }
  }
  .empty {
    opacity: 0.3;
    font-style: italic;
    font-family: var(--font-mono, monospace);
    font-size: 0.75rem;
  }
  .row {
    margin-bottom: 0.15rem;
  }
  .assistant {
    margin-bottom: 0.4rem;
  }
  .thinking {
    opacity: 0.6;
  }
  .think-toggle {
    font-family: var(--font-mono, monospace);
    font-size: 0.68rem;
    opacity: 0.6;
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }
  .think-toggle:hover {
    opacity: 1;
  }
  .chev {
    transition: transform var(--dur-fast) var(--ease-out-expo);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .think-body {
    margin: 0.25rem 0 0;
    padding-left: 0.9rem;
    font-family: var(--font-mono, monospace);
    font-size: 0.7rem;
    white-space: pre-wrap;
    word-break: break-word;
    opacity: 0.7;
    border-left: 1px solid rgba(255, 255, 255, 0.12);
  }
  /* Right-floated circle at the flow's tail — `shape-outside` reflows the
     transcript text around the orb sitting in the bottom-right corner. */
  .orb-wrap {
    float: right;
    width: var(--orb-inset);
    height: var(--orb-inset);
    shape-outside: circle(50%);
    -webkit-shape-outside: circle(50%);
    margin: 0.4rem 0 0 0.4rem;
    pointer-events: none;
  }
  .jump {
    position: absolute;
    bottom: 0.6rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 5;
    font-family: var(--font-mono, monospace);
    font-size: 0.68rem;
    color: var(--konjo-black);
    background: var(--konjo-ice);
    padding: 0.2rem 0.7rem;
    border-radius: 999px;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.4);
    opacity: 0.92;
  }
  .jump:hover {
    opacity: 1;
  }
</style>
