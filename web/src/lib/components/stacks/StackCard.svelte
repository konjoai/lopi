<!--
  StackCard — one loop in the stack: runtag, alias chip, iteration bar,
  hide-inactive summary lines, cardbar (iteration pill + facet popovers +
  config drawer toggle + duplicate/drag/delete), and the config drawer
  itself. All mutation goes through `stores/stack.ts` ops — this component
  holds no data state of its own, only the ephemeral `cfgOpen` UI toggle and
  drag-hover visuals.
-->
<script lang="ts">
  import {
    type StackCard as StackCardT,
    guardActive,
    evalActive,
    configActive,
    guardSummary,
    evalsSummary,
    scheduleSummary,
    maxIterationsLabel,
    stepMaxIterations,
    duplicateInPane,
    removeFromPane,
    updateCardInPane,
    reorderInPaneRelative
  } from '$lib/stores/stack';
  import type { StackDefaults } from '$lib/stores/stackDefaults';
  import type { Option } from '$lib/stores/controls';
  import { ICONS, PRESET_ICON, PRESET_ACCENT } from './icons';
  import { dragging } from './dnd';
  import Popover, { togglePopover } from './Popover.svelte';
  import SchedulePopover from './SchedulePopover.svelte';
  import GuardrailsPopover from './GuardrailsPopover.svelte';
  import EvalsPopover from './EvalsPopover.svelte';
  import ConfigDrawer from './ConfigDrawer.svelte';

  export let card: StackCardT;
  export let paneKey: string;
  export let index: number;
  export let paneDefaults: StackDefaults;
  export let repoOptions: Option[] = [];

  $: accent = card.preset ? PRESET_ACCENT[card.preset] : 'var(--konjo-dim2, rgba(245,245,245,.28))';

  let schedBtn: HTMLButtonElement | undefined;
  let guardBtn: HTMLButtonElement | undefined;
  let evalBtn: HTMLButtonElement | undefined;
  let cfgOpen = false;

  $: schedId = `${card.id}:sched`;
  $: guardId = `${card.id}:guard`;
  $: evalId = `${card.id}:eval`;

  $: guardsOn = guardActive(card.guardrails);
  $: evalsOn = evalActive(card);
  $: configOn = configActive(card, paneDefaults);
  $: showSep = card.scheduled || guardsOn || evalsOn;

  $: statusLabel =
    card.status === 'running' && card.iteration
      ? `running · iter ${card.iteration.current}/${card.iteration.total}`
      : card.status;

  function step(delta: number) {
    updateCardInPane(paneKey, card.id, { maxIterations: stepMaxIterations(card.maxIterations, delta) });
  }

  function dupCard() {
    duplicateInPane(paneKey, card.id);
  }
  function delCard() {
    removeFromPane(paneKey, card.id);
  }

  // ── drag to reorder (within this pane only) ─────────────────────────────────
  let dropBefore = false;
  let dropAfter = false;
  let draggable = false;

  function armDrag() {
    draggable = true;
  }
  function disarmDrag() {
    draggable = false;
  }
  function onDragStart(e: DragEvent) {
    dragging.set({ paneKey, cardId: card.id, index });
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
  function onDragEnd() {
    dragging.set(null);
    dropBefore = false;
    dropAfter = false;
    draggable = false;
  }
  function onDragOver(e: DragEvent) {
    const cur = $dragging;
    if (!cur || cur.paneKey !== paneKey || cur.cardId === card.id) return;
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const before = e.clientY - rect.top < rect.height / 2;
    dropBefore = before;
    dropAfter = !before;
  }
  function onDragLeave() {
    dropBefore = false;
    dropAfter = false;
  }
  function onDrop(e: DragEvent) {
    e.preventDefault();
    const cur = $dragging;
    const before = dropBefore;
    dropBefore = false;
    dropAfter = false;
    if (!cur || cur.paneKey !== paneKey || cur.cardId === card.id) return;
    reorderInPaneRelative(paneKey, cur.index, index, before);
  }
</script>

<div
  class="pc {card.status}"
  class:dragging={draggable && $dragging?.cardId === card.id}
  class:drop-before={dropBefore}
  class:drop-after={dropAfter}
  style="--accent:{accent}"
  role="listitem"
  {draggable}
  on:dragstart={onDragStart}
  on:dragend={onDragEnd}
  on:dragover={onDragOver}
  on:dragleave={onDragLeave}
  on:drop={onDrop}
>
  <span class="runtag {card.status}">{statusLabel}</span>

  <div class="spec">
    {#if card.alias}
      <span class="aliaschip">{@html ICONS.wrench}:{card.alias}</span>
    {/if}
    <span class="md">"{card.goal}"</span>
  </div>

  {#if card.status === 'running' && card.iteration}
    <div class="iterbar">
      {#each Array(card.iteration.total) as _, i}
        <i class={i < card.iteration.current - 1 ? 'done' : i === card.iteration.current - 1 ? 'cur' : ''}></i>
      {/each}
    </div>
  {/if}

  {#if showSep}
    <hr class="sep" />
    {#if card.scheduled}
      <div class="sumln sched">
        <span class="rl">{@html ICONS.cron}schedule</span>
        <span class="txt"><b>{scheduleSummary(card)}</b></span>
      </div>
    {/if}
    {#if guardsOn}
      <div class="sumln guard">
        <span class="rl">{@html ICONS.shield}guards</span>
        <span class="txt">{guardSummary(card)}</span>
      </div>
    {/if}
    {#if evalsOn}
      <div class="sumln eval">
        <span class="rl">{@html ICONS.checkbox}evals</span>
        <span class="txt">{evalsSummary(card)}</span>
      </div>
    {/if}
  {/if}

  <div class="cardbar">
    <span class="iterpill">
      <span class="lb">{@html ICONS.loop}<span class="val">×{maxIterationsLabel(card.maxIterations)}</span></span>
      <span class="steppers">
        <button class="sb" on:click={() => step(-1)} title="fewer iterations">−</button>
        <button class="sb" on:click={() => step(1)} title="more iterations">+</button>
      </span>
    </span>
    <button
      class="ib sched"
      class:act={card.scheduled}
      bind:this={schedBtn}
      on:click={() => togglePopover(schedId)}
      title="schedule"
    >
      {@html ICONS.cron}
    </button>
    <button
      class="ib guard"
      class:act={guardsOn}
      bind:this={guardBtn}
      on:click={() => togglePopover(guardId)}
      title="guardrails"
    >
      {@html ICONS.shield}
    </button>
    <button
      class="ib eval"
      class:act={evalsOn}
      bind:this={evalBtn}
      on:click={() => togglePopover(evalId)}
      title="evals"
    >
      {@html ICONS.checkbox}<span class="cnt">{card.evals.length}</span>
    </button>
    <button class="ib config" class:act={configOn} on:click={() => (cfgOpen = !cfgOpen)} title="run config">
      {@html ICONS.sliders}
    </button>
    <span class="sp"></span>
    <button class="ib" on:click={dupCard} title="duplicate">{@html ICONS.dup}</button>
    <button
      class="ib drag"
      title="drag to reorder"
      on:mousedown={armDrag}
      on:mouseup={disarmDrag}
    >
      {@html ICONS.drag}
    </button>
    <button class="ib danger" on:click={delCard} title="delete">{@html ICONS.trash}</button>
  </div>

  {#if cfgOpen}
    <ConfigDrawer {card} {paneKey} {paneDefaults} {repoOptions} />
  {/if}
</div>

<Popover id={schedId} anchor={schedBtn ?? null} kind="sched">
  <SchedulePopover {card} {paneKey} />
</Popover>
<Popover id={guardId} anchor={guardBtn ?? null} kind="guard">
  <GuardrailsPopover {card} {paneKey} />
</Popover>
<Popover id={evalId} anchor={evalBtn ?? null} kind="eval">
  <EvalsPopover {card} {paneKey} />
</Popover>

<style>
  .pc {
    position: relative;
    background: var(--konjo-card, #0e1214);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 9px;
    padding: 13px 14px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    transition:
      box-shadow 0.12s,
      border-color 0.12s;
  }
  .pc.running {
    border-color: rgba(255, 150, 70, 0.5);
    animation: cardflash 5s ease-in-out infinite;
  }
  .pc.queued {
    border-color: rgba(0, 212, 255, 0.4);
  }
  .pc.done {
    border-color: rgba(0, 255, 157, 0.35);
  }
  .pc.dragging {
    opacity: 0.4;
  }
  .pc.drop-before {
    box-shadow: 0 -3px 0 var(--konjo-ice);
  }
  .pc.drop-after {
    box-shadow: 0 3px 0 var(--konjo-ice);
  }
  @keyframes cardflash {
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
  .runtag {
    position: absolute;
    top: -10px;
    right: 14px;
    font-size: 9px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    background: var(--konjo-black, #0b0e10);
    border: 1px solid;
    border-radius: 3px;
    padding: 2px 8px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    z-index: 2;
  }
  .runtag.running {
    color: var(--konjo-flame);
    border-color: rgba(255, 149, 0, 0.5);
  }
  .runtag.running::before {
    content: '';
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--konjo-flame);
    box-shadow: 0 0 5px var(--konjo-ember);
  }
  .runtag.queued {
    color: var(--konjo-ice);
    border-color: rgba(0, 212, 255, 0.45);
  }
  .runtag.idle {
    color: rgba(245, 245, 245, 0.46);
    border-color: rgba(255, 255, 255, 0.11);
  }
  .runtag.done {
    color: var(--konjo-jade);
    border-color: rgba(0, 255, 157, 0.45);
  }
  .spec {
    font-size: 14px;
    line-height: 1.5;
    margin-top: 3px;
    display: flex;
    align-items: center;
    gap: 9px;
    flex-wrap: wrap;
  }
  .spec .md {
    color: rgba(245, 245, 245, 0.46);
  }
  .aliaschip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12.5px;
    color: var(--stack-teal, #00ffd4);
    border: 1px solid rgba(0, 255, 212, 0.4);
    border-radius: 7px;
    padding: 3px 10px;
    background: rgba(0, 255, 212, 0.07);
  }
  .aliaschip :global(svg) {
    width: 12px;
    height: 12px;
  }
  .iterbar {
    display: flex;
    gap: 4px;
    margin-top: 9px;
  }
  .iterbar i {
    height: 3px;
    width: 22px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.11);
  }
  .iterbar i.done {
    background: var(--konjo-jade);
  }
  .iterbar i.cur {
    background: var(--konjo-flame);
    box-shadow: 0 0 5px var(--konjo-ember);
    animation: pulse 1.8s infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.5;
    }
  }
  .sep {
    height: 1px;
    background: rgba(255, 255, 255, 0.05);
    border: none;
    margin-top: 11px;
  }
  .sumln {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 8px;
    font-size: 9.5px;
    min-width: 0;
  }
  .sumln .rl {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    font-size: 8px;
    flex: 0 0 auto;
    width: 64px;
  }
  .sumln .rl :global(svg) {
    width: 11px;
    height: 11px;
  }
  .sumln.sched .rl {
    color: var(--konjo-ice);
  }
  .sumln.guard .rl {
    color: var(--konjo-sun);
  }
  .sumln.eval .rl {
    color: var(--konjo-jade);
  }
  .sumln .txt {
    color: rgba(245, 245, 245, 0.46);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .sumln.sched .txt b {
    color: var(--konjo-ice);
  }
  .cardbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
  }
  .ib {
    position: relative;
    height: 29px;
    min-width: 29px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.11);
    background: transparent;
    color: rgba(245, 245, 245, 0.28);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    font-size: 11px;
    transition: 0.12s;
  }
  .ib :global(svg) {
    width: 14px;
    height: 14px;
  }
  .ib:hover {
    color: var(--konjo-paper, #f5f5f5);
    border-color: rgba(245, 245, 245, 0.46);
  }
  .ib .cnt {
    font-size: 9px;
    font-weight: 700;
  }
  .ib.sched.act {
    color: var(--konjo-ice);
    border-color: rgba(0, 212, 255, 0.5);
    background: rgba(0, 212, 255, 0.08);
  }
  .ib.danger:hover {
    color: var(--konjo-rose, #ff0066);
    border-color: rgba(255, 0, 102, 0.4);
  }
  .ib.guard.act {
    color: var(--konjo-sun);
    border-color: rgba(255, 204, 0, 0.5);
    background: rgba(255, 204, 0, 0.08);
  }
  .ib.eval.act {
    color: var(--konjo-jade);
    border-color: rgba(0, 255, 157, 0.5);
    background: rgba(0, 255, 157, 0.08);
  }
  .ib.config.act {
    color: var(--stack-violet, #b79bff);
    border-color: rgba(183, 155, 255, 0.5);
    background: rgba(183, 155, 255, 0.08);
  }
  .ib.drag {
    cursor: grab;
  }
  .ib.drag:active {
    cursor: grabbing;
  }
  .sp {
    flex: 1;
  }
  .iterpill {
    display: inline-flex;
    align-items: center;
    height: 29px;
    border: 1px solid rgba(255, 149, 0, 0.5);
    background: rgba(255, 69, 0, 0.09);
    border-radius: 6px;
    overflow: hidden;
    font-size: 11px;
    color: var(--konjo-flame);
    font-weight: 700;
  }
  .iterpill .lb {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 9px;
    height: 100%;
  }
  .iterpill .lb :global(svg) {
    width: 14px;
    height: 14px;
  }
  .iterpill .steppers {
    display: inline-flex;
    align-items: center;
    max-width: 0;
    overflow: hidden;
    transition: max-width 0.24s cubic-bezier(0.5, 0, 0.2, 1);
  }
  .iterpill:hover .steppers,
  .iterpill:focus-within .steppers {
    max-width: 64px;
  }
  .iterpill .sb {
    width: 28px;
    height: 29px;
    border: none;
    border-left: 1px solid rgba(255, 149, 0, 0.35);
    background: transparent;
    color: var(--konjo-flame);
    font-size: 15px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .iterpill .sb:hover {
    background: rgba(255, 149, 0, 0.2);
  }
  @media (prefers-reduced-motion: reduce) {
    .pc.running,
    .iterbar i.cur {
      animation: none;
    }
  }
</style>
