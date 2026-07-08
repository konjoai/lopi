<!--
  StackOutput — live output attachment fused under the single running card.
  Genuinely wired to `stores/transcript.ts`'s per-`task_id` block feed (the
  same store the Forge transcript uses), filtered into the mockup's four
  categories. Renders nothing when the task has no blocks yet — per §3 of
  the UI-2 brief, a stream with nothing real to show must stay empty, never
  fabricate a ticker. Backend-1 wired real `taskId`s onto running cards
  (`stores/stackRun.ts`), so this component needed no changes at all to
  start lighting up for real — it was already reading the right store,
  keyed the right way, waiting for a real id to show up.
-->
<script lang="ts">
  import { transcripts, type TranscriptBlock } from '$lib/stores/transcript';
  import { ICONS } from './icons';

  export let taskId: string;

  type Kind = 'thinking' | 'actions' | 'tools' | 'output';
  type Filter = 'all' | Kind;

  const FILTERS: Filter[] = ['all', 'thinking', 'actions', 'tools', 'output'];
  const SECTIONS: { kind: Kind; icon: string; label: string }[] = [
    { kind: 'thinking', icon: ICONS.bulb, label: 'thinking' },
    { kind: 'actions', icon: ICONS.zap, label: 'actions' },
    { kind: 'tools', icon: ICONS.wrench, label: 'tools' },
    { kind: 'output', icon: ICONS.list, label: 'output' }
  ];

  let expanded = false;
  let filter: Filter = 'all';
  let openSections: Record<Kind, boolean> = {
    thinking: true,
    actions: false,
    tools: false,
    output: false
  };

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

  function toggleSection(kind: Kind) {
    openSections = { ...openSections, [kind]: !openSections[kind] };
  }

  $: blocks = $transcripts.get(taskId) ?? [];
  $: byKind = {
    thinking: blocks.filter((b) => categorize(b) === 'thinking'),
    actions: blocks.filter((b) => categorize(b) === 'actions'),
    tools: blocks.filter((b) => categorize(b) === 'tools'),
    output: blocks.filter((b) => categorize(b) === 'output')
  };
  $: latest = blocks[blocks.length - 1];
  $: latestKind = latest ? categorize(latest) : null;
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
        <span class="live"><i></i>live output</span>
        <div class="filters">
          {#each FILTERS as f (f)}
            <button type="button" class="fchip" class:on={filter === f} on:click={() => (filter = f)}>{f}</button>
          {/each}
        </div>
        <button type="button" class="omini ocolbtn" on:click={() => (expanded = false)} title="collapse">
          {@html ICONS.collapse}
        </button>
      </div>
      <div class="osecs">
        {#each SECTIONS as s (s.kind)}
          {#if filter === 'all' || filter === s.kind}
            {@const open = filter === s.kind || openSections[s.kind]}
            <div class="osec {s.kind}" class:open>
              <button type="button" class="osh" on:click={() => toggleSection(s.kind)}>
                <span class="chev">{@html ICONS.chevdown}</span>
                {@html s.icon}<span class="olabel">{s.label}</span>
                <span class="ometa">{byKind[s.kind].length}</span>
              </button>
              <div class="osbody">
                <div class="inner">
                  {#each byKind[s.kind] as b (b.id)}
                    <div class="oline">{textOf(b)}</div>
                  {/each}
                </div>
              </div>
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .output {
    background: var(--stack-outbg, #0c1417);
    border: 1px solid rgba(255, 150, 70, 0.45);
    border-top: none;
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
  .osecs {
    max-height: 340px;
    overflow-y: auto;
  }
  .osec {
    border-top: 1px solid rgba(0, 212, 255, 0.06);
  }
  .osec:first-child {
    border-top: none;
  }
  .osh {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px;
    font-size: 10.5px;
    cursor: pointer;
    width: 100%;
    background: transparent;
    border: none;
    text-align: left;
    font-family: var(--font-mono, monospace);
  }
  .osh .chev {
    transition: transform 0.18s;
    color: rgba(245, 245, 245, 0.28);
    display: inline-flex;
  }
  .osh .chev :global(svg) {
    width: 11px;
    height: 11px;
  }
  .osec.open .osh .chev {
    transform: rotate(180deg);
  }
  .osh :global(svg) {
    width: 13px;
    height: 13px;
  }
  .osh .olabel {
    flex: 0 0 auto;
  }
  .osh .ometa {
    margin-left: auto;
    color: rgba(245, 245, 245, 0.28);
    font-size: 9px;
    text-transform: none;
  }
  .osec.thinking .osh {
    color: var(--stack-violet, #b79bff);
  }
  .osec.actions .osh {
    color: var(--konjo-sun);
  }
  .osec.tools .osh {
    color: var(--konjo-ice);
  }
  .osec.output .osh {
    color: var(--konjo-jade);
  }
  .osbody {
    max-height: 0;
    overflow: hidden;
    transition: max-height 0.22s ease;
  }
  .osec.open .osbody {
    max-height: 260px;
    overflow-y: auto;
  }
  .osbody .inner {
    padding: 2px 12px 11px 32px;
  }
  .oline {
    font-size: 10.5px;
    line-height: 1.6;
    margin-bottom: 4px;
  }
  .osec.thinking .oline {
    color: rgba(183, 155, 255, 0.72);
    font-style: italic;
  }
  .osec.actions .oline,
  .osec.tools .oline {
    color: rgba(245, 245, 245, 0.46);
  }
  .osec.output .oline {
    color: rgba(0, 255, 157, 0.75);
  }
  @media (prefers-reduced-motion: reduce) {
    .live i {
      animation: none;
    }
    .osbody {
      transition: none;
    }
  }
</style>
