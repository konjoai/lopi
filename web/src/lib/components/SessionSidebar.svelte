<script lang="ts">
  /**
   * Sessions sidebar — every task the server knows about, whether or not it is
   * currently mounted as a pane. This is the home for sessions the user closed:
   * closing a pane parks the session here (it does NOT delete it). The trash
   * action is the only path that permanently removes a session.
   */
  import { agents, deleteSession, PHASE_COLORS, type AgentState } from '$lib/stores/agents';
  import { paneSlots, closedSessions, openSession } from '$lib/stores/layout';

  export let collapsed = false;

  $: sessions = [...$agents.values()].sort((a, b) => b.startedAt - a.startedAt);
  $: openIds = new Set($paneSlots.filter((s): s is string => s !== null));

  function statusColor(s: AgentState): string {
    if (s.status === 'running') return 'var(--konjo-jade)';
    if (s.status === 'queued') return 'var(--konjo-sun)';
    if (s.status === 'failed') return 'var(--konjo-rose)';
    if (s.status === 'cancelled') return 'rgba(255,255,255,0.3)';
    return 'rgba(0,212,255,0.5)';
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
      <span class="count">{sessions.length}</span>
    {/if}
  </div>

  {#if !collapsed}
    <div class="list">
      {#if sessions.length === 0}
        <div class="empty">no sessions yet</div>
      {/if}
      {#each sessions as s (s.id)}
        {@const isOpen = openIds.has(s.id)}
        {@const isClosed = $closedSessions.has(s.id)}
        <div class="row" class:open={isOpen}>
          <button
            type="button"
            class="open-btn"
            on:click={() => openSession(s.id)}
            title={isOpen ? 'In a pane' : 'Open in a pane'}
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
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 3px;
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
