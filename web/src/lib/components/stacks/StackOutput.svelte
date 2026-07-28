<!--
  StackOutput — live output attachment fused under the single running card.
  Genuinely wired to `stores/transcript.ts`'s per-`task_id` block feed (the
  same store the Forge transcript uses), filtered by kind. Renders nothing
  when the task has no blocks yet — per §3 of the UI-2 brief, a stream with
  nothing real to show must stay empty, never fabricate a ticker.

  UI-3 consolidated the old four-section (thinking/actions/tools/output)
  accordion into a single chronological stream — one continuous transcript,
  Claude-Desktop-style, rather than several independently-collapsible
  sub-panels a user had to open one at a time. The kind filter chips still
  narrow the same stream instead of toggling separate sections. The stream
  itself is a fixed-but-expandable size: a compact default height while
  running, and a "grow" toggle for reading back through a longer run without
  losing the fixed-height default the rest of the time.
-->
<script lang="ts">
  import { afterUpdate } from 'svelte';
  import { transcripts, type TranscriptBlock } from '$lib/stores/transcript';
  import { ICONS } from './icons';

  export let taskId: string;
  /** Whether the attached card is still running. Drives the "live output"
   *  (pulsing dot) vs "logs" (static) label — the panel itself stays
   *  reachable either way; only the framing changes once nothing new is
   *  actually streaming in. */
  export let isRunning: boolean = true;

  type Kind = 'thinking' | 'actions' | 'tools' | 'output';
  type Filter = 'all' | Kind;

  const FILTERS: Filter[] = ['all', 'thinking', 'actions', 'tools', 'output'];
  const KIND_ICON: Record<Kind, string> = {
    thinking: ICONS.bulb,
    actions: ICONS.zap,
    tools: ICONS.wrench,
    output: ICONS.list
  };

  let expanded = false;
  // The stream's "fixed but expandable" size toggle — independent of
  // `expanded` (which switches between the one-line strip and the full
  // stream at all): `big` just grows the already-open stream's own
  // max-height, so a long run stays readable without abandoning the fixed
  // default the rest of the time.
  let big = false;
  let filter: Filter = 'all';
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

  /** A status block's severity tier drives its line color (see `.tier-*`
   *  below); every other kind has no tier of its own, so its line keeps the
   *  section's plain color. Without this a `bad` verifier error and a
   *  `good` score pass rendered in the exact same dim gray — the tier
   *  `transcript.ts` computes was reaching this component and being
   *  silently discarded. */
  function tierOf(b: TranscriptBlock): string | null {
    return b.kind === 'status' ? b.tier : null;
  }

  $: blocks = $transcripts.get(taskId) ?? [];
  $: filtered = filter === 'all' ? blocks : blocks.filter((b) => categorize(b) === filter);
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
        <span class="live" class:idle={!isRunning}><i></i></span>
        {#if latestKind}<span class="ok">{latestKind}</span>{/if}
        <span class="ol" class:tier-good={latest && tierOf(latest) === 'good'} class:tier-warn={latest && tierOf(latest) === 'warn'} class:tier-bad={latest && tierOf(latest) === 'bad'}
          >{latest ? textOf(latest) : ''}</span
        >
        <button type="button" class="omini oexpbtn" on:click={() => (expanded = true)} title="expand">
          {@html ICONS.expand}
        </button>
      </div>
    {:else}
      <div class="obar">
        <span class="live" class:idle={!isRunning}><i></i>{isRunning ? 'live output' : 'logs'}</span>
        <div class="filters">
          {#each FILTERS as f (f)}
            <button type="button" class="fchip" class:on={filter === f} on:click={() => (filter = f)}>{f}</button>
          {/each}
        </div>
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
        {#each filtered as b (b.id)}
          {@const kind = categorize(b)}
          <div
            class="sline {kind}"
            class:tier-good={tierOf(b) === 'good'}
            class:tier-warn={tierOf(b) === 'warn'}
            class:tier-bad={tierOf(b) === 'bad'}
          >
            <span class="sicon">{@html KIND_ICON[kind]}</span>
            <span class="stext">{textOf(b)}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* No border/animation here — `.loopwrap.hasout` in StackPane.svelte owns
     the entire outline (and, while running, its single animation) for a
     card with output attached; this panel is always borderless. Radius
     stays for background-clipping (the wrapper's own border-radius clips
     the outline, but each child's background still needs its own matching
     corners underneath it). */
  .output {
    background: var(--stack-outbg, #0c1417);
    border-radius: 0 0 9px 9px;
    overflow: hidden;
    font-family: var(--font-mono, monospace);
  }
  .ostrip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    font-size: 10px;
    color: rgba(245, 245, 245, 0.46);
    min-width: 0;
  }
  .live {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .live i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--konjo-ice);
    box-shadow: 0 0 6px var(--konjo-ice);
    animation: pulse 1.4s infinite;
  }
  /* Once the card isn't running anymore, this is a static log, not a live
     stream — the dot stops pulsing and dims instead of implying new
     content could still arrive. */
  .live.idle i {
    background: rgba(245, 245, 245, 0.28);
    box-shadow: none;
    animation: none;
  }
  .obar .live.idle {
    color: rgba(245, 245, 245, 0.46);
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }
  .ok {
    color: var(--stack-violet, #b79bff);
    flex: 0 0 auto;
  }
  .ol {
    color: rgba(245, 245, 245, 0.46);
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
    border: 1px solid rgba(255, 255, 255, 0.11);
    background: transparent;
    color: rgba(245, 245, 245, 0.28);
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
    border-color: rgba(0, 212, 255, 0.35);
  }
  .omini.oexpbtn:hover {
    background: rgba(0, 212, 255, 0.1);
  }
  .omini.osizebtn {
    color: var(--stack-violet, #b79bff);
    border-color: rgba(183, 155, 255, 0.35);
  }
  .omini.osizebtn:hover {
    background: rgba(183, 155, 255, 0.1);
  }
  .omini.ocolbtn {
    color: var(--konjo-flame);
    border-color: rgba(255, 149, 0, 0.45);
    background: rgba(255, 149, 0, 0.08);
  }
  .omini.ocolbtn:hover {
    background: rgba(255, 149, 0, 0.16);
  }
  .obar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    border-bottom: 1px solid rgba(0, 212, 255, 0.1);
    font-size: 9px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .obar .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--konjo-ice);
  }
  .filters {
    margin-left: auto;
    display: flex;
    gap: 3px;
  }
  .fchip {
    padding: 2px 7px;
    border-radius: 3px;
    color: rgba(245, 245, 245, 0.28);
    cursor: pointer;
    border: 1px solid transparent;
    text-transform: uppercase;
    background: transparent;
    font-family: var(--font-mono, monospace);
    font-size: 9px;
  }
  .fchip.on {
    color: var(--konjo-ice);
    border-color: rgba(0, 212, 255, 0.3);
    background: rgba(0, 212, 255, 0.06);
  }
  /* The consolidated stream (UI-3) — one chronological feed, fixed height by
     default and scrolled to bottom as blocks arrive, growing only when the
     `big` toggle is on. This is the ONLY scrollbar for the whole live-output
     section; nothing nested inside it may grow its own. */
  .stream {
    max-height: 300px;
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
  .stext {
    flex: 1;
    min-width: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .sline.thinking {
    color: rgba(183, 155, 255, 0.72);
    font-style: italic;
  }
  .sline.thinking .sicon {
    color: var(--stack-violet, #b79bff);
  }
  .sline.actions {
    color: rgba(245, 245, 245, 0.46);
  }
  .sline.actions .sicon {
    color: var(--konjo-sun);
  }
  .sline.tools {
    color: rgba(245, 245, 245, 0.46);
  }
  .sline.tools .sicon {
    color: var(--konjo-ice);
  }
  .sline.output {
    color: rgba(0, 255, 157, 0.75);
  }
  .sline.output .sicon {
    color: var(--konjo-jade);
  }
  /* Severity tier wins over the kind's plain color — a `bad` verifier
     error or `good` score pass must read distinctly from routine `info`
     status, matching StatusChip.svelte's jade/flame/rose language. */
  .sline.tier-good,
  .ol.tier-good {
    color: var(--konjo-jade);
  }
  .sline.tier-warn,
  .ol.tier-warn {
    color: var(--konjo-flame);
  }
  .sline.tier-bad,
  .ol.tier-bad {
    color: var(--konjo-rose);
  }
  @media (prefers-reduced-motion: reduce) {
    .live i {
      animation: none;
    }
    .stream {
      scroll-behavior: auto;
    }
  }
</style>
