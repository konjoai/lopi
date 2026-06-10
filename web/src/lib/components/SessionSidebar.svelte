<script lang="ts">
  import { onMount } from 'svelte';
  import { agents, listHistory, type HistoryTask } from '$lib/stores/agents';

  /** Two-way bound by `+layout.svelte`. */
  export let open: boolean = false;

  let history: HistoryTask[] = [];
  let loading = false;
  let filter = '';

  async function refresh() {
    loading = true;
    try {
      history = await listHistory();
    } finally {
      loading = false;
    }
  }

  onMount(refresh);

  // Refresh whenever the sidebar opens — keeps the list fresh.
  $: if (open) {
    void refresh();
  }

  // Live (running/queued) entries first, then the historical roll.
  $: liveEntries = Array.from($agents.values())
    .filter((a) => a.status === 'running' || a.status === 'queued')
    .map((a) => ({
      id: a.id,
      goal: a.goal,
      status: a.status,
      repo: a.repo,
      live: true as const
    }));

  $: historyEntries = history
    .filter((t) => !$agents.has(t.id))
    .map((t) => ({ ...t, repo: '', live: false as const }));

  $: filterLower = filter.trim().toLowerCase();
  $: matches = (text: string) => !filterLower || text.toLowerCase().includes(filterLower);

  function pickLive(id: string) {
    window.dispatchEvent(new CustomEvent('lopi:focus-agent', { detail: { id } }));
    open = false;
  }

  function reopen(t: HistoryTask) {
    window.dispatchEvent(
      new CustomEvent('lopi:reopen-task', { detail: { goal: t.goal } })
    );
    open = false;
  }

  function statusColor(s: string): string {
    if (s === 'running') return 'var(--konjo-jade)';
    if (s === 'queued') return 'var(--konjo-sun)';
    if (s === 'completed' || s === 'succeeded') return 'rgba(0, 212, 138, 0.5)';
    if (s === 'failed') return 'var(--konjo-rose)';
    return 'rgba(255,255,255,0.3)';
  }
</script>

<!-- Inline sidebar — animates its own width so the AgentGrid reflows
     beside it instead of being covered by an overlay. -->
<aside
  class="h-full flex-shrink-0 overflow-hidden border-r border-white/10 bg-konjo-deep/95 backdrop-blur-md transition-[width] duration-200 ease-out"
  style:width={open ? '20rem' : '0'}
  aria-hidden={!open}
>
  <div class="w-80 h-full flex flex-col">
    <div
      class="px-4 py-3 border-b border-white/10 flex items-center justify-between flex-shrink-0"
    >
      <h2 class="font-mono text-xs uppercase tracking-widest opacity-70">sessions</h2>
      <button
        type="button"
        on:click={() => void refresh()}
        class="font-mono text-[10px] uppercase tracking-widest opacity-50 hover:opacity-100 transition-opacity"
        title="Refresh"
        disabled={loading}
      >
        {loading ? '…' : '↻'}
      </button>
    </div>

    <div class="px-3 py-2 border-b border-white/5 flex-shrink-0">
      <input
        type="text"
        bind:value={filter}
        placeholder="filter…"
        class="w-full bg-black/40 border border-white/10 focus:border-konjo-ice outline-none rounded px-2 py-1 text-xs font-mono placeholder:opacity-30 text-konjo-paper"
      />
    </div>

    <div class="flex-1 overflow-y-auto">
      {#if liveEntries.length > 0}
        <div class="px-4 pt-3 pb-1 font-mono text-[9px] uppercase tracking-widest opacity-40">
          live ({liveEntries.length})
        </div>
        {#each liveEntries as e (e.id)}
          {#if matches(e.goal)}
            <button
              type="button"
              on:click={() => pickLive(e.id)}
              class="w-full text-left px-4 py-2 hover:bg-white/5 transition-colors border-b border-white/5"
            >
              <div class="flex items-center gap-2">
                <span
                  class="w-2 h-2 rounded-full flex-shrink-0 animate-pulse"
                  style:background={statusColor(e.status)}
                ></span>
                <span class="font-mono text-xs text-konjo-paper truncate flex-1">{e.goal}</span>
              </div>
              {#if e.repo}
                <div class="font-mono text-[9px] opacity-40 truncate mt-0.5 ml-4">{e.repo}</div>
              {/if}
            </button>
          {/if}
        {/each}
      {/if}

      {#if historyEntries.length > 0}
        <div class="px-4 pt-3 pb-1 font-mono text-[9px] uppercase tracking-widest opacity-40">
          history ({historyEntries.length})
        </div>
        {#each historyEntries as e (e.id)}
          {#if matches(e.goal)}
            <button
              type="button"
              on:click={() => reopen(e)}
              class="w-full text-left px-4 py-2 hover:bg-white/5 transition-colors border-b border-white/5"
              title="Click to re-run this task in a new pane"
            >
              <div class="flex items-center gap-2">
                <span
                  class="w-2 h-2 rounded-full flex-shrink-0"
                  style:background={statusColor(e.status)}
                ></span>
                <span class="font-mono text-xs text-konjo-paper/80 truncate flex-1">{e.goal}</span>
              </div>
              <div class="font-mono text-[9px] opacity-40 mt-0.5 ml-4 uppercase tracking-widest">
                {e.status}
              </div>
            </button>
          {/if}
        {/each}
      {/if}

      {#if !loading && liveEntries.length === 0 && historyEntries.length === 0}
        <div class="px-4 py-8 text-center font-mono text-[10px] opacity-30 uppercase tracking-widest">
          no sessions yet
        </div>
      {/if}
    </div>
  </div>
</aside>
