<!--
  StackOutput — live output attachment fused under the single running card.
  Genuinely wired to `stores/transcript.ts`'s per-`task_id` block feed (the
  same store the Forge transcript uses). Renders nothing when the task has no
  blocks yet — per §3 of the UI-2 brief, a stream with nothing real to show
  must stay empty, never fabricate a ticker.

  UI-3 consolidated the old four-section (thinking/actions/tools/output)
  accordion into a single chronological stream — one continuous transcript,
  Claude-Desktop-style. UI-4 dropped the kind-filter tabs entirely (every log
  shows, always) and the running-vs-idle "live output"/"logs" distinction —
  the stream now always reads as plain "logs", flat text with no card
  background, matching the prompt above it. The stream itself is a
  fixed-but-expandable size: ~10 lines by default (expanded, not collapsed,
  out of the gate) and a "grow" toggle for reading back through a longer run
  without losing that fixed default the rest of the time.
-->
<script lang="ts">
  import { afterUpdate } from 'svelte';
  import { transcripts, type TranscriptBlock } from '$lib/stores/transcript';
  import { ICONS } from './icons';

  export let taskId: string;

  type Kind = 'thinking' | 'actions' | 'tools' | 'output';

  const KIND_ICON: Record<Kind, string> = {
    thinking: ICONS.bulb,
    actions: ICONS.zap,
    tools: ICONS.wrench,
    output: ICONS.list
  };

  // Expanded by default (UI-4) — a collapsed one-line strip is still reachable
  // via the collapse button, it just isn't the initial state anymore.
  let expanded = true;
  // The stream's "fixed but expandable" size toggle — independent of
  // `expanded` (which switches between the one-line strip and the full
  // stream at all): `big` just grows the already-open stream's own
  // max-height, so a long run stays readable without abandoning the fixed
  // ~10-line default the rest of the time.
  let big = false;
  let streamEl: HTMLDivElement | undefined;

  function categorize(b: TranscriptBlock): Kind {
    switch (b.kind) {
      case 'thinking':
        return 'thinking';
      case 'tool_call':
        return 'tools';
      case 'status':
        return 'actions';
      case 'assistant_text':
        return 'output';
    }
  }

  function textOf(b: TranscriptBlock): string {
    switch (b.kind) {
      case 'thinking':
      case 'assistant_text':
        return b.text;
      case 'status':
        return b.label;
      case 'tool_call':
        return b.result ? `${b.tool} → ${b.result.preview}` : b.tool;
    }
  }

  $: blocks = $transcripts.get(taskId) ?? [];
  $: latest = blocks[blocks.length - 1];
  $: latestKind = latest ? categorize(latest) : null;

  // A live transcript that silently scrolls itself away from the newest
  // line reads as stalled, not streaming — pin the consolidated stream to
  // its bottom as blocks arrive, same as any chat/terminal transcript.
  afterUpdate(() => {
    if (expanded && streamEl) streamEl.scrollTop = streamEl.scrollHeight;
  });
</script>

{#if blocks.length}
  <div class="output">
    {#if !expanded}
      <div class="ostrip">
        <span class="live"><i></i></span>
        {#if latestKind}<span class="ok">{latestKind}</span>{/if}
        <span class="ol">{latest ? textOf(latest) : ''}</span>
        <button type="button" class="omini oexpbtn" on:click={() => (expanded = true)} title="expand">
          {@html ICONS.expand}
        </button>
      </div>
    {:else}
      <div class="obar">
        <span class="live"><i></i>logs</span>
        <button
          type="button"
          class="omini osizebtn"
          on:click={() => (big = !big)}
          title={big ? 'shrink output' : 'grow output'}
          aria-pressed={big}
        >
          {@html big ? ICONS.chevup : ICONS.chevdown}
        </button>
        <button type="button" class="omini ocolbtn" on:click={() => (expanded = false)} title="collapse">
          {@html ICONS.collapse}
        </button>
      </div>
      <div class="stream" class:big bind:this={streamEl}>
        {#each blocks as b (b.id)}
          {@const kind = categorize(b)}
          <div class="sline {kind}">
            <span class="sicon">{@html KIND_ICON[kind]}</span>
            <span class="stext">{textOf(b)}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* No border/animation/background here — `.loopwrap.hasout` in
     StackPane.svelte owns the card's own outline, and UI-4 dropped this
     panel's own card treatment entirely: flat text, same as the prompt
     above it, no boxed-in "card" look of its own. */
  .output {
    font-family: var(--font-mono, monospace);
  }
  .ostrip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    font-size: 10px;
    color: rgb(var(--k-text-primary-rgb) / 0.46);
    min-width: 0;
  }
  .live {
    display: inline-flex;
    flex: 0 0 auto;
  }
  /* Always a static, greyed-out dot (UI-4) — no pulsing, no blue "live"
     glow, and no running-vs-idle distinction. The stream is just logs. */
  .live i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: rgb(var(--k-text-primary-rgb) / 0.28);
  }
  .ok {
    color: var(--stack-violet, var(--k-chip-model));
    flex: 0 0 auto;
  }
  .ol {
    color: rgb(var(--k-text-primary-rgb) / 0.72);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .omini {
    width: 24px;
    height: 22px;
    border-radius: 5px;
    border: 1px solid rgb(var(--k-wash-rgb) / 0.11);
    background: transparent;
    color: rgb(var(--k-text-primary-rgb) / 0.28);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
  }
  .omini :global(svg) {
    width: 12px;
    height: 12px;
  }
  .omini.oexpbtn {
    margin-left: auto;
    color: var(--konjo-ice);
    border-color: rgb(var(--k-chip-repo-rgb) / 0.35);
  }
  .omini.oexpbtn:hover {
    background: rgb(var(--k-chip-repo-rgb) / 0.1);
  }
  .omini.osizebtn {
    margin-left: auto;
    color: var(--stack-violet, var(--k-chip-model));
    border-color: rgb(var(--k-border-interactive-rgb) / 0.35);
  }
  .omini.osizebtn:hover {
    background: rgb(var(--k-border-interactive-rgb) / 0.1);
  }
  .omini.ocolbtn {
    color: var(--konjo-flame);
    border-color: rgb(var(--k-chip-loop-rgb) / 0.45);
    background: rgb(var(--k-chip-loop-rgb) / 0.08);
  }
  .omini.ocolbtn:hover {
    background: rgb(var(--k-chip-loop-rgb) / 0.16);
  }
  .obar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    font-size: 9px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .obar .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: rgb(var(--k-text-primary-rgb) / 0.46);
  }
  /* The consolidated stream (UI-3) — one chronological feed, fixed height by
     default (~10 lines, UI-4) and scrolled to bottom as blocks arrive,
     growing only when the `big` toggle is on. This is the ONLY scrollbar for
     the whole output section; nothing nested inside it may grow its own. */
  .stream {
    max-height: 200px;
    overflow-y: auto;
    padding: 10px 12px;
    scroll-behavior: smooth;
  }
  .stream.big {
    max-height: 70vh;
  }
  .sline {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 10.5px;
    line-height: 1.6;
    padding: 3px 0;
  }
  .sicon {
    flex: 0 0 auto;
    display: inline-flex;
    margin-top: 2px;
    opacity: 0.85;
  }
  .sicon :global(svg) {
    width: 12px;
    height: 12px;
  }
  /* One uniform, easy-to-read text color for every log line regardless of
     kind or severity (UI-4) — only the leading icon still carries a
     per-kind color; per-kind/per-tier text colors read as noisy/distracting
     once there are more than a couple lines on screen. */
  .stext {
    flex: 1;
    min-width: 0;
    white-space: pre-wrap;
    word-break: break-word;
    color: rgb(var(--k-text-primary-rgb) / 0.82);
  }
  .sline.thinking .sicon {
    color: var(--stack-violet, var(--k-chip-model));
  }
  .sline.actions .sicon {
    color: var(--konjo-sun);
  }
  .sline.tools .sicon {
    color: var(--konjo-ice);
  }
  .sline.output .sicon {
    color: var(--konjo-jade);
  }
</style>
