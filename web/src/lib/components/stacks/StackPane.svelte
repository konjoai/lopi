<!--
  StackPane — one pane's chrome: header (logo + title + status dot + inert
  X), top composer (new prompts prepend), the card stack itself (flowing
  down to the currently-executing loop at the bottom), and the amber
  run-stack footer. Two of these render side-by-side in `/stacks` and are
  fully independent — no cross-pane drag this slice.
-->
<script lang="ts">
  import { type StackPaneState, addToPane, buildCard, type DryRunResult } from '$lib/stores/stack';
  import { runs, runStack, pauseStack, resumeStack, type RunPhase } from '$lib/stores/stackRun';
  import { agents } from '$lib/stores/agents';
  import type { StackDefaults } from '$lib/stores/stackDefaults';
  import type { Option } from '$lib/stores/controls';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import StackCard from './StackCard.svelte';
  import StackConnector from './StackConnector.svelte';
  import StackOutput from './StackOutput.svelte';
  import RunMenu from './RunMenu.svelte';
  import { ICONS } from './icons';

  export let pane: StackPaneState;
  export let paneDefaults: StackDefaults;
  export let repoOptions: Option[] = [];

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

  let runMenuOpen = false;
  let dryRunResult: DryRunResult | null = null;

  $: phase = $runs.get(pane.key)?.phase as RunPhase | undefined;
  $: runError = $runs.get(pane.key)?.error;
  $: runLabel =
    phase === 'running'
      ? 'pause'
      : phase === 'paused'
        ? 'resume'
        : phase === 'draining'
          ? 'draining…'
          : 'run stack';
  $: runIcon = phase === 'running' ? ICONS.pause : ICONS.play;

  // The split button's main half doubles as the pause/resume toggle once a
  // run is active — the dropdown (RunMenu) carries the full intent set
  // (Run now/Run once/Schedule stack/Dry run) plus Drain while running.
  function runMain() {
    if (phase === 'running') {
      pauseStack(pane.key);
    } else if (phase === 'paused') {
      resumeStack(pane.key, paneDefaults, agents);
    } else {
      dryRunResult = null;
      runStack(pane.key, 'run', paneDefaults, agents);
    }
    runMenuOpen = false;
  }

  /** Dismiss a finished run's error banner — clears its `runs` entry
   *  entirely, so the split button falls back to its idle "run stack"
   *  state rather than getting stuck showing a stale phase. */
  function dismissRunError() {
    runs.update((m) => {
      const next = new Map(m);
      next.delete(pane.key);
      return next;
    });
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
            <StackCard {card} paneKey={pane.key} index={i} {paneDefaults} {repoOptions} />
            <StackOutput taskId={card.taskId} />
          </div>
        {:else}
          <div class="loopwrap">
            <StackCard {card} paneKey={pane.key} index={i} {paneDefaults} {repoOptions} />
          </div>
        {/if}
        {#if i < pane.cards.length - 1}
          <StackConnector {card} paneKey={pane.key} index={i} />
        {/if}
      {/each}
    {/if}
  </div>

  <div class="panefoot">
    {#if runError}
      <div class="runbanner err">
        <span>{runError}</span>
        <button type="button" on:click={dismissRunError}>{@html ICONS.x}</button>
      </div>
    {:else if dryRunResult}
      <div class="runbanner" class:err={!dryRunResult.valid}>
        <span>
          {#if dryRunResult.valid}
            dry run: {dryRunResult.plan.length} loop{dryRunResult.plan.length === 1 ? '' : 's'} would run, in order
          {:else}
            dry run found {dryRunResult.issues.length} issue{dryRunResult.issues.length === 1 ? '' : 's'}: {dryRunResult
              .issues[0].message}
          {/if}
        </span>
        <button type="button" on:click={() => (dryRunResult = null)}>{@html ICONS.x}</button>
      </div>
    {/if}
    <div class="runsplit">
      <button
        class="runmain"
        type="button"
        on:click={runMain}
        disabled={phase === 'draining'}
        title="run this stack"
      >
        {@html runIcon} {runLabel}
      </button>
      <button
        class="runchev"
        type="button"
        on:click={() => (runMenuOpen = !runMenuOpen)}
        aria-expanded={runMenuOpen}
      >
        {@html ICONS.chevup}
      </button>
    </div>
    {#if runMenuOpen}
      <RunMenu
        paneKey={pane.key}
        defaults={paneDefaults}
        {phase}
        onDryRun={(r) => (dryRunResult = r)}
        onClose={() => (runMenuOpen = false)}
      />
    {/if}
  </div>
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
  .panefoot {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 14px 18px 20px;
    position: relative;
  }
  .runbanner {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border-radius: 8px;
    background: rgba(0, 212, 255, 0.06);
    border: 1px solid rgba(0, 212, 255, 0.25);
    color: rgba(245, 245, 245, 0.72);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
  }
  .runbanner span {
    flex: 1;
    min-width: 0;
  }
  .runbanner button {
    flex: 0 0 auto;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    display: inline-flex;
  }
  .runbanner button :global(svg) {
    width: 12px;
    height: 12px;
  }
  .runbanner.err {
    background: rgba(255, 90, 90, 0.08);
    border-color: rgba(255, 90, 90, 0.35);
    color: rgba(255, 170, 170, 0.9);
  }
  .runmain:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .runsplit {
    display: inline-flex;
    border-radius: 9px;
    overflow: hidden;
    box-shadow: 0 5px 18px rgba(255, 149, 0, 0.28);
  }
  .runmain {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: linear-gradient(180deg, #ffb648, #ff9500);
    color: #231000;
    border: none;
    padding: 12px 26px;
    font-family: var(--font-mono, monospace);
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
  }
  .runmain :global(svg) {
    width: 15px;
    height: 15px;
  }
  .runchev {
    background: linear-gradient(180deg, #ffa733, #f08600);
    border: none;
    border-left: 1px solid rgba(0, 0, 0, 0.28);
    color: #231000;
    padding: 0 13px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
  }
  .runchev :global(svg) {
    width: 14px;
    height: 14px;
  }
  @media (prefers-reduced-motion: reduce) {
    .loopwrap.hasout :global(.pc.running),
    .loopwrap.hasout :global(.output) {
      animation: none;
    }
  }
</style>
