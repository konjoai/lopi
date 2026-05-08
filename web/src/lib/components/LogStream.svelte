<!--
  LogStream — terminal-style log viewer.
  Color-coded by level. Auto-scrolls but pauses on hover.
-->
<script lang="ts">
  import { logs, type LogEntry } from '$lib/stores/agents';
  import { afterUpdate } from 'svelte';

  export let limit: number = 12;

  let container: HTMLDivElement;
  let pinned = true;

  $: tail = $logs.slice(-limit);

  afterUpdate(() => {
    if (pinned && container) container.scrollTop = container.scrollHeight;
  });

  function levelColor(l: LogEntry['level']): string {
    if (l === 'error') return 'var(--konjo-rose)';
    if (l === 'warn') return 'var(--konjo-flame)';
    if (l === 'debug') return 'rgba(255,255,255,0.4)';
    return 'var(--konjo-ice)';
  }

  function fmtTime(ts: number): string {
    const d = new Date(ts);
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
  }
</script>

<div
  bind:this={container}
  class="font-mono text-xs leading-relaxed h-32 overflow-y-auto bg-black/40 ring-1 ring-white/5 rounded-lg px-3 py-2"
  on:mouseenter={() => (pinned = false)}
  on:mouseleave={() => (pinned = true)}
  role="log"
  aria-live="polite"
>
  {#each tail as e (e.ts + e.taskId + e.message)}
    <div class="flex gap-2 items-baseline opacity-90 hover:opacity-100">
      <span class="text-white/30 tabular-nums">{fmtTime(e.ts)}</span>
      <span class="text-white/40 truncate max-w-[7rem]">{e.taskId}</span>
      <span style:color={levelColor(e.level)} class="uppercase text-[9px] tracking-widest min-w-[2.5rem]">
        {e.level}
      </span>
      <span class="flex-1 truncate">{e.message}</span>
    </div>
  {/each}
  {#if tail.length === 0}
    <div class="opacity-30">— waiting for events —</div>
  {/if}
</div>
