<!--
  CodeBlock — a fenced code block in the Claude.ai style: a language label and a
  Copy button on a header bar, syntax highlighting (Shiki, async + debounced so a
  streaming transcript never thrashes), horizontal scroll, and dedicated red/green
  gutter coloring for `diff` blocks. Falls back to plain monospace if Shiki fails.
-->
<script lang="ts">
  import { highlight } from '$lib/render/highlight';

  export let lang = '';
  export let code = '';

  let copied = false;
  let html: string | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;

  $: label = lang || 'text';
  $: isDiff = lang.toLowerCase() === 'diff';
  $: diffLines = isDiff ? code.split('\n') : [];

  // Debounce highlighting: re-highlight 120ms after the code settles, not on
  // every keystroke of a streaming block. Diff blocks render natively below.
  $: if (!isDiff) scheduleHighlight(code, lang);

  function scheduleHighlight(c: string, l: string) {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      void highlight(c, l).then((out) => {
        html = out;
      });
    }, 120);
  }

  function diffClass(line: string): string {
    if (line.startsWith('+') && !line.startsWith('+++')) return 'diff-add';
    if (line.startsWith('-') && !line.startsWith('---')) return 'diff-del';
    if (line.startsWith('@@')) return 'diff-hunk';
    return 'diff-ctx';
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      copied = true;
      setTimeout(() => (copied = false), 1400);
    } catch (err) {
      console.warn('[lopi] copy failed:', err);
    }
  }
</script>

<div class="code-block">
  <div class="code-head">
    <span class="lang">{label}</span>
    <button type="button" class="copy" on:click={copy} title="Copy code">
      {copied ? '✓ copied' : 'copy'}
    </button>
  </div>
  {#if isDiff}
    <pre class="code-body diff"><code
        >{#each diffLines as line}<span class={diffClass(line)}>{line || ' '}
</span>{/each}</code
      ></pre>
  {:else if html}
    <!-- Shiki output is generated from agent code; it is escaped HTML wrapping
         the source text (no script execution surface). -->
    <div class="code-body shiki-host">{@html html}</div>
  {:else}
    <pre class="code-body"><code>{code}</code></pre>
  {/if}
</div>

<style>
  .code-block {
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    overflow: hidden;
    margin: 0.5rem 0;
    background: #0d1117; /* github-dark canvas, matches Shiki theme */
  }
  .code-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.25rem 0.6rem;
    background: rgba(255, 255, 255, 0.04);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }
  .lang {
    font-family: var(--font-mono, monospace);
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    opacity: 0.5;
  }
  .copy {
    font-family: var(--font-mono, monospace);
    font-size: 0.65rem;
    color: var(--konjo-ice);
    opacity: 0.7;
    padding: 0.1rem 0.4rem;
    border-radius: 5px;
    transition: opacity var(--dur-fast) var(--ease-out-expo), background var(--dur-fast);
  }
  .copy:hover {
    opacity: 1;
    background: rgba(0, 212, 255, 0.12);
  }
  .code-body {
    margin: 0;
    padding: 0.6rem 0.75rem;
    overflow-x: auto;
    font-family: var(--font-mono, monospace);
    font-size: 0.72rem;
    line-height: 1.5;
    color: #e6edf3;
  }
  .shiki-host :global(pre.shiki) {
    margin: 0;
    padding: 0.6rem 0.75rem;
    overflow-x: auto;
    font-size: 0.72rem;
    line-height: 1.5;
    background: transparent !important;
  }
  .diff code {
    display: block;
    white-space: pre;
  }
  .diff-add {
    color: #3fb950;
    background: rgba(46, 160, 67, 0.15);
    display: block;
  }
  .diff-del {
    color: #f85149;
    background: rgba(248, 81, 73, 0.15);
    display: block;
  }
  .diff-hunk {
    color: var(--konjo-ice);
    opacity: 0.8;
    display: block;
  }
  .diff-ctx {
    opacity: 0.8;
    display: block;
  }
</style>
