<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { pulse, pulseCount, pulseKindCounts, type PulseEntry } from '$lib/stores/events';
  import { connectionState } from '$lib/stores/agents';
  import Panel from '$lib/components/ui/Panel.svelte';
  import StatCard from '$lib/components/ui/StatCard.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';

  let kindFilter = 'all';
  let paused = false;
  let autoScroll = true;
  let scrollEl: HTMLDivElement | null = null;
  let frozen: PulseEntry[] = [];

  // Pretty labels for the event kinds.
  const KIND_LABEL: Record<string, string> = {
    task_queued: 'queued',
    task_started: 'started',
    status_changed: 'status',
    log_line: 'log',
    score_updated: 'score',
    task_completed: 'done',
    task_cancelled: 'cancel',
    pool_stats: 'pool',
    turn_metrics: 'turn',
    verifier_verdict: 'verify',
    budget_exceeded: 'budget'
  };

  function tierColor(tier: PulseEntry['tier']): string {
    switch (tier) {
      case 'good':
        return 'var(--konjo-jade)';
      case 'warn':
        return 'var(--konjo-sun)';
      case 'bad':
        return 'var(--konjo-rose)';
      default:
        return 'var(--konjo-accent)';
    }
  }

  $: source = paused ? frozen : $pulse;
  $: filtered = kindFilter === 'all' ? source : source.filter((e) => e.kind === kindFilter);
  $: kinds = Object.entries($pulseKindCounts).sort((a, b) => b[1] - a[1]);

  $: if (autoScroll && !paused && filtered.length && scrollEl) {
    tick().then(() => {
      if (scrollEl) scrollEl.scrollTop = scrollEl.scrollHeight;
    });
  }

  function togglePause() {
    if (!paused) frozen = [...$pulse];
    paused = !paused;
  }

  function fmtClock(ts: number): string {
    return new Date(ts).toLocaleTimeString();
  }

  // Live throughput — events/sec over a 5s window.
  let rate = 0;
  let lastCount = 0;
  onMount(() => {
    lastCount = $pulseCount;
    const t = setInterval(() => {
      const now = $pulseCount;
      rate = (now - lastCount) / 5;
      lastCount = now;
    }, 5000);
    return () => clearInterval(t);
  });
</script>

<svelte:head><title>lopi · pulse</title></svelte:head>

<div class="max-w-6xl mx-auto px-6 py-8 space-y-6">
  <!-- Live throughput strip -->
  <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
    <StatCard label="events seen" value={$pulseCount.toLocaleString()} />
    <StatCard label="events / sec" value={rate.toFixed(1)} color="var(--konjo-jade)" />
    <StatCard label="in buffer" value={$pulse.length} />
    <StatCard
      label="stream"
      value={$connectionState}
      color={$connectionState === 'connected'
        ? 'var(--konjo-jade)'
        : $connectionState === 'mock'
          ? 'var(--konjo-sun)'
          : 'var(--konjo-rose)'}
    />
  </div>

  <!-- Kind histogram -->
  <Panel title="Event Mix" subtitle="live distribution by kind">
    {#if kinds.length === 0}
      <EmptyState title="no events yet" detail="the feed will light up as agents work" />
    {:else}
      {@const maxC = Math.max(...kinds.map(([, c]) => c))}
      <div class="space-y-1.5">
        {#each kinds as [kind, count] (kind)}
          <button
            type="button"
            on:click={() => (kindFilter = kindFilter === kind ? 'all' : kind)}
            class="w-full flex items-center gap-3 group"
          >
            <span
              class="font-mono text-[10px] uppercase tracking-widest w-16 text-right flex-shrink-0 transition-colors"
              class:text-konjo-accent={kindFilter === kind}
              class:opacity-50={kindFilter !== kind}
            >
              {KIND_LABEL[kind] ?? kind}
            </span>
            <div class="flex-1 h-3 bg-black/40 rounded-full overflow-hidden">
              <div
                class="h-full rounded-full transition-all duration-500 group-hover:brightness-125"
                style:width={`${(count / maxC) * 100}%`}
                style:background="var(--konjo-accent)"
                style:opacity={kindFilter === 'all' || kindFilter === kind ? 0.8 : 0.25}
              ></div>
            </div>
            <span class="font-mono text-[10px] tabular-nums w-10 opacity-60 flex-shrink-0">{count}</span>
          </button>
        {/each}
      </div>
    {/if}
  </Panel>

  <!-- Live feed -->
  <Panel
    title="Pulse"
    subtitle="{filtered.length} event{filtered.length === 1 ? '' : 's'}{kindFilter !== 'all' ? ` · ${KIND_LABEL[kindFilter] ?? kindFilter}` : ''}"
  >
    <svelte:fragment slot="actions">
      {#if kindFilter !== 'all'}
        <button
          type="button"
          on:click={() => (kindFilter = 'all')}
          class="font-mono text-[10px] uppercase tracking-widest opacity-50 hover:opacity-100 hover:text-konjo-accent transition-all px-2 py-1"
        >
          clear filter
        </button>
      {/if}
      <label class="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-widest opacity-50 cursor-pointer">
        <input type="checkbox" bind:checked={autoScroll} class="accent-[var(--konjo-accent)]" />
        follow
      </label>
      <button
        type="button"
        on:click={togglePause}
        class="px-2 py-1 rounded border font-mono text-[10px] uppercase tracking-widest transition-colors"
        class:text-konjo-sun={paused}
        class:opacity-50={!paused}
        style:border-color={paused ? 'var(--konjo-sun)' : 'rgba(255,255,255,0.1)'}
      >
        {paused ? '▶ resume' : '⏸ pause'}
      </button>
    </svelte:fragment>

    {#if filtered.length === 0}
      <EmptyState title="quiet for now" detail="every agent event streams here as it happens" />
    {:else}
      <div
        bind:this={scrollEl}
        class="bg-black/40 rounded p-3 h-[55vh] overflow-y-auto font-mono text-[11px] space-y-0.5"
      >
        {#each filtered as e (e.seq)}
          <div class="flex gap-2 items-baseline pulse-row" style:--tier={tierColor(e.tier)}>
            <span class="opacity-30 flex-shrink-0 tabular-nums">{fmtClock(e.ts)}</span>
            <span
              class="flex-shrink-0 w-14 uppercase tracking-wider text-[9px] px-1 rounded text-center self-center"
              style:color={tierColor(e.tier)}
              style:background={`color-mix(in srgb, ${tierColor(e.tier)} 12%, transparent)`}
            >
              {KIND_LABEL[e.kind] ?? e.kind}
            </span>
            {#if e.taskId}
              <span class="opacity-40 flex-shrink-0">{e.taskId.slice(0, 8)}</span>
            {/if}
            <span class="break-words opacity-85 min-w-0" style:color={e.tier === 'bad' ? tierColor(e.tier) : 'inherit'}>
              {e.summary}
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </Panel>
</div>

<style>
  .pulse-row {
    animation: pulse-in 0.3s cubic-bezier(0.16, 1, 0.3, 1) both;
    border-left: 2px solid transparent;
    border-image: linear-gradient(to bottom, var(--tier), transparent) 1;
    padding-left: 6px;
    margin-left: -8px;
  }
  @keyframes pulse-in {
    from {
      opacity: 0;
      transform: translateX(-6px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
</style>
