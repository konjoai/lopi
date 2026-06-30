<!--
  Markdown — renders a GitHub-flavored markdown string by splitting it at fenced
  code blocks: prose runs become sanitized HTML, code runs become <CodeBlock>s
  (language label + copy + highlight + diff coloring). An open (streaming) block
  shows a tail caret after the last prose run, mirroring Claude.ai.
-->
<script lang="ts">
  import { splitMarkdown, renderProse, type Segment } from '$lib/render/markdown';
  import CodeBlock from './CodeBlock.svelte';

  export let source = '';
  export let streaming = false;

  let segments: Segment[] = [];
  $: segments = splitMarkdown(source);
  $: lastIsProse = segments.length > 0 && segments[segments.length - 1].kind === 'prose';
</script>

<div class="md">
  {#each segments as seg, i}
    {#if seg.kind === 'code'}
      <CodeBlock lang={seg.lang} code={seg.code} />
    {:else}
      <div class="prose">
        <!-- renderProse sanitizes with DOMPurify before injection. -->
        {@html renderProse(seg.md)}{#if streaming && lastIsProse && i === segments.length - 1}<span
            class="caret"
          ></span>{/if}
      </div>
    {/if}
  {/each}
  {#if streaming && !lastIsProse}
    <span class="caret"></span>
  {/if}
</div>

<style>
  .md {
    font-size: 0.8rem;
    line-height: 1.6;
    color: var(--konjo-paper);
    word-break: break-word;
  }
  .prose :global(p) {
    margin: 0.35rem 0;
  }
  .prose :global(p:first-child) {
    margin-top: 0;
  }
  .prose :global(ul),
  .prose :global(ol) {
    margin: 0.35rem 0;
    padding-left: 1.2rem;
  }
  .prose :global(li) {
    margin: 0.15rem 0;
  }
  .prose :global(h1),
  .prose :global(h2),
  .prose :global(h3) {
    font-family: var(--font-display, inherit);
    font-weight: 700;
    margin: 0.6rem 0 0.3rem;
    line-height: 1.25;
  }
  .prose :global(h1) {
    font-size: 1.05rem;
  }
  .prose :global(h2) {
    font-size: 0.95rem;
  }
  .prose :global(h3) {
    font-size: 0.85rem;
  }
  .prose :global(code) {
    font-family: var(--font-mono, monospace);
    font-size: 0.74rem;
    background: rgba(255, 255, 255, 0.08);
    padding: 0.05rem 0.3rem;
    border-radius: 4px;
  }
  .prose :global(a) {
    color: var(--konjo-ice);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .prose :global(blockquote) {
    border-left: 2px solid rgba(255, 255, 255, 0.18);
    margin: 0.35rem 0;
    padding-left: 0.6rem;
    opacity: 0.8;
  }
  .prose :global(table) {
    border-collapse: collapse;
    font-size: 0.72rem;
    margin: 0.4rem 0;
  }
  .prose :global(th),
  .prose :global(td) {
    border: 1px solid rgba(255, 255, 255, 0.12);
    padding: 0.2rem 0.45rem;
  }
  .prose {
    display: inline;
  }
  .caret {
    display: inline-block;
    width: 0.5rem;
    height: 0.95rem;
    margin-left: 0.1rem;
    vertical-align: text-bottom;
    background: var(--konjo-ice);
    opacity: 0.75;
    animation: caret-blink 1.05s steps(1) infinite;
  }
  @keyframes caret-blink {
    50% {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .caret {
      animation: none;
    }
  }
</style>
