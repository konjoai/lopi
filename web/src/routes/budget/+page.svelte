<!--
  Budget — cost governance. Live fleet spend, burn-rate vs a configurable
  hourly cap, a 7-day spend trend, cost breakdowns by repo/model, and
  per-agent spend with stop controls. Phase 10 redesign: notch-badge stat
  cards, spend trend sparkline, alert threshold, by-repo/by-model breakdown.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { agents, cancelTask } from '$lib/stores/agents';
  import {
    fleetBudget,
    hourlyCap,
    alertPct,
    byRepo,
    byModel,
    trend,
    tokensToday,
    budgetColor,
    startBudgetSampler,
    startBudgetBreakdownPoller
  } from '$lib/stores/budget';

  onMount(() => {
    startBudgetSampler();
    startBudgetBreakdownPoller();
  });

  const PRESETS = [1, 5, 10, 25, 50];

  $: spenders = [...$agents.values()]
    .filter((a) => a.cost > 0)
    .sort((a, b) => b.cost - a.cost)
    .slice(0, 8);
  $: maxSpenderCost = spenders.length ? spenders[0].cost : 1;
  $: color = budgetColor($fleetBudget.state);

  function fmtMins(m: number | null): string {
    if (m === null) return '—';
    if (m < 1) return '<1m';
    if (m < 60) return `${Math.round(m)}m`;
    return `${(m / 60).toFixed(1)}h`;
  }

  $: tokensDisplay =
    $tokensToday >= 1000 ? `${Math.round($tokensToday / 1000)}K` : String($tokensToday);

  $: statCards = [
    {
      label: 'spent',
      value: `$${$fleetBudget.spent.toFixed(4)}`,
      color: '#00ffd4',
      border: 'rgba(0,255,212,0.32)',
      badge: 'rgba(0,255,212,0.4)'
    },
    {
      label: 'burn/h',
      value: `$${$fleetBudget.burnPerHour.toFixed(2)}`,
      color: '#b79bff',
      border: 'rgba(183,155,255,0.32)',
      badge: 'rgba(183,155,255,0.4)'
    },
    {
      label: 'cap/h',
      value: `$${$hourlyCap % 1 === 0 ? $hourlyCap : $hourlyCap.toFixed(2)}`,
      color: '#00d4ff',
      border: 'rgba(0,212,255,0.28)',
      badge: 'rgba(0,212,255,0.35)'
    },
    {
      label: 'to cap',
      value: fmtMins($fleetBudget.minutesToCap),
      color: '#ff9500',
      border: 'rgba(255,149,0,0.32)',
      badge: 'rgba(255,149,0,0.4)'
    },
    {
      label: 'tokens',
      value: tokensDisplay,
      color: '#ffcc00',
      border: 'rgba(255,204,0,0.32)',
      badge: 'rgba(255,204,0,0.4)'
    },
    {
      label: 'running',
      value: String($fleetBudget.running),
      color: '#00ff9d',
      border: 'rgba(0,255,157,0.3)',
      badge: 'rgba(0,255,157,0.4)'
    }
  ];

  // ── Spend trend — 7 real calendar days from turn_metrics, oldest first ──────
  function weekdayAbbrev(dateStr: string): string {
    return new Date(`${dateStr}T00:00:00Z`)
      .toLocaleDateString('en-US', { weekday: 'short', timeZone: 'UTC' })
      .slice(0, 3)
      .toLowerCase();
  }

  $: trendMax = Math.max(1, ...$trend.map((t) => t.cost));
  $: trendBars = $trend.map((t, i) => ({
    heightPct: (t.cost / trendMax) * 100,
    isToday: i === $trend.length - 1,
    label: i === $trend.length - 1 ? 'today' : weekdayAbbrev(t.date)
  }));

  // Compares today's spend to the average of the prior 6 days — the nearest
  // honest analog to "vs prior week" this app's history actually supports.
  $: trendDelta = (() => {
    if ($trend.length < 2) return null;
    const today = $trend[$trend.length - 1].cost;
    const prior = $trend.slice(0, -1);
    const priorAvg = prior.reduce((a, b) => a + b.cost, 0) / prior.length;
    if (priorAvg === 0) return today > 0 ? { pct: null, up: true } : null;
    const pct = ((today - priorAvg) / priorAvg) * 100;
    return { pct: Math.round(Math.abs(pct)), up: pct >= 0 };
  })();

  $: byRepoMax = Math.max(1, ...$byRepo.map((r) => r.cost));
  $: byModelMax = Math.max(1, ...$byModel.map((m) => m.cost));
</script>

<svelte:head><title>lopi · budget</title></svelte:head>

<div class="max-w-5xl mx-auto px-6 py-8 space-y-6">
  <!-- Header -->
  <div class="flex items-end justify-between flex-wrap gap-4">
    <div>
      <h1 class="font-display text-2xl">Budget</h1>
      <p class="font-mono text-[11px] uppercase tracking-widest opacity-45 mt-1">
        cost governance · live burn vs cap
      </p>
    </div>
  </div>

  <!-- Stat cards -->
  <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3">
    {#each statCards as s (s.label)}
      <div
        class="relative bg-konjo-card rounded-[9px] px-3.5 pt-4 pb-3"
        style:border={`1px solid ${s.border}`}
      >
        <span
          class="absolute -top-[9px] left-3 bg-konjo-deep rounded-[3px] px-[7px] py-0.5 font-mono text-[8px] tracking-[0.09em] uppercase text-white/50"
          style:border={`1px solid ${s.badge}`}
        >
          {s.label}
        </span>
        <div
          class="text-xl font-bold mt-1.5 tabular-nums"
          style:color={s.color}
        >
          {s.value}
        </div>
      </div>
    {/each}
  </div>

  <!-- Spend trend -->
  <div class="relative bg-konjo-card border border-white/[0.14] rounded-[10px] pt-[22px] px-5 pb-4">
    <span
      class="absolute -top-[9px] left-4 bg-konjo-deep border border-white/[0.14] rounded-[3px] px-[9px] py-0.5 font-mono text-[9.5px] tracking-[0.1em] uppercase text-white/55"
    >
      spend, last 7 days
    </span>
    <div class="flex justify-end mb-2.5">
      {#if trendDelta}
        <span
          class="font-mono text-[11px]"
          style:color={trendDelta.up ? 'var(--konjo-jade)' : 'var(--konjo-flame)'}
        >
          {trendDelta.up ? '▲' : '▼'}
          {trendDelta.pct !== null ? `${trendDelta.pct}%` : 'new spend'} vs 6-day avg
        </span>
      {/if}
    </div>
    {#if trendBars.length === 0}
      <div class="font-mono text-[11px] opacity-30 py-4 text-center">no spend recorded yet</div>
    {:else}
      <div class="flex items-end gap-2 h-16">
        {#each trendBars as bar, i (i)}
          <div
            class="flex-1 rounded-t-[3px]"
            style:background={bar.isToday ? '#00ffd4' : 'rgba(0,255,212,0.35)'}
            style:height={`${bar.heightPct}%`}
          ></div>
        {/each}
      </div>
      <div class="flex justify-between mt-[7px] font-mono text-[9px] text-white/35">
        {#each trendBars as bar, i (i)}
          <span>{bar.label}</span>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Burn vs cap -->
  <div
    class="relative bg-konjo-card border border-white/[0.14] rounded-[10px] pt-[22px] px-5 pb-5"
    style="box-shadow: inset 0 1px 0 rgba(255,255,255,0.06);"
  >
    <span
      class="absolute -top-[9px] left-4 bg-konjo-deep border border-konjo-teal/40 rounded-[3px] px-[9px] py-0.5 flex items-center gap-1.5 font-mono text-[9.5px] tracking-[0.1em] uppercase text-konjo-teal"
    >
      <span class="w-[5px] h-[5px] rounded-full bg-konjo-teal animate-pulse"></span>burn vs cap
    </span>
    <div class="flex justify-end mb-2.5">
      <span class="font-mono text-[11px] tabular-nums text-konjo-teal">
        {Math.round($fleetBudget.fraction * 100)}% of cap
      </span>
    </div>
    <div class="relative h-3 rounded-full bg-black/45 overflow-hidden">
      <div
        class="h-full rounded-full transition-all duration-500"
        style:width={`${Math.min(100, $fleetBudget.fraction * 100)}%`}
        style="background:#00ffd4; box-shadow:0 0 12px #00ffd4;"
      ></div>
      <div class="absolute top-0 bottom-0 w-px bg-white/30" style="left: 75%"></div>
    </div>

    <!-- Cap setter -->
    <div class="flex items-center gap-2 mt-[18px] flex-wrap">
      <span class="font-mono text-[10px] uppercase tracking-widest opacity-40 mr-0.5">cap $/h</span>
      <input
        type="number"
        min="0.5"
        step="0.5"
        bind:value={$hourlyCap}
        class="w-[76px] bg-black/30 border border-white/10 focus:border-konjo-ice rounded-md px-2 py-1.5 font-mono text-xs tabular-nums outline-none transition-colors"
      />
      {#each PRESETS as p}
        <button
          type="button"
          on:click={() => hourlyCap.set(p)}
          class="press font-mono text-[10.5px] px-3 py-1.5 rounded-[11px] border transition-colors"
          style:border-color={$hourlyCap === p ? '#00d4ff' : 'rgba(255,255,255,0.12)'}
          style:color={$hourlyCap === p ? '#00d4ff' : 'rgba(245,245,245,0.55)'}
        >
          ${p}
        </button>
      {/each}
    </div>

    <!-- Alert threshold -->
    <div class="flex items-center gap-2.5 mt-[18px]">
      <span class="font-mono text-[9.5px] tracking-[0.08em] uppercase text-white/40 flex-shrink-0">
        alert threshold
      </span>
      <input
        type="range"
        min="10"
        max="100"
        step="1"
        bind:value={$alertPct}
        class="flex-1 accent-konjo-flame"
      />
      <span class="font-mono text-[10.5px] text-konjo-flame w-[34px] text-right flex-shrink-0">
        {$alertPct}%
      </span>
    </div>
  </div>

  <!-- Breakdown: by repo / by model -->
  <div class="grid grid-cols-2 gap-3.5">
    <div class="relative bg-konjo-card border border-white/[0.14] rounded-[10px] pt-[22px] px-[18px] pb-4">
      <span
        class="absolute -top-[9px] left-4 bg-konjo-deep border border-white/[0.14] rounded-[3px] px-[9px] py-0.5 font-mono text-[9px] tracking-[0.1em] uppercase text-white/55"
      >
        by repo
      </span>
      <div class="flex flex-col gap-2.5 mt-2">
        {#if $byRepo.length === 0}
          <div class="font-mono text-[11px] opacity-30 py-2">no spend yet</div>
        {:else}
          {#each $byRepo as r (r.name)}
            <div class="flex items-center gap-2.5">
              <span class="w-[74px] font-mono text-[11px] text-white/65 flex-shrink-0 truncate">
                {r.name}
              </span>
              <div class="flex-1 h-1.5 rounded-[3px] bg-black/40 overflow-hidden">
                <div
                  class="h-full rounded-[3px] bg-konjo-teal"
                  style:width={`${(r.cost / byRepoMax) * 100}%`}
                ></div>
              </div>
              <span class="w-[52px] text-right font-mono text-[11px] text-konjo-teal flex-shrink-0">
                ${r.cost.toFixed(2)}
              </span>
            </div>
          {/each}
        {/if}
      </div>
    </div>
    <div class="relative bg-konjo-card border border-white/[0.14] rounded-[10px] pt-[22px] px-[18px] pb-4">
      <span
        class="absolute -top-[9px] left-4 bg-konjo-deep border border-white/[0.14] rounded-[3px] px-[9px] py-0.5 font-mono text-[9px] tracking-[0.1em] uppercase text-white/55"
      >
        by model
      </span>
      <div class="flex flex-col gap-2.5 mt-2">
        {#if $byModel.length === 0}
          <div class="font-mono text-[11px] opacity-30 py-2">no spend today</div>
        {:else}
          {#each $byModel as m (m.name)}
            <div class="flex items-center gap-2.5">
              <span class="w-[74px] font-mono text-[11px] text-white/65 flex-shrink-0 truncate">
                {m.name}
              </span>
              <div class="flex-1 h-1.5 rounded-[3px] bg-black/40 overflow-hidden">
                <div
                  class="h-full rounded-[3px] bg-konjo-violet-light"
                  style:width={`${(m.cost / byModelMax) * 100}%`}
                ></div>
              </div>
              <span
                class="w-[52px] text-right font-mono text-[11px] text-konjo-violet-light flex-shrink-0"
              >
                ${m.cost.toFixed(2)}
              </span>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </div>

  <!-- Top spenders -->
  <div class="relative bg-konjo-card border border-white/[0.14] rounded-[10px] pt-[22px] px-[18px] pb-4">
    <span
      class="absolute -top-[9px] left-4 bg-konjo-deep border border-konjo-sun/40 rounded-[3px] px-[9px] py-0.5 font-mono text-[9.5px] tracking-[0.1em] uppercase text-konjo-sun"
    >
      top spenders
    </span>
    {#if spenders.length === 0}
      <div class="font-mono text-[11px] opacity-30 py-4 text-center">no spend yet</div>
    {:else}
      <div class="flex flex-col gap-2 mt-2.5">
        {#each spenders as a (a.id)}
          <div
            class="flex items-center gap-3 px-3 py-2.5 rounded-r-[7px]"
            style:border-left={`3px solid ${a.status === 'running' ? '#00ff9d' : 'rgba(255,255,255,0.14)'}`}
            style:background={a.status === 'running' ? 'rgba(0,255,157,0.04)' : 'rgba(255,255,255,0.015)'}
          >
            <span
              class="w-[7px] h-[7px] rounded-full flex-shrink-0"
              class:animate-pulse={a.status === 'running'}
              style:background={a.status === 'running' ? '#00ff9d' : 'rgba(245,245,245,0.25)'}
            ></span>
            <div class="flex-1 min-w-0">
              <div class="font-mono text-xs text-white/85 truncate">{a.goal}</div>
              <div class="h-1 mt-1.5 rounded-full bg-black/40 overflow-hidden">
                <div
                  class="h-full rounded-full bg-konjo-sun"
                  style:width={`${(a.cost / maxSpenderCost) * 100}%`}
                ></div>
              </div>
            </div>
            <span class="font-mono text-xs tabular-nums text-konjo-sun w-[66px] text-right flex-shrink-0">
              ${a.cost.toFixed(4)}
            </span>
            <button
              type="button"
              on:click={() => cancelTask(a.id)}
              disabled={a.status !== 'running'}
              class="press w-[26px] h-[26px] flex items-center justify-center rounded-md border text-[9px] flex-shrink-0 transition-colors"
              style:border-color={a.status === 'running' ? 'rgba(255,0,102,0.4)' : 'rgba(255,255,255,0.1)'}
              style:color={a.status === 'running' ? '#ff0066' : 'rgba(245,245,245,0.2)'}
              title="Stop"
            >◼</button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
