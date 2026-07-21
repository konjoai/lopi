<!--
  StackOverviewCard — the `/overview` board's per-stack card. Dense
  rows-in-columns layout (design handoff's "1b" direction): left-accent bar,
  name + live dot, single-line prompt, mini loop-progress dots, and a single
  right-aligned meta value (repo while queued, elapsed+cost while live,
  cost/failed once done).
-->
<script lang="ts">
  import type { StackOverviewCard } from '$lib/stores/stackOverview';

  export let card: StackOverviewCard;

  $: isLive = card.lifecycle === 'running' || card.lifecycle === 'testing';
</script>

<button
  type="button"
  class="scard"
  style:--accent={card.accentColor}
  on:click
>
  <div class="body">
    <div class="row1">
      <span class="name">{card.title}</span>
      {#if isLive}
        <span class="dot" aria-hidden="true"></span>
      {/if}
    </div>
    <div class="goal">{card.goal}</div>
    <div class="loops" aria-hidden="true">
      {#each card.loops as loop (loop.id)}
        <span class="seg" class:pulsing={loop.pulsing} style:background={loop.color}></span>
      {/each}
    </div>
  </div>
  <div class="meta" style:color={card.lifecycle === 'queued' ? 'rgba(245,245,245,0.35)' : card.metaRightColor}>
    {card.lifecycle === 'queued' ? card.repo : card.metaRight}
  </div>
</button>

<style>
  .scard {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 12px 10px;
    border: none;
    border-left: 3px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 6%, #101013);
    border-radius: 0 8px 8px 0;
    text-align: left;
    cursor: pointer;
    font: inherit;
    color: inherit;
    transition: background 0.12s;
  }
  .scard:hover,
  .scard:focus-visible {
    background: color-mix(in srgb, var(--accent) 12%, #101013);
    outline: none;
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .row1 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
  }
  .name {
    color: var(--konjo-paper, #f5f5f5);
    font-weight: 600;
    font-size: 12.5px;
  }
  .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent);
    animation: cardpulse 1.8s ease-in-out infinite;
  }
  .goal {
    color: rgba(245, 245, 245, 0.65);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .loops {
    display: flex;
    gap: 4px;
    margin-top: 7px;
  }
  .seg {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
  .seg.pulsing {
    animation: cardpulse 1.8s ease-in-out infinite;
  }
  .meta {
    flex: 0 0 auto;
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    white-space: nowrap;
    padding-top: 1px;
  }
  @keyframes cardpulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .dot,
    .seg.pulsing {
      animation: none;
    }
  }
</style>
