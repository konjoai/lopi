<!--
  CostAnalytics — accumulated spend visualized as a sparkline + breakdown.

  Three views in one panel:
    • Total spend across all agents (live counter)
    • Per-agent cost breakdown (top 5 sorted descending)
    • Sparkline of total cost over the recent run window (60 samples)
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { agents, type AgentState } from '$lib/stores/agents';

  // Sample history: every 1s, push the current total cost
  const SAMPLES = 60;
  let history: number[] = Array(SAMPLES).fill(0);
  let timer: ReturnType<typeof setInterval> | null = null;

  $: agentList = [...$agents.values()];
  $: total = agentList.reduce((sum, a) => sum + a.cost, 0);
  $: topByCost = agentList
    .filter((a) => a.cost > 0)
    .sort((a, b) => b.cost - a.cost)
    .slice(0, 5);

  onMount(() => {
    timer = setInterval(() => {
      history = [...history.slice(1), total];
    }, 1000);
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  // Sparkline geometry
  const W = 220;
  const H = 40;
  $: maxSample = Math.max(0.0001, ...history);
  $: pathD = (() => {
    if (history.every((v) => v === 0)) return '';
    const step = W / (history.length - 1);
    const points = history.map((v, i) => {
      const x = i * step;
      const y = H - (v / maxSample) * H;
      return `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`;
    });
    return points.join(' ');
  })();

  $: areaD = pathD ? `${pathD} L ${W} ${H} L 0 ${H} Z` : '';

  function fmt(v: number): string {
    return `$${v.toFixed(4)}`;
  }
</script>

<div class="bg-black/30 border border-white/5 rounded-lg px-4 py-3 space-y-3">
  <div class="flex items-baseline justify-between">
    <span class="font-mono text-[10px] uppercase tracking-widest opacity-50">spend</span>
    <span class="font-mono text-xl font-semibold tabular-nums">
      {fmt(total)}
    </span>
  </div>

  {#if pathD}
    <svg viewBox="0 0 {W} {H}" width={W} height={H} class="block w-full" preserveAspectRatio="none">
      <defs>
        <linearGradient id="costGradient" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stop-color="var(--konjo-ember)" stop-opacity="0.5" />
          <stop offset="100%" stop-color="var(--konjo-ember)" stop-opacity="0" />
        </linearGradient>
      </defs>
      <path d={areaD} fill="url(#costGradient)" />
      <path
        d={pathD}
        fill="none"
        stroke="var(--konjo-flame)"
        stroke-width="1.5"
        stroke-linejoin="round"
        stroke-linecap="round"
      />
    </svg>
  {:else}
    <div class="h-[40px] flex items-center justify-center text-[10px] uppercase tracking-widest opacity-30 font-mono">
      no spend yet
    </div>
  {/if}

  {#if topByCost.length > 0}
    <div class="pt-2 border-t border-white/5 space-y-1.5">
      <div class="font-mono text-[10px] uppercase tracking-widest opacity-50 mb-1">top agents</div>
      {#each topByCost as a (a.id)}
        <div class="flex items-baseline justify-between gap-3 text-xs">
          <span class="truncate opacity-80 max-w-[12rem]">{a.goal}</span>
          <span class="font-mono tabular-nums opacity-90 flex-shrink-0">{fmt(a.cost)}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>
