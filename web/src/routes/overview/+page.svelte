<!--
  /overview — the Loop Stacks board: every stack across the account, grouped
  into four lifecycle columns (queued/running/testing/done), kanban-style.
  Replaces the old per-agent rollup table — a "stack" (a Loop Stacks pane,
  see `stores/stack.ts`) is the unit users actually think in, and one stack
  can chain several loops, each of which used to show as its own disconnected
  row here.

  Web translation of the iOS "Overview" handoff
  (`design_handoff_ios_loop_stacks/README.md`): its column-header treatment
  ("1a" — dot + uppercase label + right-aligned count, a colored underline)
  paired with its denser card body ("1b" — left-accent bar, name + live dot,
  single prompt line, compact loop-progress dots, one right-aligned meta
  value) rather than iOS's swipe-to-manage single scrolling list, since the
  web app already has a full per-stack management surface on `/stacks`.

  Honest truth: every card is a real client-side pane from `panes`, resolved
  against the live `agents` map — no fabricated stacks. A stack with no
  cards yet (still just an open composer) doesn't appear; add its first
  prompt on Loop Stacks to put it on the board.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { panes } from '$lib/stores/stack';
  import { agents, connectionState } from '$lib/stores/agents';
  import { focusStack } from '$lib/stores/focusStack';
  import {
    buildStackOverviewCards,
    groupByLifecycle,
    totalCost,
    LIFECYCLE_ORDER,
    LIFECYCLE_LABEL,
    LIFECYCLE_COLOR,
    type StackOverviewCard as StackOverviewCardT
  } from '$lib/stores/stackOverview';
  import StackOverviewCard from '$lib/components/stacks/StackOverviewCard.svelte';

  $: cards = buildStackOverviewCards($panes, $agents);
  $: groups = groupByLifecycle(cards);
  $: liveCount = groups.running.length + groups.testing.length;
  $: spent = totalCost($agents);
  $: offline = $connectionState === 'offline';

  function open(card: StackOverviewCardT) {
    focusStack(card.key);
    goto('/stacks');
  }
</script>

<div class="max-w-[1400px] mx-auto px-4 py-8 space-y-5">
  <div class="head">
    <div class="titlerow">
      <h1 class="font-display text-2xl">Stack Loops</h1>
      <span class="live" class:offline>
        <span class="livedot"></span>{offline ? 'OFFLINE' : 'LIVE'}
      </span>
    </div>
    <p class="subtitle">
      <span class="stat"><b>{cards.length}</b> stacks</span>
      <span class="stat"><b class="ice">{liveCount}</b> live</span>
      <span class="stat"><b>${spent.toFixed(4)}</b> spent</span>
    </p>
  </div>

  {#if cards.length === 0}
    <div class="banner">no stacks yet — add a prompt on Loop Stacks to put one on the board</div>
  {:else}
    <div class="board">
      {#each LIFECYCLE_ORDER as lifecycle (lifecycle)}
        <div class="col">
          <div class="colhead" style:border-color={LIFECYCLE_COLOR[lifecycle]}>
            <span class="cdot" style:background={LIFECYCLE_COLOR[lifecycle]}></span>
            <span class="clabel">{LIFECYCLE_LABEL[lifecycle]}</span>
            <span class="ccount" style:color={LIFECYCLE_COLOR[lifecycle]}>{groups[lifecycle].length}</span>
          </div>
          <div class="cbody">
            {#each groups[lifecycle] as card (card.key)}
              <StackOverviewCard {card} on:click={() => open(card)} />
            {:else}
              <div class="empty">none</div>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .head {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .titlerow {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .live {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    letter-spacing: 0.1em;
    color: var(--konjo-jade, #00ff9d);
  }
  .live.offline {
    color: rgba(245, 245, 245, 0.35);
  }
  .livedot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }
  .live:not(.offline) .livedot {
    animation: livepulse 1.8s ease-in-out infinite;
  }
  @keyframes livepulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }
  .subtitle {
    display: flex;
    gap: 18px;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    color: rgba(245, 245, 245, 0.5);
    margin: 0;
  }
  .subtitle b {
    color: var(--konjo-paper, #f5f5f5);
    font-weight: 700;
  }
  .subtitle b.ice {
    color: var(--konjo-ice, #00d4ff);
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
  .board {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 16px;
    align-items: start;
  }
  .col {
    min-width: 0;
  }
  .colhead {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 4px 10px;
    border-bottom: 2px solid;
    position: sticky;
    top: 0;
    background: var(--konjo-black, #0a0a0a);
    z-index: 1;
  }
  .cdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .clabel {
    color: var(--konjo-paper, #f5f5f5);
    font-weight: 600;
    font-size: 12px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .ccount {
    margin-left: auto;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
  }
  .cbody {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 12px;
    max-height: calc(100vh - 260px);
    overflow-y: auto;
  }
  .empty {
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    color: rgba(245, 245, 245, 0.25);
    padding: 10px 4px;
    text-align: center;
    border: 1px dashed rgba(255, 255, 255, 0.08);
    border-radius: 8px;
  }
</style>
