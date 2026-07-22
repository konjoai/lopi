<!--
  Budget — cost governance. Live fleet spend, burn-rate vs a configurable hourly
  cap, projection, per-agent spend with stop controls, and a fleet kill switch.
  Phase 10 of the competitive roadmap: the market's loudest pain, surfaced.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { agents, cancelTask, type AgentState } from '$lib/stores/agents';
  import { budgetAlerts } from '$lib/stores/events';
  import {
    fleetBudget,
    hourlyCap,
    budgetColor,
    startBudgetSampler
  } from '$lib/stores/budget';
  import StatCard from '$lib/components/ui/StatCard.svelte';

  onMount(startBudgetSampler);

  const PRESETS = [1, 5, 10, 25, 50];

  $: spenders = [...$agents.values()]
    .filter((a) => a.cost > 0)
    .sort((a, b) => b.cost - a.cost)
    .slice(0, 8);
  $: maxCost = spenders.length ? spenders[0].cost : 1;
  $: color = budgetColor($fleetBudget.state);

  async function stopAll() {
    for (const a of $agents.values()) {
      if (a.status === 'running') await cancelTask(a.id);
    }
  }

  function fmtMins(m: number | null): string {
    if (m === null) return '—';
    if (m < 1) return '<1m';
    if (m < 60) return `${Math.round(m)}m`;
    return `${(m / 60).toFixed(1)}h`;
  }
</script>

<svelte:head><title>lopi · budget</title></svelte:head>

<div class="max-w-5xl mx-auto px-6 py-8 space-y-6">
  <!-- Header -->
  <div class="flex items-end justify-between flex-wrap gap-4">
    <div>
      <h1 class="font-display text-2xl">Budget</h1>
      <p class="font-mono text-[11px] uppercase tracking-widest opacity-50 mt-1">
        cost governance · live burn vs cap
      </p>
    </div>
    <button
      type="button"
      on:click={stopAll}
      disabled={$fleetBudget.running === 0}
      class="press font-mono text-[11px] uppercase tracking-widest px-4 py-2 rounded-lg border border-konjo-rose/40 text-konjo-rose hover:bg-konjo-rose/10 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
    >
      ◼ stop all running ({$fleetBudget.running})
    </button>
  </div>

  <!-- Stat cards -->
  <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
    {#each [['spent (session)', `$${$fleetBudget.spent.toFixed(4)}`, 'var(--konjo-flame)'], ['burn rate', `$${$fleetBudget.burnPerHour.toFixed(2)}/h`, color], ['hourly cap', `$${$fleetBudget.cap.toFixed(2)}`, 'var(--konjo-ice)'], ['to cap', fmtMins($fleetBudget.minutesToCap), color]] as [label, value, c]}
      <StatCard {label} {value} color={c} />
    {/each}
  </div>

  <!-- Burn meter vs cap -->
  <div class="rounded-xl border border-white/8 bg-konjo-deep/50 backdrop-blur-sm p-5">
    <div class="flex items-center justify-between mb-3">
      <span class="font-mono text-[10px] uppercase tracking-widest opacity-60">burn vs cap</span>
      <span class="font-mono text-[11px] tabular-nums" style:color>
        {Math.round($fleetBudget.fraction * 100)}% of cap
      </span>
    </div>
    <div class="relative h-3 rounded-full bg-black/40 overflow-hidden">
      <div
        class="h-full rounded-full transition-all duration-500"
        style:width={`${Math.min(100, $fleetBudget.fraction * 100)}%`}
        style:background={color}
        style:box-shadow={`0 0 12px ${color}`}
      ></div>
      <!-- 75% warn marker -->
      <div class="absolute top-0 bottom-0 w-px bg-white/30" style="left: 75%"></div>
    </div>

    <!-- Cap setter -->
    <div class="flex items-center gap-2 mt-4 flex-wrap">
      <span class="font-mono text-[10px] uppercase tracking-widest opacity-40">cap $/h</span>
      <input
        type="number"
        min="0.5"
        step="0.5"
        bind:value={$hourlyCap}
        class="w-20 bg-black/30 border border-white/10 focus:border-konjo-ice rounded px-2 py-1 font-mono text-xs tabular-nums outline-none transition-colors"
      />
      {#each PRESETS as p}
        <button
          type="button"
          on:click={() => hourlyCap.set(p)}
          class="press font-mono text-[10px] px-2.5 py-1 rounded border transition-colors"
          class:border-konjo-ice={$hourlyCap === p}
          class:text-konjo-ice={$hourlyCap === p}
          class:border-white-10={$hourlyCap !== p}
          style:border-color={$hourlyCap === p ? 'var(--konjo-ice)' : 'rgba(255,255,255,0.1)'}
          style:opacity={$hourlyCap === p ? '1' : '0.6'}
        >
          ${p}
        </button>
      {/each}
    </div>
  </div>

  <!-- Top spenders -->
  <div class="rounded-xl border border-white/8 bg-konjo-deep/50 backdrop-blur-sm p-5">
    <div class="font-mono text-[10px] uppercase tracking-widest opacity-60 mb-3">top spenders</div>
    {#if spenders.length === 0}
      <div class="font-mono text-[11px] opacity-30 py-4 text-center">no spend yet</div>
    {:else}
      <div class="space-y-2.5">
        {#each spenders as a (a.id)}
          <div class="flex items-center gap-3">
            <div class="w-1.5 h-1.5 rounded-full flex-shrink-0" class:animate-pulse={a.status === 'running'} style:background={a.status === 'running' ? 'var(--konjo-jade)' : 'rgba(255,255,255,0.2)'}></div>
            <div class="flex-1 min-w-0">
              <div class="font-mono text-[11px] truncate">{a.goal}</div>
              <div class="h-1 mt-1 rounded-full bg-black/40 overflow-hidden">
                <div class="h-full rounded-full" style:width={`${(a.cost / maxCost) * 100}%`} style:background="var(--konjo-flame)"></div>
              </div>
            </div>
            <span class="font-mono text-[11px] tabular-nums w-16 text-right" style:color="var(--konjo-flame)">${a.cost.toFixed(4)}</span>
            <button
              type="button"
              on:click={() => cancelTask(a.id)}
              disabled={a.status !== 'running'}
              class="press w-6 h-6 flex items-center justify-center text-konjo-rose hover:bg-konjo-rose/10 rounded disabled:opacity-20 text-[10px] transition-colors"
              title="Stop"
            >◼</button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Breach history -->
  {#if $budgetAlerts.length > 0}
    <div class="rounded-xl border border-konjo-rose/30 bg-konjo-rose/5 p-5">
      <div class="font-mono text-[10px] uppercase tracking-widest text-konjo-rose mb-3">recent breaches</div>
      <div class="space-y-1.5">
        {#each $budgetAlerts as alert (alert.seq)}
          <div class="font-mono text-[11px] flex items-center gap-2">
            <span class="text-konjo-rose">◈</span>
            <span class="opacity-70">{alert.scope}</span>
            <span class="opacity-40">·</span>
            <span class="tabular-nums">${alert.burnedUsd.toFixed(2)} / ${alert.limitUsd.toFixed(2)}/h</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
