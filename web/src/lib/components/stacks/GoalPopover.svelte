<!--
  GoalPopover — content rendered inside `Popover` (kind="goal") for the
  dock's gauge/goal button, and (round 2, item 9) each card's own goal
  button. Explains what "pursue goal" actually does (the toggle used to be a
  bare, unconfigurable button — see the plan this ships with) and exposes
  `StackGoal.noProgressLimit`, a field that already existed on the model but
  had no UI writer anywhere until now.

  `scope` distinguishes the two call sites: 'stack' (default) is the
  original dock-level behavior unchanged; 'card' (round 2) drops the
  no-progress-limit stepper entirely — that field drives the *stack*
  sequencer's own re-run-the-whole-chain loop, which has no per-card
  execution equivalent, so showing it at card scope would be exactly the
  "inert control that looks enforced" this codebase's own conventions rule
  out (see `stores/stack.ts::StackCard.goalPursuit`'s doc comment). The
  `noProgressLimit`/`onChangeNoProgressLimit` props are only read in 'stack'
  scope and are optional so a card-scope caller doesn't need to supply them.
-->
<script lang="ts">
  import { closePopover } from './Popover.svelte';
  import Toggle from './Toggle.svelte';
  import { ICONS } from './icons';

  export let pursue: boolean;
  export let noProgressLimit: number | undefined = undefined;
  /** True once `pursue` is on *and* the stack/card carries real
   *  chain-acceptance evals beyond the baseline — mirrors
   *  `stackPursuesGoal`/`cardPursuesGoal`. When false the toggle is on but
   *  inert, so we say so. */
  export let pursues: boolean;
  export let onTogglePursue: () => void;
  export let onChangeNoProgressLimit: ((value: number) => void) | undefined = undefined;
  /** 'stack' (default) — the dock's original chain-wide goal. 'card' — this
   *  loop's own goal; hides the no-progress-limit stepper (see file doc). */
  export let scope: 'stack' | 'card' = 'stack';

  function step(delta: number) {
    onChangeNoProgressLimit?.(Math.max(0, (noProgressLimit ?? 0) + delta));
  }
</script>

<div class="ph">{@html ICONS.gauge}goal</div>
<div class="pbody">
  {#if scope === 'stack'}
    <p class="explain">
      When on, the stack re-runs its whole chain of loops until the chain-acceptance evals pass — "pursue goal" instead
      of a single "run stack".
    </p>
  {:else}
    <p class="explain">
      When on, this loop is pursuing its own acceptance: the backend already retries it (up to its iteration cap) until
      its evals pass — "pursue" just makes that explicit instead of a loop silently self-reporting done.
    </p>
  {/if}
  <div class="gline">
    <Toggle on={pursue} onToggle={onTogglePursue} accent="flame" />
    <span class="lbl">pursue</span>
  </div>
  {#if pursue && !pursues}
    <p class="hint">
      {scope === 'stack'
        ? 'add chain-acceptance evals for the goal to pursue — a goal with nothing to check is inert'
        : 'add evals for this loop to pursue — a goal with nothing to check is inert'}
    </p>
  {/if}
  {#if scope === 'stack'}
    <div class="gseg-row last">
      <span class="lbl">no-progress limit</span>
      <span class="stepper">
        <button type="button" on:click={() => step(-1)} title="fewer tolerated non-gaining runs">−</button>
        <span class="v">{noProgressLimit === 0 ? 'off' : noProgressLimit}</span>
        <button type="button" on:click={() => step(1)} title="more tolerated non-gaining runs">+</button>
      </span>
    </div>
    <p class="explain small">stop after this many consecutive chain-runs with no gain; 0 disables the no-progress check.</p>
  {/if}
</div>
<div class="popfoot">
  <button class="apply" on:click={closePopover}>done</button>
</div>

<style>
  .explain {
    margin: 0 0 10px;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    line-height: 1.5;
    color: rgba(245, 245, 245, 0.6);
  }
  .explain.small {
    margin: 8px 0 0;
    font-size: 9px;
    color: rgba(245, 245, 245, 0.4);
  }
  .hint {
    margin: -4px 0 10px;
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: rgba(245, 245, 245, 0.4);
  }
  .gline {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 10px;
  }
  .gline .lbl {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    color: var(--konjo-paper, #f5f5f5);
  }
  .gseg-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 9px;
  }
  .gseg-row .lbl {
    text-transform: uppercase;
    font-size: 8.5px;
    letter-spacing: 0.06em;
    font-family: var(--font-mono, monospace);
    color: rgba(245, 245, 245, 0.66);
  }
  .stepper {
    display: inline-flex;
    align-items: center;
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 6px;
    overflow: hidden;
  }
  .stepper button {
    width: 24px;
    height: 25px;
    border: none;
    background: transparent;
    color: var(--konjo-flame);
    font-size: 14px;
    cursor: pointer;
  }
  .stepper .v {
    min-width: 28px;
    text-align: center;
    padding: 0 4px;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    color: var(--konjo-paper, #f5f5f5);
    border-left: 1px solid rgba(255, 255, 255, 0.11);
    border-right: 1px solid rgba(255, 255, 255, 0.11);
    line-height: 25px;
  }
</style>
