<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { recentLogs, type LogRow } from '$lib/api';
  import { logs as liveLogs, type LogEntry } from '$lib/stores/agents';
  import Panel from '$lib/components/ui/Panel.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { levelColor } from '$lib/components/ui/badges';

  interface DisplayLog {
    key: string;
    ts: number;
    taskId: string;
    level: string;
    line: string;
  }

  let historical: DisplayLog[] = [];
  let loadError = '';
  let loading = true;

  // Controls
  let levelFilter = 'all';
  let search = '';
  let paused = false;
  let autoScroll = true;
  let mountedAt = Date.now();

  let scrollEl: HTMLDivElement | null = null;
  // Frozen view while paused — live updates buffer behind the scenes.
  let frozen: DisplayLog[] = [];

  function fromRow(r: LogRow): DisplayLog {
    return {
      key: `h-${r.id}`,
      ts: Date.parse(r.ts) || 0,
      taskId: r.task_id,
      level: r.level,
      line: r.line
    };
  }

  function fromLive(l: LogEntry, i: number): DisplayLog {
    return { key: `l-${l.ts}-${i}`, ts: l.ts, taskId: l.taskId, level: l.level, line: l.message };
  }

  // Live entries that arrived after mount (historical fetch covers the rest).
  $: liveAfterMount = $liveLogs
    .map((l, i) => ({ l, i }))
    .filter(({ l }) => l.ts >= mountedAt)
    .map(({ l, i }) => fromLive(l, i));

  $: combined = [...historical, ...liveAfterMount];

  $: filtered = (paused ? frozen : combined).filter((d) => {
    if (levelFilter !== 'all' && d.level !== levelFilter) return false;
    if (search && !`${d.taskId} ${d.line}`.toLowerCase().includes(search.toLowerCase()))
      return false;
    return true;
  });

  $: if (paused === false) frozen = combined;

  function togglePause() {
    if (!paused) frozen = combined;
    paused = !paused;
  }

  // Auto-scroll on new entries
  $: if (autoScroll && !paused && filtered.length && scrollEl) {
    tick().then(() => {
      if (scrollEl) scrollEl.scrollTop = scrollEl.scrollHeight;
    });
  }

  function exportLogs() {
    const text = filtered
      .map(
        (d) =>
          `${new Date(d.ts).toISOString()} [${d.level.toUpperCase()}] ${d.taskId.slice(0, 8)} ${d.line}`
      )
      .join('\n');
    const blob = new Blob([text], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `lopi-logs-${new Date().toISOString().replace(/[:.]/g, '-')}.log`;
    a.click();
    URL.revokeObjectURL(url);
  }

  onMount(async () => {
    mountedAt = Date.now();
    try {
      const r = await recentLogs(1000);
      historical = r.logs.map(fromRow);
      loadError = '';
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  });

  const LEVELS = ['all', 'info', 'warn', 'error', 'debug'];
</script>

<svelte:head><title>lopi · logs</title></svelte:head>

<div class="max-w-6xl mx-auto px-6 py-8">
  <Panel
    title="Live Logs"
    subtitle="{filtered.length} line{filtered.length === 1 ? '' : 's'} · global tail across all tasks"
  >
    <svelte:fragment slot="actions">
      <button
        type="button"
        on:click={togglePause}
        class="px-2 py-1 rounded border font-mono text-[10px] uppercase tracking-widest transition-colors"
        class:text-konjo-sun={paused}
        class:border-konjo-sun={paused}
        class:opacity-50={!paused}
        style:border-color={paused ? 'var(--konjo-sun)' : 'rgba(255,255,255,0.1)'}
      >
        {paused ? '▶ resume' : '⏸ pause'}
      </button>
      <button
        type="button"
        on:click={exportLogs}
        class="px-2 py-1 rounded border border-white/10 font-mono text-[10px] uppercase tracking-widest opacity-50 hover:opacity-100 hover:text-konjo-accent hover:border-konjo-accent/50 transition-all"
      >
        ↓ export
      </button>
    </svelte:fragment>

    <!-- Filter bar -->
    <div class="flex flex-wrap items-center gap-3 mb-4">
      <div class="flex gap-1">
        {#each LEVELS as lvl (lvl)}
          <button
            type="button"
            on:click={() => (levelFilter = lvl)}
            class="px-2 py-0.5 rounded font-mono text-[10px] uppercase tracking-widest border transition-colors"
            class:opacity-40={levelFilter !== lvl}
            style:color={lvl === 'all' ? 'var(--konjo-paper)' : levelColor(lvl)}
            style:border-color={levelFilter === lvl
              ? lvl === 'all'
                ? 'var(--konjo-accent)'
                : levelColor(lvl)
              : 'rgba(255,255,255,0.1)'}
          >
            {lvl}
          </button>
        {/each}
      </div>
      <input
        type="text"
        bind:value={search}
        placeholder="filter…"
        class="flex-1 min-w-32 bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-xs font-mono placeholder:opacity-30 py-1 transition-colors"
      />
      <label class="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-widest opacity-50 cursor-pointer">
        <input type="checkbox" bind:checked={autoScroll} class="accent-[var(--konjo-accent)]" />
        follow
      </label>
    </div>

    {#if loading}
      <EmptyState title="loading…" />
    {:else if loadError && filtered.length === 0}
      <EmptyState error title="backend unreachable" detail={loadError} />
    {:else if filtered.length === 0}
      <EmptyState title="no log lines" detail="agents will speak here as they work" />
    {:else}
      <div
        bind:this={scrollEl}
        class="bg-black/40 rounded p-3 h-[60vh] overflow-y-auto font-mono text-[11px] space-y-0.5"
      >
        {#each filtered as d (d.key)}
          <div class="flex gap-2 log-line">
            <span class="opacity-30 flex-shrink-0 tabular-nums">
              {new Date(d.ts).toLocaleTimeString()}
            </span>
            <span class="flex-shrink-0 w-10 uppercase" style:color={levelColor(d.level)}>
              {d.level}
            </span>
            <span class="opacity-40 flex-shrink-0">{d.taskId.slice(0, 8)}</span>
            <span class="break-words opacity-80 min-w-0">{d.line}</span>
          </div>
        {/each}
      </div>
    {/if}
  </Panel>
</div>

<style>
  .log-line {
    animation: log-in 0.25s ease-out both;
  }
  @keyframes log-in {
    from {
      opacity: 0;
      transform: translateX(-4px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
</style>
