<script lang="ts">
  /**
   * Sessions sidebar — every task the server knows about, whether or not it is
   * currently mounted as a pane. This is the home for sessions the user closed:
   * closing a pane parks the session here (it does NOT delete it). The trash
   * action is the only path that permanently removes a session.
   */
  import { agents, deleteSession, PHASE_COLORS, type AgentState } from '$lib/stores/agents';
  import { paneSlots, closedSessions, openSession } from '$lib/stores/layout';
  import { filterSessions, groupSessions } from '$lib/stores/session-groups';

  export let collapsed = false;

  let query = '';

  // Filter then group (active / done / failed, newest-first, empties dropped).
  $: visible = filterSessions($agents.values(), query);
  $: groups = groupSessions(visible);
  $: total = $agents.size;
  $: openIds = new Set($paneSlots.filter((s): s is string => s !== null));

  function statusColor(s: AgentState): string {
    if (s.status === 'running') return 'var(--konjo-jade)';
    if (s.status === 'queued') return 'var(--konjo-sun)';
    if (s.status === 'failed') return 'var(--konjo-rose)';
    if (s.status === 'cancelled') return 'rgba(255,255,255,0.3)';
    return 'rgba(0,212,255,0.5)';
  }

  // Drag a session out of the sidebar; the grid's pane-host accepts the drop
  // and mounts it into that specific pane (see AgentGrid).
  function onRowDragStart(e: DragEvent, id: string) {
    e.dataTransfer?.setData('application/x-lopi-session', id);
    e.dataTransfer?.setData('text/plain', id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
</script>

<aside class="sidebar" class:collapsed>
  <div class="head">
    <button
      type="button"
      class="collapse"
      on:click={() => (collapsed = !collapsed)}
      title={collapsed ? 'Expand sessions' : 'Collapse sessions'}
      aria-label="Toggle sessions sidebar"
    >
      {collapsed ? '»' : '«'}
    </button>
    {#if !collapsed}
      <span class="title">sessions</span>
      <span class="count">{total}</span>
    {/if}
  </div>

  {#if !collapsed}
    {#if total > 0}
      <div class="search">
        <span class="search-icon">⌕</span>
        <input
          type="search"
          placeholder="filter goal / repo / branch"
          bind:value={query}
          spellcheck="false"
        />
        {#if query}
          <button type="button" class="clear" on:click={() => (query = '')} aria-label="Clear filter">✕</button>
        {/if}
      </div>
    {/if}

    <div class="list">
      {#if total === 0}
        <div class="empty">no sessions yet</div>
      {:else if groups.length === 0}
        <div class="empty">no matches for “{query}”</div>
      {/if}
      {#each groups as group (group.key)}
        <div class="group-head">
          <span class="group-label">{group.label}</span>
          <span class="group-count">{group.sessions.length}</span>
        </div>
        {#each group.sessions as s (s.id)}
          {@const isOpen = openIds.has(s.id)}
          {@const isClosed = $closedSessions.has(s.id)}
          <div
            class="row"
            class:open={isOpen}
            draggable="true"
            role="group"
            on:dragstart={(e) => onRowDragStart(e, s.id)}
            title="Drag into a pane"
          >
            <button
              type="button"
              class="open-btn"
              on:click={() => openSession(s.id)}
              title={isOpen ? 'In a pane' : 'Open in first free pane (or drag onto one)'}
            >
              <span class="dot" style:background={statusColor(s)}></span>
              <span class="meta">
                <span class="goal">{s.goal}</span>
                <span class="sub">
                  <span style:color={PHASE_COLORS[s.phase]}>{s.phase}</span>
                  {#if isClosed && !isOpen}<span class="parked">· parked</span>{/if}
                </span>
              </span>
            </button>
            <button
              type="button"
              class="trash"
              on:click={() => deleteSession(s.id)}
              title="Delete session permanently"
              aria-label="Delete session permanently"
            >
              🗑
            </button>
          </div>
        {/each}
      {/each}
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    width: 240px;
    height: 100%;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid rgba(255, 255, 255, 0.06);
    background: rgba(5, 5, 6, 0.6);
    backdrop-filter: blur(8px);
    transition: width 0.2s ease;
    overflow: hidden;
  }
  .sidebar.collapsed {
    width: 40px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 10px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    flex-shrink: 0;
  }
  .collapse {
    width: 20px;
    height: 20px;
    border: none;
    background: transparent;
    color: var(--konjo-accent);
    cursor: pointer;
    border-radius: 4px;
    font-size: 12px;
  }
  .collapse:hover {
    background: rgb(var(--konjo-accent-rgb) / 0.1);
  }
  .title {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    opacity: 0.6;
    flex: 1;
  }
  .count {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    opacity: 0.4;
    font-variant-numeric: tabular-nums;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 8px 2px;
    padding: 5px 8px;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.07);
    background: rgba(255, 255, 255, 0.025);
    flex-shrink: 0;
  }
  .search:focus-within {
    border-color: rgb(var(--konjo-accent-rgb) / 0.45);
  }
  .search-icon {
    font-size: 12px;
    opacity: 0.4;
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    outline: none;
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
  }
  .search input::placeholder {
    opacity: 0.3;
  }
  .search input::-webkit-search-cancel-button {
    display: none;
  }
  .clear {
    border: none;
    background: transparent;
    color: rgba(255, 255, 255, 0.35);
    cursor: pointer;
    font-size: 9px;
    padding: 0 2px;
  }
  .clear:hover {
    color: var(--konjo-paper, #f5f5f5);
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 6px 3px;
    position: sticky;
    top: 0;
    background: rgba(5, 5, 6, 0.85);
    backdrop-filter: blur(6px);
    z-index: 1;
  }
  .group-label {
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    letter-spacing: 0.2em;
    text-transform: uppercase;
    opacity: 0.45;
  }
  .group-count {
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    opacity: 0.3;
    font-variant-numeric: tabular-nums;
  }
  .group-head::after {
    content: '';
    flex: 1;
    height: 1px;
    background: rgba(255, 255, 255, 0.05);
  }
  .empty {
    opacity: 0.3;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    text-align: center;
    padding: 20px 0;
  }
  .row {
    display: flex;
    align-items: stretch;
    gap: 2px;
    border-radius: 8px;
    border: 1px solid transparent;
    transition: background 0.12s;
    cursor: grab;
  }
  .row:active {
    cursor: grabbing;
  }
  .row:hover {
    background: rgba(255, 255, 255, 0.03);
  }
  .row.open {
    border-color: rgb(var(--konjo-accent-rgb) / 0.25);
    background: rgb(var(--konjo-accent-rgb) / 0.05);
  }
  .open-btn {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    border: none;
    background: transparent;
    color: var(--konjo-paper, #f5f5f5);
    cursor: pointer;
    text-align: left;
    min-width: 0;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .meta {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .goal {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    opacity: 0.7;
  }
  .parked {
    opacity: 0.5;
  }
  .trash {
    width: 26px;
    border: none;
    background: transparent;
    color: rgba(255, 255, 255, 0.25);
    cursor: pointer;
    font-size: 11px;
    border-radius: 6px;
    flex-shrink: 0;
  }
  .trash:hover {
    color: var(--konjo-rose);
    background: rgb(var(--konjo-accent-rgb) / 0.05);
  }
</style>
