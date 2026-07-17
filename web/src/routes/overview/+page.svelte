<!--
  /overview — the app-wide rollup. One dense, read-only row per live agent
  across every pane and card, sortable by lifecycle with orb-colored status.
  This is the single surface that replaces Fleet + Dashboard + Pulse's
  *information* (per-agent metrics, whole-fleet glance, live status). Clicking a
  row focuses that agent on Loop Stacks. Constellation's 3D orbital view is
  cut, not folded in here. The old Tasks page folds in too: its dead-letter view
  is now the `dead-letter` status filter, not a separate route.

  Honest truth: rows come only from the live `agents` store — no fabricated
  agents ever fill it. Offline says offline; connected-but-idle says idle.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { flip } from 'svelte/animate';
  import { agents, permissionWaiting, activeAgentId, connectionState } from '$lib/stores/agents';
  import {
    overviewRows,
    filterRows,
    filterCounts,
    formatElapsed,
    type StatusFilter,
    type OverviewRow
  } from '$lib/stores/overview';

  let filter: StatusFilter = 'all';

  $: rows = overviewRows($agents, $permissionWaiting);
  $: counts = filterCounts(rows);
  $: shown = filterRows(rows, filter);
  $: offline = $connectionState === 'offline' || $connectionState === 'connecting';
  $: idle = $connectionState === 'connected' && rows.length === 0;

  const FILTERS: { key: StatusFilter; label: string }[] = [
    { key: 'all', label: 'all' },
    { key: 'running', label: 'running' },
    { key: 'queued', label: 'queued' },
    { key: 'done', label: 'done' },
    { key: 'dead-letter', label: 'dead-letter' }
  ];

  function focus(row: OverviewRow) {
    activeAgentId.set(row.id);
    goto('/stacks');
  }

  function scoreColor(score: number): string {
    if (score >= 0.8) return 'var(--konjo-jade)';
    if (score >= 0.5) return 'var(--konjo-sun)';
    return 'var(--konjo-rose)';
  }
</script>

<div class="max-w-[1400px] mx-auto px-4 py-8 space-y-6">
  <div>
    <h1 class="font-display text-2xl">Overview</h1>
    <p class="font-mono text-[11px] uppercase tracking-widest opacity-50 mt-1">
      every active pane &amp; card · goal · phase · elapsed · cost · score · click to open
    </p>
  </div>

  <!-- Lifecycle filter chips (dead-letter folds in the old Tasks view) -->
  <div class="flex flex-wrap gap-2">
    {#each FILTERS as f (f.key)}
      <button
        type="button"
        class="chip"
        class:active={filter === f.key}
        on:click={() => (filter = f.key)}
      >
        {f.label}
        <span class="cnt">{counts[f.key]}</span>
      </button>
    {/each}
  </div>

  {#if offline}
    <div class="banner err">
      backend offline — {$connectionState === 'connecting' ? 'connecting to lopi sail…' : 'start `lopi sail` to see live agents'}
    </div>
  {:else if idle}
    <div class="banner">no live sessions — launch a run to populate the overview</div>
  {:else if shown.length === 0}
    <div class="banner">no {filter} agents</div>
  {:else}
    <div class="tablewrap">
      <table>
        <thead>
          <tr>
            <th class="c-dot"></th>
            <th class="c-goal">goal</th>
            <th class="c-repo">repo · branch</th>
            <th class="c-phase">phase</th>
            <th class="c-num">elapsed</th>
            <th class="c-num">cost</th>
            <th class="c-num">score</th>
          </tr>
        </thead>
        <tbody>
          {#each shown as row (row.id)}
            <tr
              class="row {row.special}"
              animate:flip={{ duration: 260 }}
              style:--orb={row.orbColor}
              on:click={() => focus(row)}
              tabindex="0"
              role="button"
              on:keydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), focus(row))}
              title="Open on Loop Stacks"
            >
              <td class="c-dot"><span class="dot" class:awaiting={row.awaiting}></span></td>
              <td class="c-goal"><span class="goal">{row.goal}</span></td>
              <td class="c-repo">
                <span class="repo">{row.repo || '—'}</span>
                {#if row.branch}<span class="branch">{row.branch}</span>{/if}
              </td>
              <td class="c-phase">
                <span class="phase" style:color={row.orbColor}>{row.phase}</span>
                {#if row.status !== 'running' && row.status !== 'queued'}
                  <span class="term">{row.status}</span>
                {/if}
              </td>
              <td class="c-num tabular">{formatElapsed(row.elapsedMs)}</td>
              <td class="c-num tabular cost">${row.cost.toFixed(4)}</td>
              <td class="c-num tabular">
                {#if row.score !== undefined}
                  <span style:color={scoreColor(row.score)}>{Math.round(row.score * 100)}</span>
                {:else}
                  <span class="opacity-30">—</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .chip {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    padding: 5px 12px;
    border-radius: 7px;
    border: 1px solid rgba(255, 255, 255, 0.11);
    background: transparent;
    color: rgba(245, 245, 245, 0.5);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    transition: 0.12s;
  }
  .chip:hover {
    color: var(--konjo-paper, #f5f5f5);
    border-color: rgba(245, 245, 245, 0.4);
  }
  .chip.active {
    color: var(--konjo-ice);
    border-color: rgba(0, 212, 255, 0.5);
    background: rgba(0, 212, 255, 0.08);
  }
  .chip .cnt {
    font-size: 10px;
    font-weight: 700;
    opacity: 0.7;
  }
  .banner {
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    padding: 24px;
    text-align: center;
    border: 1px dashed rgba(255, 255, 255, 0.14);
    border-radius: 10px;
    color: rgba(245, 245, 245, 0.5);
  }
  .banner.err {
    border-color: rgba(255, 0, 102, 0.35);
    color: var(--konjo-rose, #ff0066);
  }
  .tablewrap {
    overflow-x: auto;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
  }
  thead th {
    text-align: left;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: rgba(245, 245, 245, 0.4);
    padding: 10px 12px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    font-weight: 500;
  }
  .c-num {
    text-align: right;
  }
  .c-dot {
    width: 26px;
    text-align: center;
  }
  tbody .row {
    cursor: pointer;
    transition: background 0.12s;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }
  tbody .row:hover,
  tbody .row:focus-visible {
    background: color-mix(in srgb, var(--orb) 9%, transparent);
    outline: none;
  }
  td {
    padding: 9px 12px;
    vertical-align: middle;
  }
  .dot {
    display: inline-block;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--orb);
    box-shadow: 0 0 7px 0 color-mix(in srgb, var(--orb) 70%, transparent);
  }
  /* Motion echoes the orb vocabulary: running phases breathe, terminal
     hardStop is steady, awaiting pulses for attention. */
  .row.none .dot,
  .row.attentionPulse .dot,
  .row.kryptonite .dot,
  .row.stutter .dot,
  .row.reverseSpin .dot {
    animation: dotpulse 1.8s ease-in-out infinite;
  }
  .row.hardStop .dot {
    animation: none;
  }
  .dot.awaiting {
    animation: dotpulse 0.9s ease-in-out infinite !important;
  }
  @keyframes dotpulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.45;
    }
  }
  .goal {
    color: var(--konjo-paper, #f5f5f5);
    display: inline-block;
    max-width: 420px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: bottom;
  }
  .repo {
    color: rgba(245, 245, 245, 0.7);
  }
  .branch {
    color: rgba(245, 245, 245, 0.4);
    margin-left: 8px;
  }
  .phase {
    font-weight: 600;
  }
  .term {
    margin-left: 8px;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    opacity: 0.5;
  }
  .tabular {
    font-variant-numeric: tabular-nums;
  }
  .cost {
    color: var(--konjo-flame, #ff9500);
  }
  @media (prefers-reduced-motion: reduce) {
    .dot {
      animation: none !important;
    }
  }
</style>
