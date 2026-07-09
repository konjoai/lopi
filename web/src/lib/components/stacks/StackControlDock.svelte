<!--
  StackControlDock — the purple stack control area at the base of each
  pane (Stack-1). Reuses the exact per-loop controls (`Popover.svelte`, the
  iteration-pill stepper, the schedule/guardrails/evals/config popovers),
  just scoped to the whole stack instead of one card, plus the pane's
  already-real run/pause/resume/drain machinery (moved here verbatim from
  `StackPane.svelte`'s old plain footer — the mockup shows exactly one run
  button, not two).

  Two placement modes, per `docs/ui/lopi-stack-control-area.html`:
  `STACK_CONTROL_MODE === 'dock'` (shipped default) is a collapsible strip —
  header (STACK chip + summary + chevron) always visible, controls expand
  in the middle, run stays pinned at the bottom in both states.
  `'sticky'` is the always-fully-expanded variant; its CSS ships below,
  unused while the constant reads `'dock'` (`stores/stack.ts`'s doc comment
  on the constant explains the migration path — same shape as
  `SIDEBAR_MODE`).
-->
<script lang="ts">
  import {
    type StackPaneState,
    STACK_CONTROL_MODE,
    stackGuardActive,
    stackEvalActive,
    stackDefaultsActive,
    stackGuardSummary,
    stackEvalsSummary,
    maxIterationsLabel,
    stepMaxIterations,
    cronHuman,
    updateStackConfig,
    duplicateStackInPanes,
    deleteStackFromPanes,
    reorderStacksInPanes,
    type DryRunResult
  } from '$lib/stores/stack';
  import { runs, runStack, pauseStack, resumeStack, type RunPhase } from '$lib/stores/stackRun';
  import { agents } from '$lib/stores/agents';
  import { MODEL_OPTIONS, labelFor, type Option } from '$lib/stores/controls';
  import { draggingPane } from './dnd';
  import { ICONS } from './icons';
  import Popover, { togglePopover } from './Popover.svelte';
  import SchedulePopover from './SchedulePopover.svelte';
  import GuardrailsPopover from './GuardrailsPopover.svelte';
  import EvalsPopover from './EvalsPopover.svelte';
  import StackConfigPopover from './StackConfigPopover.svelte';
  import RunMenu from './RunMenu.svelte';

  export let pane: StackPaneState;
  /** This pane's index in `$panes` — the drag-drop source/target identity,
   *  mirroring `StackCard.svelte`'s own `index` prop one level up. */
  export let index: number;
  export let repoOptions: Option[] = [];

  $: config = pane.config;

  let dockOpen = false;
  let schedBtn: HTMLButtonElement | undefined;
  let guardBtn: HTMLButtonElement | undefined;
  let evalBtn: HTMLButtonElement | undefined;
  let cfgBtn: HTMLButtonElement | undefined;

  $: schedId = `${pane.key}:stack:sched`;
  $: guardId = `${pane.key}:stack:guard`;
  $: evalId = `${pane.key}:stack:eval`;
  $: cfgId = `${pane.key}:stack:config`;

  $: scheduledOn = config.scheduled;
  $: guardsOn = stackGuardActive(config.guardrails);
  $: evalsOn = stackEvalActive(config);
  $: configOn = stackDefaultsActive(config.defaults);
  $: showSummary = scheduledOn || guardsOn || evalsOn || configOn;

  $: modelLabel = labelFor(MODEL_OPTIONS, config.defaults.model);
  $: dockSummary = `${scheduledOn ? cronHuman(config.cron) + ' · ' : ''}loop ×${maxIterationsLabel(config.loopCount)} · ${modelLabel}`;

  function stepLoop(delta: number) {
    updateStackConfig(pane.key, { loopCount: stepMaxIterations(config.loopCount, delta) });
  }

  // ── run stack (moved verbatim from StackPane.svelte's old footer) ───────────
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

  function runMain() {
    if (phase === 'running') {
      pauseStack(pane.key);
    } else if (phase === 'paused') {
      resumeStack(pane.key, config.defaults, agents);
    } else {
      dryRunResult = null;
      runStack(pane.key, 'run', config.defaults, agents);
    }
    runMenuOpen = false;
  }

  function dismissRunError() {
    runs.update((m) => {
      const next = new Map(m);
      next.delete(pane.key);
      return next;
    });
  }

  // ── stack ops: duplicate / drag-reorder / delete (Stack-1 Phase 1) ──────────
  function dupStack() {
    duplicateStackInPanes(pane.key);
  }
  function delStack() {
    runs.update((m) => {
      const next = new Map(m);
      next.delete(pane.key);
      return next;
    });
    deleteStackFromPanes(pane.key);
  }
  function onDragStart(e: DragEvent) {
    draggingPane.set({ paneKey: pane.key, index });
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
  function onDragEnd() {
    draggingPane.set(null);
    dropBefore = false;
    dropAfter = false;
  }

  // ── drop target: the dock's own root, before/after by cursor Y — mirrors
  //    StackCard.svelte's within-pane card drag exactly, one level up. ──────
  let dropBefore = false;
  let dropAfter = false;

  function onDockDragOver(e: DragEvent) {
    const cur = $draggingPane;
    if (!cur || cur.paneKey === pane.key) return;
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const before = e.clientY - rect.top < rect.height / 2;
    dropBefore = before;
    dropAfter = !before;
  }
  function onDockDragLeave() {
    dropBefore = false;
    dropAfter = false;
  }
  function onDockDrop(e: DragEvent) {
    e.preventDefault();
    const cur = $draggingPane;
    const before = dropBefore;
    dropBefore = false;
    dropAfter = false;
    if (!cur || cur.paneKey === pane.key) return;
    reorderStacksInPanes(cur.index, index, before);
  }
</script>

<div
  class="sctrl"
  class:dock={STACK_CONTROL_MODE === 'dock'}
  class:sticky={STACK_CONTROL_MODE === 'sticky'}
  class:open={dockOpen}
  class:drop-before={dropBefore}
  class:drop-after={dropAfter}
  role="group"
  aria-label="stack controls"
  on:dragover={onDockDragOver}
  on:dragleave={onDockDragLeave}
  on:drop={onDockDrop}
>
  {#if STACK_CONTROL_MODE === 'dock'}
    <div class="dockhead">
      <span class="stag">stack</span>
      <span class="dsum">{dockSummary}</span>
      <button class="exp" type="button" on:click={() => (dockOpen = !dockOpen)} aria-expanded={dockOpen} title="stack controls">
        {@html ICONS.chevup}
      </button>
    </div>
  {:else}
    <div class="sctop">
      <span class="stag">stack</span>
      <span class="sp"></span>
    </div>
  {/if}

  <div class="dockbody">
    <div class="inner">
      {#if showSummary}
        {#if scheduledOn}
          <div class="sumln sched">
            <span class="rl">{@html ICONS.cron}schedule</span>
            <span class="txt"><b>{cronHuman(config.cron)}</b></span>
          </div>
          <div class="hintrow">not yet enforced — no whole-chain cron exists server-side yet</div>
        {/if}
        {#if guardsOn}
          <div class="sumln guard">
            <span class="rl">{@html ICONS.shield}guards</span>
            <span class="txt">{stackGuardSummary(config.guardrails)}</span>
          </div>
        {/if}
        {#if evalsOn}
          <div class="sumln eval">
            <span class="rl">{@html ICONS.checkbox}evals</span>
            <span class="txt">{stackEvalsSummary(config)}</span>
          </div>
        {/if}
        {#if configOn}
          <div class="sumln cfg">
            <span class="rl">{@html ICONS.sliders}default</span>
            <span class="txt">model <b>{modelLabel}</b> · every loop inherits</span>
          </div>
        {/if}
      {/if}

      <div class="cardbar">
        <span class="iterpill">
          <span class="lb">{@html ICONS.loop}<span class="val">×{maxIterationsLabel(config.loopCount)}</span></span>
          <span class="steppers">
            <button class="sb" type="button" on:click={() => stepLoop(-1)} title="fewer chain repeats">−</button>
            <button class="sb" type="button" on:click={() => stepLoop(1)} title="more chain repeats">+</button>
          </span>
        </span>
        <button class="ib sched" class:act={scheduledOn} bind:this={schedBtn} on:click={() => togglePopover(schedId)} title="schedule the stack">
          {@html ICONS.cron}
        </button>
        <button class="ib guard" class:act={guardsOn} bind:this={guardBtn} on:click={() => togglePopover(guardId)} title="stack guardrails">
          {@html ICONS.shield}
        </button>
        <button class="ib eval" class:act={evalsOn} bind:this={evalBtn} on:click={() => togglePopover(evalId)} title="stack evals">
          {@html ICONS.checkbox}<span class="cnt">{config.evals.length}</span>
        </button>
        <button class="ib config" class:act={configOn} bind:this={cfgBtn} on:click={() => togglePopover(cfgId)} title="stack default config">
          {@html ICONS.sliders}
        </button>
        <span class="sp"></span>
        <button class="ib" type="button" on:click={dupStack} title="duplicate stack">{@html ICONS.dup}</button>
        <button
          class="ib drag"
          type="button"
          title="drag to reorder stacks"
          draggable="true"
          on:dragstart={onDragStart}
          on:dragend={onDragEnd}
        >
          {@html ICONS.drag}
        </button>
        <button class="ib danger" type="button" on:click={delStack} title="delete stack">{@html ICONS.trash}</button>
      </div>
    </div>
  </div>

  <div class="dockrun">
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
      <button class="runmain" type="button" on:click={runMain} disabled={phase === 'draining'} title="run this stack">
        {@html runIcon} {runLabel}
      </button>
      <button class="runchev" type="button" on:click={() => (runMenuOpen = !runMenuOpen)} aria-expanded={runMenuOpen}>
        {@html ICONS.chevup}
      </button>
    </div>
    {#if runMenuOpen}
      <RunMenu
        paneKey={pane.key}
        defaults={config.defaults}
        {phase}
        onDryRun={(r) => (dryRunResult = r)}
        onClose={() => (runMenuOpen = false)}
      />
    {/if}
  </div>
</div>

<Popover id={schedId} anchor={schedBtn ?? null} kind="sched">
  <SchedulePopover
    scheduled={config.scheduled}
    cron={config.cron}
    onToggle={() => updateStackConfig(pane.key, { scheduled: !config.scheduled })}
    onChange={(next) => updateStackConfig(pane.key, { cron: next })}
  />
</Popover>
<Popover id={guardId} anchor={guardBtn ?? null} kind="guard">
  <GuardrailsPopover
    scope="stack"
    onFail={config.guardrails.onFail}
    budget={config.guardrails.budget}
    onChangeOnFail={(onFail) => updateStackConfig(pane.key, { guardrails: { ...config.guardrails, onFail } })}
    onChangeBudget={(budget) => updateStackConfig(pane.key, { guardrails: { ...config.guardrails, budget } })}
    maxIterations={config.loopCount}
    onStep={stepLoop}
    iterLabel="loop stack"
  />
</Popover>
<Popover id={evalId} anchor={evalBtn ?? null} kind="eval">
  <EvalsPopover evals={config.evals} onChange={(evals) => updateStackConfig(pane.key, { evals })} heading="chain acceptance" />
</Popover>
<Popover id={cfgId} anchor={cfgBtn ?? null} kind="config">
  <StackConfigPopover
    defaults={config.defaults}
    onChange={(patch) => updateStackConfig(pane.key, { defaults: { ...config.defaults, ...patch } })}
    {repoOptions}
  />
</Popover>

<style>
  .sctrl {
    position: relative;
    flex: 0 0 auto;
    background: linear-gradient(180deg, rgba(150, 120, 230, 0.22), rgba(120, 92, 205, 0.14));
    border-top: 1.5px solid rgba(183, 155, 255, 0.55);
    box-shadow: 0 -10px 30px rgba(120, 90, 200, 0.14);
    padding: 14px 16px 16px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
  }
  .sctrl.dock {
    padding-top: 0;
  }
  /* Sticky mode's CSS ships unused while STACK_CONTROL_MODE === 'dock' — see
     the constant's doc comment in stores/stack.ts. */
  .sctrl.sticky {
    border: 1.5px solid rgba(183, 155, 255, 0.5);
    border-radius: 11px;
    margin-top: 14px;
    box-shadow: 0 6px 26px rgba(120, 90, 200, 0.18);
  }
  .sctrl.sticky .dockbody {
    max-height: none;
    overflow: visible;
  }
  .stag {
    font-size: 9px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: #1a1030;
    background: var(--stack-violet, #b79bff);
    border-radius: 4px;
    padding: 3px 10px;
    font-weight: 700;
    flex: 0 0 auto;
  }
  .sctop {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 2px;
  }
  .sctop .sp {
    flex: 1;
  }
  .dockhead {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 13px 0 2px;
  }
  .dockhead .dsum {
    font-size: 10px;
    color: rgba(245, 245, 245, 0.66);
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sctrl.dock.open .dockhead .dsum {
    display: none;
  }
  .dockhead .exp {
    width: 34px;
    height: 34px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 7px;
    background: rgba(0, 0, 0, 0.2);
    color: var(--stack-violet, #b79bff);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex: 0 0 auto;
    margin-left: auto;
  }
  .dockhead .exp :global(svg) {
    width: 16px;
    height: 16px;
    transition: transform 0.2s;
  }
  .sctrl.dock.open .dockhead .exp :global(svg) {
    transform: rotate(180deg);
  }
  .dockbody {
    max-height: 0;
    overflow: hidden;
    transition: max-height 0.26s ease;
  }
  .sctrl.dock.open .dockbody {
    max-height: 420px;
  }
  .dockbody .inner {
    padding-top: 2px;
  }
  .hintrow {
    font-size: 9px;
    color: rgba(245, 245, 245, 0.28);
    padding: 4px 0 0 71px;
  }
  .sumln {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 8px;
    font-size: 9.5px;
  }
  .sumln:first-of-type {
    margin-top: 10px;
  }
  .sumln .rl {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    text-transform: uppercase;
    font-size: 8px;
    letter-spacing: 0.06em;
    width: 64px;
    flex: 0 0 auto;
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
  .sumln.cfg .rl {
    color: #e6ddff;
  }
  .sumln .txt {
    color: rgba(245, 245, 245, 0.66);
  }
  .sumln .txt b {
    color: var(--konjo-paper, #f5f5f5);
  }
  .sumln.sched .txt b {
    color: var(--konjo-ice);
  }
  .sumln.guard .txt b {
    color: var(--konjo-sun);
  }
  .cardbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 13px;
  }
  .ib {
    position: relative;
    height: 29px;
    min-width: 29px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.16);
    background: rgba(0, 0, 0, 0.18);
    color: rgba(245, 245, 245, 0.66);
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
    border-color: rgba(255, 255, 255, 0.32);
  }
  .ib .cnt {
    font-size: 9px;
    font-weight: 700;
  }
  .ib.sched.act {
    color: var(--konjo-ice);
    border-color: rgba(0, 212, 255, 0.6);
    background: rgba(0, 212, 255, 0.14);
  }
  .ib.guard.act {
    color: var(--konjo-sun);
    border-color: rgba(255, 204, 0, 0.6);
    background: rgba(255, 204, 0, 0.14);
  }
  .ib.eval.act {
    color: var(--konjo-jade);
    border-color: rgba(0, 255, 157, 0.6);
    background: rgba(0, 255, 157, 0.14);
  }
  .ib.config.act {
    color: #efe9ff;
    border-color: rgba(255, 255, 255, 0.5);
    background: rgba(255, 255, 255, 0.14);
  }
  .ib.danger:hover {
    color: var(--konjo-rose, #ff0066);
    border-color: rgba(255, 0, 102, 0.4);
  }
  .ib.drag {
    cursor: grab;
  }
  .sp {
    flex: 1;
  }
  .iterpill {
    display: inline-flex;
    align-items: center;
    height: 29px;
    border: 1px solid rgba(255, 149, 0, 0.6);
    background: rgba(255, 120, 0, 0.16);
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
    border-left: 1px solid rgba(255, 149, 0, 0.4);
    background: transparent;
    color: var(--konjo-flame);
    font-size: 15px;
    cursor: pointer;
  }
  .iterpill .sb:hover {
    background: rgba(255, 149, 0, 0.24);
  }
  .dockrun {
    padding-top: 13px;
    position: relative;
  }
  .runbanner {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    margin-bottom: 9px;
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(255, 255, 255, 0.16);
    color: rgba(245, 245, 245, 0.72);
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
    background: rgba(255, 90, 90, 0.1);
    border-color: rgba(255, 90, 90, 0.4);
    color: rgba(255, 170, 170, 0.95);
  }
  .runsplit {
    display: flex;
    border-radius: 8px;
    overflow: hidden;
    width: 100%;
    box-shadow: 0 4px 16px rgba(255, 149, 0, 0.3);
  }
  .runmain {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    flex: 1;
    background: linear-gradient(180deg, #ffb648, #ff9500);
    color: #231000;
    border: none;
    padding: 12px;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
  }
  .runmain :global(svg) {
    width: 15px;
    height: 15px;
  }
  .runmain:disabled {
    opacity: 0.6;
    cursor: not-allowed;
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
  .sctrl.drop-before {
    box-shadow: 0 -3px 0 var(--stack-violet, #b79bff);
  }
  .sctrl.drop-after {
    box-shadow: 0 3px 0 var(--stack-violet, #b79bff);
  }
  @media (prefers-reduced-motion: reduce) {
    .dockbody {
      transition: none;
    }
  }
</style>
