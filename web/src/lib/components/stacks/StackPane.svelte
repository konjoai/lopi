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
  import { type StackPaneState, addToPane, buildCard, perLoopScheduleGoverned, paneIsBare } from '$lib/stores/stack';
  import type { Option } from '$lib/stores/controls';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import StackCard from './StackCard.svelte';
  import StackConnector from './StackConnector.svelte';
  import StackOutput from './StackOutput.svelte';
  import StackControlDock from './StackControlDock.svelte';
  import { runs, runBarePane } from '$lib/stores/stackRun';
  import { agents } from '$lib/stores/agents';
  import { ICONS } from './icons';

  export let pane: StackPaneState;
  export let index: number;
  export let repoOptions: Option[] = [];
  /** Close this pane. Null keeps the header X inert (e.g. a lone pane). */
  export let onClose: (() => void) | null = null;

  $: paneDefaults = pane.config.defaults;
  // F2 — a bare pane (≤1 card) has no dock, so it carries its own run button.
  // Its live phase drives the button label the same way the dock's does.
  $: barePhase = $runs.get(pane.key)?.phase;
  $: bareRunning = barePhase === 'running';
  $: scheduleGoverned = perLoopScheduleGoverned(pane.config);
  // Unify-2 §3: a 0- or 1-card pane is a *bare* box (composer + card + orb) that
  // reads like the old Forge pane; the purple stack control dock and inter-card
  // connectors appear only once a second loop makes it a real stack.
  $: bare = paneIsBare(pane);

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
    <button
      class="hx"
      class:live={onClose}
      type="button"
      title={onClose ? 'close pane' : 'close'}
      disabled={!onClose}
      on:click={() => onClose?.()}
    >{@html ICONS.x}</button>
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

  {#if !bare}
    <StackControlDock {pane} {index} {repoOptions} />
  {:else if pane.cards.length >= 1}
    <!-- F2 — bare pane's own run affordance (no dock at ≤1 card). Runs the
         single staged card via the loop-semantics-free bare payload. -->
    <div class="barerun">
      <button
        class="barerunbtn"
        type="button"
        title="run this prompt"
        disabled={bareRunning}
        on:click={() => runBarePane(pane.key, paneDefaults, agents)}
      >
        {@html ICONS.play}
        {bareRunning ? 'running…' : 'run'}
      </button>
    </div>
  {/if}
</div>

<style>
  .pane {
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 14px;
    background: var(--konjo-panel, #0a0d0f);
    position: relative;
    /* Fills its auto-tiling TileGrid cell; the card stack scrolls internally so
       a tall stack never blows out the grid. */
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .panehead,
  .panecomposer {
    flex: 0 0 auto;
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
  .panehead .hx.live {
    cursor: pointer;
  }
  .panehead .hx.live:hover {
    color: var(--konjo-rose, #ff0066);
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
    flex: 1 1 auto;
    overflow-y: auto;
    min-height: 0;
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

  /* F2 — bare-pane run button (the ≤1-card pane's stand-in for the dock's
     run-stack action). Same warm accent as the dock's `.runmain`. */
  .barerun {
    padding-top: 13px;
  }
  .barerunbtn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    width: 100%;
    background: linear-gradient(180deg, #ffb648, #ff9500);
    color: #231000;
    border: none;
    border-radius: 8px;
    padding: 12px;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
  }
  .barerunbtn :global(svg) {
    width: 15px;
    height: 15px;
  }
  .barerunbtn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
