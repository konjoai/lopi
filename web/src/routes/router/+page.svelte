<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    listConstellations,
    registerConstellation,
    dispatchConstellation,
    constellationStats,
    type Constellation,
    type ConstellationMember,
    type ConstellationStats,
    type DispatchDecision,
    type RoutingStrategy
  } from '$lib/api';
  import Panel from '$lib/components/ui/Panel.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';

  let constellations: Constellation[] = [];
  let stats: Record<string, ConstellationStats> = {};
  let loading = true;
  let loadError = '';
  let actionError = '';
  let flash = '';
  let lastDecision: Record<string, DispatchDecision> = {};
  let statsTimer: ReturnType<typeof setInterval> | null = null;

  // ── Register form ───────────────────────────────────────────────────────────
  let formOpen = false;
  let name = '';
  let strategy: 'RoundRobin' | 'WeightedRandom' | 'LeastLoaded' | 'TagMatch' = 'LeastLoaded';
  let tagMatchTags = '';
  let members: ConstellationMember[] = [blankMember()];
  let dispatchTags: Record<string, string> = {};
  let saving = false;

  function blankMember(): ConstellationMember {
    return { agent_id: '', weight: 1, tags: [], max_concurrent: 0 };
  }

  const STRATEGY_KIND = {
    RoundRobin: 'round_robin',
    WeightedRandom: 'weighted_random',
    LeastLoaded: 'least_loaded',
    TagMatch: 'tag_match'
  } as const;

  function strategyValue(): RoutingStrategy {
    if (strategy === 'TagMatch') {
      return {
        kind: 'tag_match',
        required_tags: tagMatchTags
          .split(',')
          .map((t) => t.trim())
          .filter(Boolean)
      };
    }
    return { kind: STRATEGY_KIND[strategy] } as RoutingStrategy;
  }

  // Pretty label from the wire shape ({kind: 'tag_match', required_tags}).
  function strategyLabel(s: RoutingStrategy): string {
    const pretty: Record<string, string> = {
      round_robin: 'RoundRobin',
      weighted_random: 'WeightedRandom',
      least_loaded: 'LeastLoaded',
      tag_match: 'TagMatch'
    };
    const base = pretty[s.kind] ?? s.kind;
    if (s.kind === 'tag_match') return `${base} [${s.required_tags.join(', ')}]`;
    return base;
  }

  async function refresh() {
    try {
      const r = await listConstellations();
      constellations = r.constellations;
      loadError = '';
      await refreshStats();
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  }

  async function refreshStats() {
    const entries = await Promise.allSettled(
      constellations.map((c) => constellationStats(c.name))
    );
    const next: Record<string, ConstellationStats> = {};
    entries.forEach((res, i) => {
      if (res.status === 'fulfilled') next[constellations[i].name] = res.value;
    });
    stats = next;
  }

  async function save() {
    const cleanMembers = members
      .map((m) => ({ ...m, agent_id: m.agent_id.trim() }))
      .filter((m) => m.agent_id);
    if (!name.trim() || cleanMembers.length === 0 || saving) {
      actionError = 'name and at least one member with an agent id are required';
      return;
    }
    saving = true;
    actionError = '';
    try {
      await registerConstellation({
        name: name.trim(),
        agents: cleanMembers,
        routing_strategy: strategyValue()
      });
      flash = `${name.trim()} registered`;
      setTimeout(() => (flash = ''), 2500);
      formOpen = false;
      name = '';
      members = [blankMember()];
      await refresh();
    } catch (e) {
      actionError = e instanceof Error ? e.message : 'register failed';
    } finally {
      saving = false;
    }
  }

  async function dispatch(c: Constellation) {
    actionError = '';
    const tags = (dispatchTags[c.name] ?? '')
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean);
    try {
      const decision = await dispatchConstellation(c.name, tags);
      lastDecision[c.name] = decision;
      lastDecision = lastDecision;
      await refreshStats();
    } catch (e) {
      actionError = e instanceof Error ? e.message : 'dispatch failed';
    }
  }

  function addMember() {
    members = [...members, blankMember()];
  }
  function removeMember(i: number) {
    members = members.filter((_, idx) => idx !== i);
  }
  function setTags(m: ConstellationMember, value: string) {
    m.tags = value
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean);
  }

  function fmtTime(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleTimeString();
  }

  onMount(() => {
    refresh();
    statsTimer = setInterval(refreshStats, 4000);
  });
  onDestroy(() => {
    if (statsTimer) clearInterval(statsTimer);
  });
</script>

<svelte:head><title>lopi · router</title></svelte:head>

<div class="max-w-6xl mx-auto px-6 py-8 space-y-6">
  <Panel
    title="Constellation Router"
    subtitle="dispatch strategies across agent pools · {constellations.length} registered"
  >
    <svelte:fragment slot="actions">
      {#if flash}
        <span class="font-mono text-[10px] uppercase tracking-widest text-konjo-jade animate-pulse">
          {flash}
        </span>
      {/if}
      <button
        type="button"
        on:click={() => (formOpen = !formOpen)}
        class="px-3 py-1 rounded font-mono text-[10px] uppercase tracking-widest bg-konjo-accent/10 text-konjo-accent border border-konjo-accent/40 hover:bg-konjo-accent/20 transition-colors"
      >
        {formOpen ? 'close' : '+ register'}
      </button>
    </svelte:fragment>

    {#if actionError}
      <p class="mb-3 font-mono text-xs" style:color="var(--konjo-rose)">{actionError}</p>
    {/if}

    {#if loading}
      <EmptyState title="loading…" />
    {:else if loadError}
      <EmptyState error title="backend unreachable" detail={loadError} />
    {:else if constellations.length === 0 && !formOpen}
      <EmptyState
        title="no constellations"
        detail="register a pool of agents and a routing strategy to dispatch across them"
      />
    {:else}
      <div class="space-y-4">
        {#each constellations as c (c.name)}
          {@const st = stats[c.name]}
          {@const maxLoad = st ? Math.max(1, ...st.members.map((m) => m.dispatched_total)) : 1}
          <div class="rounded-lg border border-white/10 overflow-hidden">
            <!-- Header -->
            <div class="px-4 py-3 border-b border-white/5 flex items-center justify-between gap-3 bg-black/20">
              <div class="flex items-center gap-2 min-w-0">
                <span class="font-display text-sm font-bold">{c.name}</span>
                <span
                  class="font-mono text-[9px] uppercase tracking-widest px-1.5 py-0.5 rounded bg-konjo-accent/10 text-konjo-accent border border-konjo-accent/30"
                >
                  {strategyLabel(c.routing_strategy)}
                </span>
                <span class="font-mono text-[10px] opacity-40">{c.agents.length} member{c.agents.length === 1 ? '' : 's'}</span>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0">
                <input
                  type="text"
                  placeholder="tags…"
                  bind:value={dispatchTags[c.name]}
                  class="w-24 bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-[11px] font-mono py-0.5 placeholder:opacity-30 transition-colors"
                />
                <button
                  type="button"
                  on:click={() => dispatch(c)}
                  class="px-3 py-1 rounded font-mono text-[10px] uppercase tracking-widest bg-konjo-jade/10 text-konjo-jade border border-konjo-jade/40 hover:bg-konjo-jade/20 transition-colors"
                >
                  ▶ dispatch
                </button>
              </div>
            </div>

            <!-- Live decision flash -->
            {#if lastDecision[c.name]}
              {@const d = lastDecision[c.name]}
              <div class="px-4 py-2 bg-konjo-jade/5 border-b border-konjo-jade/10 decision-flash">
                <span class="font-mono text-[11px]">
                  <span class="text-konjo-jade">→ {d.agent_id}</span>
                  <span class="opacity-40"> via {d.strategy} at {fmtTime(d.at)}</span>
                </span>
              </div>
            {/if}

            <!-- Member load bars (live) -->
            <div class="px-4 py-3 space-y-2">
              {#each c.agents as m (m.agent_id)}
                {@const load = st?.members.find((x) => x.agent_id === m.agent_id)}
                <div class="flex items-center gap-3">
                  <span class="font-mono text-[11px] w-28 truncate flex-shrink-0">{m.agent_id}</span>
                  {#if m.tags.length > 0}
                    <span class="font-mono text-[9px] opacity-40 flex-shrink-0">
                      [{m.tags.join(', ')}]
                    </span>
                  {/if}
                  <div class="flex-1 h-2.5 bg-black/40 rounded-full overflow-hidden">
                    <div
                      class="h-full rounded-full transition-all duration-500"
                      style:width={`${((load?.dispatched_total ?? 0) / maxLoad) * 100}%`}
                      style:background="var(--konjo-accent)"
                      style:opacity={load && load.in_flight > 0 ? 1 : 0.45}
                    ></div>
                  </div>
                  <span class="font-mono text-[10px] tabular-nums opacity-60 w-24 text-right flex-shrink-0">
                    {load?.dispatched_total ?? 0} total
                    {#if load && load.in_flight > 0}
                      · <span class="text-konjo-jade">{load.in_flight} live</span>
                    {/if}
                  </span>
                </div>
              {/each}
            </div>

            <!-- Recent decisions -->
            {#if st && st.recent_decisions.length > 0}
              <div class="px-4 py-2 border-t border-white/5 bg-black/20">
                <div class="font-mono text-[9px] uppercase tracking-widest opacity-40 mb-1">
                  recent dispatches
                </div>
                <div class="flex flex-wrap gap-1.5">
                  {#each st.recent_decisions.slice(0, 12) as d, i (i)}
                    <span class="font-mono text-[10px] px-1.5 py-0.5 rounded bg-white/5 opacity-70">
                      {d.agent_id}
                    </span>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </Panel>

  {#if formOpen}
    <Panel title="Register constellation" subtitle="define a pool + routing strategy">
      <div class="space-y-4">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <label class="flex flex-col gap-1">
            <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">name</span>
            <input
              type="text"
              bind:value={name}
              placeholder="backend-pool"
              class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 placeholder:opacity-30 transition-colors"
            />
          </label>
          <label class="flex flex-col gap-1">
            <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">strategy</span>
            <select
              bind:value={strategy}
              class="bg-konjo-deep border border-white/10 rounded px-2 py-1.5 font-mono text-xs outline-none focus:border-konjo-accent"
            >
              <option value="RoundRobin">RoundRobin</option>
              <option value="WeightedRandom">WeightedRandom</option>
              <option value="LeastLoaded">LeastLoaded</option>
              <option value="TagMatch">TagMatch</option>
            </select>
          </label>
        </div>

        {#if strategy === 'TagMatch'}
          <label class="flex flex-col gap-1">
            <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">
              required tags (comma-separated)
            </span>
            <input
              type="text"
              bind:value={tagMatchTags}
              placeholder="rust, backend"
              class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 placeholder:opacity-30 transition-colors"
            />
          </label>
        {/if}

        <div>
          <div class="flex items-center justify-between mb-2">
            <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">members</span>
            <button
              type="button"
              on:click={addMember}
              class="font-mono text-[10px] uppercase tracking-widest text-konjo-accent hover:opacity-70 transition-opacity"
            >
              + add member
            </button>
          </div>
          <div class="space-y-2">
            {#each members as m, i (i)}
              <div class="flex flex-wrap items-center gap-2 rounded border border-white/5 p-2">
                <input
                  type="text"
                  bind:value={m.agent_id}
                  placeholder="agent-id"
                  class="flex-1 min-w-28 bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-xs font-mono py-1 placeholder:opacity-30 transition-colors"
                />
                <label class="flex items-center gap-1 font-mono text-[10px] opacity-60">
                  w
                  <input
                    type="number"
                    bind:value={m.weight}
                    step="0.5"
                    min="0"
                    class="w-14 bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-xs font-mono py-1 transition-colors"
                  />
                </label>
                <label class="flex items-center gap-1 font-mono text-[10px] opacity-60">
                  max
                  <input
                    type="number"
                    bind:value={m.max_concurrent}
                    min="0"
                    class="w-14 bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-xs font-mono py-1 transition-colors"
                  />
                </label>
                <input
                  type="text"
                  on:input={(e) => setTags(m, e.currentTarget.value)}
                  placeholder="tags…"
                  class="w-32 bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-xs font-mono py-1 placeholder:opacity-30 transition-colors"
                />
                {#if members.length > 1}
                  <button
                    type="button"
                    on:click={() => removeMember(i)}
                    class="text-konjo-rose hover:opacity-70 font-mono text-sm px-1"
                    aria-label="Remove member"
                  >
                    ✕
                  </button>
                {/if}
              </div>
            {/each}
          </div>
        </div>

        <button
          type="button"
          on:click={save}
          disabled={saving}
          class="px-4 py-1.5 rounded font-mono text-xs uppercase tracking-widest bg-konjo-accent/10 text-konjo-accent border border-konjo-accent/40 hover:bg-konjo-accent/20 disabled:opacity-30 transition-colors"
        >
          {saving ? 'registering…' : 'register'}
        </button>
      </div>
    </Panel>
  {/if}
</div>

<style>
  .decision-flash {
    animation: flash-in 0.5s ease-out both;
  }
  @keyframes flash-in {
    0% {
      opacity: 0;
      background: rgb(var(--konjo-accent-rgb) / 0.25);
    }
    100% {
      opacity: 1;
    }
  }
</style>
