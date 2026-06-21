<!--
  Fleet — the Command Center. Every agent as a card on a Kanban board, columns
  by lifecycle phase. The third lens on the fleet alongside Forge (orbs) and
  Constellation (orbital): a dense, scannable status board with branch, score,
  spend and elapsed per agent. Click a card to focus it in the Forge.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { flip } from 'svelte/animate';
  import { agents, activeAgentId, PHASE_COLORS, type AgentState } from '$lib/stores/agents';

  // Lifecycle columns, left → right. Each agent lands in exactly one.
  const COLUMNS = [
    { key: 'queued', label: 'Queued', color: 'rgba(245,245,245,0.5)' },
    { key: 'planning', label: 'Planning', color: 'var(--konjo-ice)' },
    { key: 'implementing', label: 'Implementing', color: 'var(--konjo-ember)' },
    { key: 'testing', label: 'Testing', color: 'var(--konjo-sun)' },
    { key: 'review', label: 'Review', color: '#00ffd4' },
    { key: 'done', label: 'Done', color: 'var(--konjo-jade)' },
    { key: 'failed', label: 'Failed', color: 'var(--konjo-rose)' }
  ] as const;

  type ColKey = (typeof COLUMNS)[number]['key'];

  function columnFor(a: AgentState): ColKey {
    if (a.status === 'completed') return 'done';
    if (a.status === 'failed' || a.status === 'cancelled') return 'failed';
    if (a.status === 'queued') return 'queued';
    if (a.awaitingApproval) return 'review';
    switch (a.phase) {
      case 'Implementation':
        return 'implementing';
      case 'Testing':
        return 'testing';
      case 'Conclusion':
        return 'review';
      default:
        return 'planning';
    }
  }

  // Group the live agent map into ordered columns.
  $: grouped = (() => {
    const cols: Record<ColKey, AgentState[]> = {
      queued: [],
      planning: [],
      implementing: [],
      testing: [],
      review: [],
      done: [],
      failed: []
    };
    for (const a of $agents.values()) cols[columnFor(a)].push(a);
    for (const k of Object.keys(cols) as ColKey[]) {
      cols[k].sort((x, y) => y.startedAt - x.startedAt);
    }
    return cols;
  })();

  function focus(a: AgentState) {
    activeAgentId.set(a.id);
    goto('/');
  }

  function fmtElapsed(ms: number): string {
    const s = Math.floor(ms / 1000);
    const m = Math.floor(s / 60);
    return m > 0 ? `${m}m ${s % 60}s` : `${s}s`;
  }

  function scoreColor(score: number): string {
    if (score >= 0.8) return 'var(--konjo-jade)';
    if (score >= 0.5) return 'var(--konjo-sun)';
    return 'var(--konjo-rose)';
  }
</script>

<div class="h-full overflow-x-auto overflow-y-hidden px-4 py-4">
  <div class="flex gap-3 h-full min-w-max">
    {#each COLUMNS as col (col.key)}
      {@const cards = grouped[col.key]}
      <section class="fleet-col flex flex-col w-72 flex-shrink-0 h-full">
        <!-- Column header -->
        <div class="col-head flex items-center justify-between px-3 py-2 mb-2 rounded-lg" style:--col={col.color}>
          <div class="flex items-center gap-2">
            <span class="w-1.5 h-1.5 rounded-full" style:background={col.color}></span>
            <span class="font-mono text-[11px] uppercase tracking-widest" style:color={col.color}>
              {col.label}
            </span>
          </div>
          <span class="font-mono text-[11px] tabular-nums opacity-50">{cards.length}</span>
        </div>

        <!-- Cards -->
        <div class="flex-1 overflow-y-auto pr-1 space-y-2 min-h-0">
          {#each cards as a (a.id)}
            {@const phase = PHASE_COLORS[a.phase] ?? '#00d4ff'}
            <button
              type="button"
              on:click={() => focus(a)}
              animate:flip={{ duration: 420 }}
              class="card lift press w-full text-left rounded-lg p-3 bg-konjo-deep/60 backdrop-blur-sm border border-white/10"
              style:--phase={phase}
              class:card-live={a.status === 'running'}
            >
              <!-- Goal -->
              <div class="font-mono text-[11px] leading-snug text-konjo-paper line-clamp-2 mb-1.5">
                {a.goal}
              </div>
              <!-- Repo · branch -->
              <div class="flex items-center gap-1.5 font-mono text-[9px] opacity-40 mb-2 truncate">
                <span class="truncate">{a.repo}</span>
                {#if a.branch}<span class="opacity-60">· {a.branch}</span>{/if}
              </div>

              <!-- Metrics row -->
              <div class="flex items-center gap-2 font-mono text-[9px]">
                {#if a.score !== undefined}
                  <span class="px-1.5 py-0.5 rounded tabular-nums" style:color={scoreColor(a.score)} style:background={`color-mix(in srgb, ${scoreColor(a.score)} 14%, transparent)`}>
                    {Math.round(a.score * 100)}%
                  </span>
                {/if}
                <span class="opacity-50 tabular-nums" style:color="var(--konjo-flame)">${a.cost.toFixed(3)}</span>
                <span class="opacity-40 tabular-nums">{fmtElapsed(a.elapsedMs)}</span>
                {#if a.attempt > 1}
                  <span class="opacity-40">·{a.attempt}</span>
                {/if}
                {#if a.verifierPassed !== undefined}
                  <span class="ml-auto" style:color={a.verifierPassed ? 'var(--konjo-jade)' : 'var(--konjo-rose)'}>
                    {a.verifierPassed ? '✓' : '✕'}
                  </span>
                {/if}
              </div>

              <!-- Pressure bar (phase-tinted) -->
              <div class="h-1 mt-2 rounded-full bg-black/40 overflow-hidden">
                <div class="h-full rounded-full transition-all duration-300" style:width={`${a.pressure * 100}%`} style:background={phase}></div>
              </div>
            </button>
          {/each}

          {#if cards.length === 0}
            <div class="text-center font-mono text-[10px] opacity-20 py-6 select-none">—</div>
          {/if}
        </div>
      </section>
    {/each}
  </div>
</div>

<style>
  .col-head {
    background: color-mix(in srgb, var(--col) 6%, transparent);
    border: 1px solid color-mix(in srgb, var(--col) 18%, transparent);
  }
  .card {
    box-shadow: var(--shadow-pane);
    border-left: 2px solid color-mix(in srgb, var(--phase) 55%, transparent);
  }
  .card:hover {
    border-color: rgba(255, 255, 255, 0.18);
    border-left-color: var(--phase);
  }
  /* A live card carries a faint phase rim so the board telegraphs work. */
  .card-live {
    box-shadow:
      var(--shadow-pane),
      0 0 18px -8px color-mix(in srgb, var(--phase) 60%, transparent);
  }
  .line-clamp-2 {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
</style>
