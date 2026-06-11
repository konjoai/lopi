<script lang="ts">
  import { onMount } from 'svelte';
  import {
    getStats,
    healthSummary,
    cacheStats,
    clearCache,
    queryAudit,
    listPatterns,
    rawGet,
    type PoolStatsResponse,
    type HealthSummary,
    type AuditEvent,
    type PatternRow
  } from '$lib/api';
  import Panel from '$lib/components/ui/Panel.svelte';
  import StatCard from '$lib/components/ui/StatCard.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';

  let stats: PoolStatsResponse | null = null;
  let health: HealthSummary | null = null;
  let cache: Record<string, unknown> | null = null;
  let audit: AuditEvent[] = [];
  let patterns: PatternRow[] = [];
  let loadError = '';
  let loading = true;
  let cacheFlash = '';

  // API console
  const KNOWN_ENDPOINTS = [
    '/api/health',
    '/api/stats',
    '/api/tasks',
    '/api/logs',
    '/api/schedules',
    '/api/agents/health/summary',
    '/api/cache/stats',
    '/api/audit',
    '/api/patterns',
    '/api/plans',
    '/api/quality/trend',
    '/api/tasks/dead-letter',
    '/api/tools',
    '/api/constellations',
    '/api/config',
    '/api/version'
  ];
  let consolePath = '/api/health';
  let consoleResult = '';
  let consoleStatus: 'idle' | 'loading' | 'ok' | 'error' = 'idle';

  async function refresh() {
    const results = await Promise.allSettled([
      getStats(),
      healthSummary(),
      cacheStats(),
      queryAudit(50),
      listPatterns()
    ]);
    const [s, h, c, a, p] = results;
    if (s.status === 'fulfilled') stats = s.value;
    if (h.status === 'fulfilled') health = h.value;
    if (c.status === 'fulfilled') cache = c.value;
    if (a.status === 'fulfilled') audit = a.value.events.slice().reverse();
    if (p.status === 'fulfilled') patterns = p.value.patterns;
    loadError =
      results.every((r) => r.status === 'rejected') && results[0].status === 'rejected'
        ? results[0].reason instanceof Error
          ? results[0].reason.message
          : 'failed to load'
        : '';
    loading = false;
  }

  async function handleClearCache() {
    try {
      const r = await clearCache();
      cacheFlash = `cleared ${r.deleted} entries`;
      setTimeout(() => (cacheFlash = ''), 2500);
      await refresh();
    } catch (e) {
      cacheFlash = e instanceof Error ? e.message : 'clear failed';
    }
  }

  async function runConsole() {
    if (!consolePath.startsWith('/api/') && consolePath !== '/metrics') {
      consoleStatus = 'error';
      consoleResult = 'only /api/* paths (and /metrics) are allowed';
      return;
    }
    consoleStatus = 'loading';
    try {
      const r = await rawGet(consolePath);
      consoleResult = JSON.stringify(r, null, 2);
      consoleStatus = 'ok';
    } catch (e) {
      consoleResult = e instanceof Error ? e.message : 'request failed';
      consoleStatus = 'error';
    }
  }

  function fmtUptime(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  }

  function fmtTime(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
  }

  onMount(() => {
    refresh();
    const t = setInterval(refresh, 10000);
    return () => clearInterval(t);
  });
</script>

<svelte:head><title>lopi · debug</title></svelte:head>

<div class="max-w-6xl mx-auto px-6 py-8 space-y-6">
  {#if loading}
    <Panel title="Status"><EmptyState title="loading…" /></Panel>
  {:else if loadError}
    <Panel title="Status"><EmptyState error title="backend unreachable" detail={loadError} /></Panel>
  {:else}
    <!-- Pool stats -->
    {#if stats}
      <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-7 gap-3">
        <StatCard label="running" value={stats.running} color="var(--konjo-jade)" />
        <StatCard label="queued" value={stats.queued} color="var(--konjo-sun)" />
        <StatCard label="succeeded" value={stats.succeeded} color="var(--konjo-jade)" />
        <StatCard label="failed" value={stats.failed} color="var(--konjo-rose)" />
        <StatCard label="uptime" value={fmtUptime(stats.uptime_secs)} />
        <StatCard label="tokens today" value={stats.total_tokens_today.toLocaleString()} />
        <StatCard
          label="cost today"
          value={`$${stats.total_cost_usd_today.toFixed(2)}`}
          color="var(--konjo-flame)"
        />
      </div>
    {/if}

    <!-- Agent health -->
    <Panel title="Agent Health" subtitle="heartbeat classification">
      {#if health && health.total > 0}
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <StatCard label="total" value={health.total} />
          <StatCard label="healthy" value={health.healthy} color="var(--konjo-jade)" />
          <StatCard label="degraded" value={health.degraded} color="var(--konjo-sun)" />
          <StatCard label="dead" value={health.dead} color="var(--konjo-rose)" />
        </div>
      {:else}
        <EmptyState title="no heartbeats yet" detail="agents report in once they start working" />
      {/if}
    </Panel>

    <!-- Cache -->
    <Panel title="Plan Cache" subtitle="semantic plan reuse">
      <svelte:fragment slot="actions">
        {#if cacheFlash}
          <span class="font-mono text-[10px] uppercase tracking-widest text-konjo-jade">
            {cacheFlash}
          </span>
        {/if}
        <button
          type="button"
          on:click={handleClearCache}
          class="px-2 py-1 rounded border border-white/10 font-mono text-[10px] uppercase tracking-widest text-konjo-rose hover:border-konjo-rose/50 hover:bg-konjo-rose/10 transition-colors"
        >
          clear
        </button>
      </svelte:fragment>
      {#if cache}
        <div class="flex flex-wrap gap-6 font-mono text-sm">
          {#each Object.entries(cache) as [k, v] (k)}
            <div>
              <span class="opacity-40 text-[10px] uppercase tracking-widest block">{k}</span>
              <span class="tabular-nums">{typeof v === 'number' ? v.toLocaleString() : String(v)}</span>
            </div>
          {/each}
        </div>
      {:else}
        <EmptyState title="cache stats unavailable" />
      {/if}
    </Panel>

    <!-- Learned patterns -->
    <Panel title="Patterns" subtitle="what the memory layer has learned">
      {#if patterns.length === 0}
        <EmptyState title="no patterns yet" detail="completed runs feed the pattern store" />
      {:else}
        <div class="overflow-x-auto -mx-4 -my-4">
          <table class="w-full text-left font-mono text-xs">
            <thead>
              <tr class="border-b border-white/5 text-[9px] uppercase tracking-widest opacity-40">
                <th class="px-4 py-2 font-normal">keywords</th>
                <th class="px-4 py-2 font-normal text-right">avg attempts</th>
                <th class="px-4 py-2 font-normal text-right">success rate</th>
                <th class="px-4 py-2 font-normal">last seen</th>
              </tr>
            </thead>
            <tbody>
              {#each patterns as p (p.id)}
                <tr class="border-b border-white/5 hover:bg-white/5 transition-colors">
                  <td class="px-4 py-2 max-w-sm truncate">{p.goal_keywords}</td>
                  <td class="px-4 py-2 text-right tabular-nums">{p.avg_attempts.toFixed(1)}</td>
                  <td
                    class="px-4 py-2 text-right tabular-nums"
                    style:color={p.success_rate >= 0.8
                      ? 'var(--konjo-jade)'
                      : p.success_rate >= 0.5
                        ? 'var(--konjo-sun)'
                        : 'var(--konjo-rose)'}
                  >
                    {Math.round(p.success_rate * 100)}%
                  </td>
                  <td class="px-4 py-2 opacity-50">{fmtTime(p.last_seen)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </Panel>

    <!-- Audit trail -->
    <Panel title="Audit Trail" subtitle="last {audit.length} control-plane events">
      {#if audit.length === 0}
        <EmptyState title="no audit events" detail="mutations to tasks + schedules land here" />
      {:else}
        <div class="bg-black/40 rounded p-3 max-h-72 overflow-y-auto font-mono text-[11px] space-y-1">
          {#each audit as ev (ev.id)}
            <div class="flex gap-2 items-baseline">
              <span class="opacity-30 flex-shrink-0 tabular-nums">{fmtTime(ev.ts)}</span>
              <span class="text-konjo-accent flex-shrink-0">{ev.action}</span>
              <span class="opacity-50">{ev.subject_type}/{ev.subject_id.slice(0, 8)}</span>
              <span class="opacity-30">by {ev.actor}</span>
            </div>
          {/each}
        </div>
      {/if}
    </Panel>
  {/if}

  <!-- API console — manual endpoint probing (read-only) -->
  <Panel title="API Console" subtitle="manual GET probe · read-only">
    <div class="flex gap-2 mb-3">
      <input
        type="text"
        bind:value={consolePath}
        list="known-endpoints"
        on:keydown={(e) => {
          if (e.key === 'Enter') runConsole();
        }}
        class="flex-1 bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 transition-colors"
      />
      <datalist id="known-endpoints">
        {#each KNOWN_ENDPOINTS as ep (ep)}
          <option value={ep}></option>
        {/each}
      </datalist>
      <button
        type="button"
        on:click={runConsole}
        class="px-4 py-1 rounded font-mono text-xs uppercase tracking-widest bg-konjo-accent/10 text-konjo-accent border border-konjo-accent/40 hover:bg-konjo-accent/20 transition-colors"
      >
        {consoleStatus === 'loading' ? '…' : 'GET'}
      </button>
    </div>
    {#if consoleResult}
      <pre
        class="bg-black/40 rounded p-3 overflow-x-auto font-mono text-[11px] max-h-80 overflow-y-auto leading-relaxed"
        style:color={consoleStatus === 'error' ? 'var(--konjo-rose)' : 'inherit'}>{consoleResult}</pre>
    {/if}
  </Panel>
</div>
