<!--
  StackConnector — the vertical gap between two cards in a pane. Dotted with
  a cyan cadence badge when the card above is scheduled. Hovering the gap
  reveals a dashed "add between" block that inserts a fresh card right here
  via `stack.insert` (the pre-flight gate's `insertCardIntoPane`).

  V&V FINDING (docs/ui/UI-2-VV-report.md §4.1): this used to also render a
  `budget N` badge, styled identically to the (real, WIRED) schedule badge,
  whenever a card's guardrails.budget !== 'auto'. Nothing server-side reads
  that field — see `stores/stack.ts::cardToTaskPayload`'s key-completeness
  test — so the badge read as an enforced limit when nothing enforced it.
  Hidden per Phase 0 of the backend-1 sprint until budget enforcement is
  real; do not re-add it as a no-op decoration.
-->
<script lang="ts">
  import { type StackCard as StackCardT, cronHuman, buildCard, insertCardIntoPane } from '$lib/stores/stack';
  import { ICONS } from './icons';

  /** The card above this gap — its schedule drives the cadence badge. */
  export let card: StackCardT;
  export let paneKey: string;
  /** This card's index in the pane; the new card lands right after it. */
  export let index: number;

  $: sched = card.scheduled;

  function insertHere() {
    insertCardIntoPane(paneKey, index + 1, buildCard('new prompt'));
  }
</script>

<div class="connector" class:sched>
  <span class="cline-full"></span>
  {#if sched}
    <span class="connbadge sched">{@html ICONS.cron}{cronHuman(card.cron)}</span>
  {/if}
  <button type="button" class="cinsert" on:click={insertHere} title="add a prompt here">
    {@html ICONS.plus}
  </button>
</div>

<style>
  .connector {
    position: relative;
    height: 52px;
    margin: 2px 0;
  }
  .connector.sched {
    height: 72px;
  }
  .cline-full {
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    border-left: 2px solid var(--konjo-flame);
    opacity: 0.45;
    transform: translateX(-1px);
  }
  .connector.sched .cline-full {
    border-left: 2px dashed rgba(245, 245, 245, 0.46);
    opacity: 0.55;
  }
  .connbadge {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-family: var(--font-mono, monospace);
    font-size: 9.5px;
    border: 1px solid;
    border-radius: 20px;
    padding: 4px 12px;
    background: var(--konjo-black, #0b0e10);
    white-space: nowrap;
    z-index: 2;
  }
  .connbadge :global(svg) {
    width: 11px;
    height: 11px;
  }
  .connbadge.sched {
    color: var(--konjo-ice);
    border-color: rgba(0, 212, 255, 0.45);
  }
  .cinsert {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
    height: 30px;
    border: 1.5px dashed rgba(0, 212, 255, 0.5);
    border-radius: 8px;
    background: rgba(0, 212, 255, 0.05);
    color: var(--konjo-ice);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity 0.13s;
    cursor: pointer;
    z-index: 3;
  }
  .cinsert :global(svg) {
    width: 16px;
    height: 16px;
  }
  .connector:hover .cinsert,
  .cinsert:focus-visible {
    opacity: 1;
  }
  @media (prefers-reduced-motion: reduce) {
    .cinsert {
      transition: none;
    }
  }
</style>
