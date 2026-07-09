<!--
  StackPane — one pane's chrome: header (logo + title + status dot + inert
  X), top composer (new prompts prepend), the card stack itself (flowing
  down to the currently-executing loop at the bottom), and the purple stack
  control area (`StackControlDock.svelte` — loop/schedule/guardrails/evals/
  config for the whole chain, plus the real run-stack action; see Stack-1).
  Two (or more, since Stack-1 added `duplicateStack`) of these render
  side-by-side in `/stacks` and are fully independent — no cross-pane card
  drag this slice (whole-*stack* reordering is in scope, via the dock).
-->
<script lang="ts">
  import { type StackPaneState, addToPane, buildCard, perLoopScheduleGoverned } from '$lib/stores/stack';
  import type { Option } from '$lib/stores/controls';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import StackCard from './StackCard.svelte';
  import StackConnector from './StackConnector.svelte';
  import StackOutput from './StackOutput.svelte';
  import StackControlDock from './StackControlDock.svelte';
  import { ICONS } from './icons';

  export let pane: StackPaneState;
  export let index: number;
  export let repoOptions: Option[] = [];

  $: paneDefaults = pane.config.defaults;
  $: scheduleGoverned = perLoopScheduleGoverned(pane.config);

  let composerValue = '';

  function submit() {
    const text = composerValue.trim();
    if (!text) return;
    addToPane(pane.key, buildCard(text));
    composerValue = '';
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      submit();
    }
  }
</script>

<div class="pane">
  <div class="panehead">
    <span class="logo">{@html ICONS.grid}</span>
    <span class="ptitle">{pane.title}</span>
    <span class="hsp"></span>
    <span class="hdot"></span>
    <button class="hx" type="button" title="close (not wired this slice)">{@html ICONS.x}</button>
  </div>

  <div class="panecomposer">
    <span class="ar">&gt;</span>
    <input
      bind:value={composerValue}
      on:keydown={onKeydown}
      placeholder="add a prompt or goal…"
      spellcheck="false"
    />
    <button class="addbtn2" type="button" on:click={submit} disabled={!composerValue.trim()} title="add to stack">
      {@html ICONS.plus}
    </button>
  </div>

  <div class="panestack">
    {#if pane.cards.length === 0}
      <EmptyState title="no loops yet" detail="add one above" />
    {:else}
      {#each pane.cards as card, i (card.id)}
        {#if card.status === 'running' && card.taskId}
          <div class="loopwrap hasout">
            <StackCard {card} paneKey={pane.key} index={i} {paneDefaults} {repoOptions} {scheduleGoverned} />
            <StackOutput taskId={card.taskId} />
          </div>
        {:else}
          <div class="loopwrap">
            <StackCard {card} paneKey={pane.key} index={i} {paneDefaults} {repoOptions} {scheduleGoverned} />
          </div>
        {/if}
        {#if i < pane.cards.length - 1}
          <StackConnector {card} paneKey={pane.key} index={i} {scheduleGoverned} />
        {/if}
      {/each}
    {/if}
  </div>

  <StackControlDock {pane} {index} {repoOptions} />
</div>

<style>
  .pane {
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 14px;
    background: var(--konjo-panel, #0a0d0f);
    position: relative;
    flex: 1 1 480px;
    max-width: 720px;
    min-width: 320px;
  }
  .panehead {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 14px 18px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }
  .panehead .logo {
    color: var(--konjo-flame);
    display: inline-flex;
  }
  .panehead .logo :global(svg) {
    width: 19px;
    height: 19px;
  }
  .panehead .ptitle {
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--konjo-paper, #f5f5f5);
  }
  .panehead .hsp {
    flex: 1;
  }
  .panehead .hdot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: rgba(245, 245, 245, 0.28);
  }
  .panehead .hx {
    background: none;
    border: none;
    color: rgba(245, 245, 245, 0.28);
    cursor: not-allowed;
    display: inline-flex;
  }
  .panehead .hx :global(svg) {
    width: 16px;
    height: 16px;
  }
  .panecomposer {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 13px 18px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }
  .panecomposer .ar {
    color: var(--konjo-flame);
    font-family: var(--font-mono, monospace);
    font-size: 15px;
  }
  .panecomposer input {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 14px;
    outline: none;
    min-width: 0;
  }
  .panecomposer input::placeholder {
    color: rgba(245, 245, 245, 0.28);
  }
  .addbtn2 {
    width: 34px;
    height: 34px;
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 7px;
    background: transparent;
    color: rgba(245, 245, 245, 0.28);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
  }
  .addbtn2:hover:not(:disabled) {
    color: var(--konjo-flame);
    border-color: rgba(255, 149, 0, 0.4);
  }
  .addbtn2:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .addbtn2 :global(svg) {
    width: 16px;
    height: 16px;
  }
  .panestack {
    padding: 24px 18px 8px;
  }
  .loopwrap.hasout :global(.pc) {
    border-bottom-left-radius: 0;
    border-bottom-right-radius: 0;
  }
  .loopwrap.hasout :global(.pc.running) {
    border-bottom-color: rgba(255, 255, 255, 0.1);
    animation-name: cardflash;
  }
  .loopwrap.hasout :global(.output) {
    animation: outflash 5s ease-in-out infinite;
  }
  @keyframes outflash {
    0%,
    100% {
      border-color: rgba(255, 150, 70, 0.5);
      box-shadow: 0 0 0 0 rgba(255, 149, 0, 0);
    }
    50% {
      border-color: rgba(255, 195, 110, 0.98);
      box-shadow: 0 0 20px rgba(255, 149, 0, 0.2);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .loopwrap.hasout :global(.pc.running),
    .loopwrap.hasout :global(.output) {
      animation: none;
    }
  }
</style>
