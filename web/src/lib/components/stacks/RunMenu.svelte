<!--
  RunMenu — the run-stack chevron's dropdown. Genuinely wired to
  `stores/stackRun.ts`: Run now / Run once / Schedule stack / Dry run when
  no run is active for this pane, or Pause/Resume + Drain once one is.
  Closes on outside-click or Escape.
-->
<script lang="ts">
  import { get } from 'svelte/store';
  import {
    runStack,
    pauseStack,
    resumeStack,
    drainStack,
    scheduleStack,
    type RunPhase
  } from '$lib/stores/stackRun';
  import { agents } from '$lib/stores/agents';
  import { panes, executionOrder, dryRunStack, buildCronString, type DryRunResult, type PaneDefaults } from '$lib/stores/stack';
  import { ICONS } from './icons';

  export let paneKey: string;
  export let defaults: PaneDefaults;
  export let phase: RunPhase | undefined;
  export let onClose: () => void;
  /** Bubbles a "Dry run" result up so the pane can show it — dry-running
   *  never executes anything, so there's nothing else to react to. */
  export let onDryRun: (result: DryRunResult) => void = () => {};

  interface MenuItem {
    icon: string;
    name: string;
    sub: string;
    action: () => void;
  }

  function paneCards() {
    return get(panes).find((p) => p.key === paneKey)?.cards ?? [];
  }

  const idleItems: MenuItem[] = [
    {
      icon: ICONS.play,
      name: 'Run now',
      sub: 'start now',
      action: () => runStack(paneKey, 'run', defaults, agents)
    },
    {
      icon: ICONS.check,
      name: 'Run once',
      sub: 'one pass each',
      action: () => runStack(paneKey, 'run-once', defaults, agents)
    },
    {
      icon: ICONS.cron,
      name: 'Schedule stack',
      sub: 'one cron, bottom card',
      action: () => {
        const cards = executionOrder(paneCards());
        if (cards.length === 0) return;
        const cronExpr = buildCronString(cards[0].cron);
        void scheduleStack(paneKey, cronExpr, defaults);
      }
    },
    {
      icon: ICONS.flask,
      name: 'Dry run',
      sub: 'validate only',
      action: () => onDryRun(dryRunStack(paneCards(), defaults))
    }
  ];

  /** Once a run is active, the menu swaps to control signals instead of
   *  launch intents — Dry run stays available since it never touches
   *  execution either way. */
  $: activeItems =
    phase === 'running'
      ? [
          { icon: ICONS.pause, name: 'Pause', sub: 'halt after this card', action: () => pauseStack(paneKey) },
          { icon: ICONS.x, name: 'Drain', sub: 'finish then stop', action: () => drainStack(paneKey) }
        ]
      : phase === 'paused'
        ? [
            {
              icon: ICONS.play,
              name: 'Resume',
              sub: 'continue run',
              action: () => resumeStack(paneKey, defaults, agents)
            },
            { icon: ICONS.x, name: 'Drain', sub: 'stop for good', action: () => drainStack(paneKey) }
          ]
        : [];

  let items: MenuItem[] = idleItems;
  $: items = [
    ...activeItems,
    ...(phase === 'running' || phase === 'paused' ? idleItems.filter((it) => it.name === 'Dry run') : idleItems)
  ];

  function pick(item: MenuItem) {
    item.action();
    onClose();
  }

  function onOutside(e: MouseEvent) {
    const el = e.target as HTMLElement;
    if (el.closest('.runmenu') || el.closest('.runchev')) return;
    onClose();
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window on:keydown={onKeydown} />
<svelte:body on:mousedown|capture={onOutside} />

<div class="runmenu">
  {#each items as it (it.name)}
    <button type="button" class="rm" on:click={() => pick(it)}>
      {@html it.icon}<span class="rmn">{it.name}</span><span class="rms">{it.sub}</span>
    </button>
  {/each}
</div>

<style>
  .runmenu {
    position: absolute;
    bottom: 72px;
    left: 50%;
    transform: translateX(-50%);
    width: 320px;
    max-width: calc(100vw - 24px);
    background: var(--konjo-panel, #0a0d0f);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 11px;
    box-shadow: 0 20px 55px rgba(0, 0, 0, 0.8);
    overflow: hidden;
    z-index: 40;
  }
  .rm {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 13px 17px;
    cursor: pointer;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    width: 100%;
    background: transparent;
    border-left: none;
    border-right: none;
    border-top: none;
    text-align: left;
  }
  .rm:last-child {
    border-bottom: none;
  }
  .rm:hover {
    background: rgba(255, 255, 255, 0.03);
  }
  .rm :global(svg) {
    width: 16px;
    height: 16px;
    color: var(--konjo-flame);
    flex: 0 0 auto;
  }
  .rm .rmn {
    font-family: var(--font-sans, 'Space Grotesk', sans-serif);
    font-size: 14px;
    color: var(--konjo-paper, #f5f5f5);
    flex: 1;
  }
  .rm .rms {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: rgba(245, 245, 245, 0.28);
  }
</style>
