<!--
  GuardrailsPopover — content rendered inside `Popover` for the cardbar's
  sun guardrails button. At loop scope every field is WIRED: `gate`/`until`/
  `onFail` map onto the real `CreateTaskOptions.gate` / `.until` /
  `.on_fail` fields (landed PR #62), and the max-iter stepper edits the same
  `maxIterations` the cardbar's iteration pill does. `budget` is the one
  client-only field — not yet wired to a backend budget parameter.

  Generalized (Stack-1) to value + callback props instead of `card`/
  `paneKey`, and a `scope` prop that hides the gate/until rows at stack
  scope — there is no server-side "whole chain" for a shell precondition/
  exit-condition to run against (see `stores/stack.ts::StackGuardrails`'s
  doc comment), so showing those two fields there would be exactly the
  "inert control that looks enforced" the brief rules out. `onFail` stays
  wired at both scopes; scoped to the stack it drives the chain sequencer's
  on-fail policy (`stores/stackRun.ts`) instead of one task's retry pacing.
-->
<script lang="ts">
  import { type OnFail, type Budget, maxIterationsLabel, cardIterationsLabel } from '$lib/stores/stack';
  import { closePopover } from './Popover.svelte';
  import Toggle from './Toggle.svelte';
  import { ICONS } from './icons';

  export let scope: 'loop' | 'stack' = 'loop';
  export let gate = false;
  export let gateCmd = '';
  export let until = false;
  export let untilCmd = '';
  export let onFail: OnFail;
  export let budget: Budget;
  export let onChangeGate: (patch: { gate?: boolean; gateCmd?: string }) => void = () => {};
  export let onChangeUntil: (patch: { until?: boolean; untilCmd?: string }) => void = () => {};
  export let onChangeOnFail: (value: OnFail) => void;
  export let onChangeBudget: (value: Budget) => void;
  /** Max-iter stepper — the same field the cardbar's iteration pill edits at
   *  loop scope, or the chain loop-count at stack scope. `label` lets the
   *  stack scope call it "loop stacks" instead of "max iter". */
  export let maxIterations: number;
  export let onStep: (delta: number) => void;
  export let iterLabel = 'max iter';

  const ON_FAIL: OnFail[] = ['stop', 'continue', 'backoff'];
  const BUDGETS: Budget[] = ['auto', '200k', 'none'];

  function onGateInput(e: Event) {
    onChangeGate({ gateCmd: (e.target as HTMLInputElement).value });
  }
  function onUntilInput(e: Event) {
    onChangeUntil({ untilCmd: (e.target as HTMLInputElement).value });
  }
</script>

<div class="ph">{@html ICONS.shield}guardrails · {scope === 'stack' ? 'chain limits' : 'run limits'}</div>
<div class="pbody">
  {#if scope === 'loop'}
    <div class="gline">
      <Toggle on={gate} onToggle={() => onChangeGate({ gate: !gate })} accent="sun" />
      <span class="lbl">gate</span>
      <input value={gateCmd} disabled={!gate} placeholder="shell cmd, must pass first" on:input={onGateInput} />
    </div>
    <div class="gline">
      <Toggle on={until} onToggle={() => onChangeUntil({ until: !until })} accent="sun" />
      <span class="lbl">until</span>
      <input value={untilCmd} disabled={!until} placeholder="loop until exit 0" on:input={onUntilInput} />
    </div>
  {/if}
  <div class="gseg-row">
    <span class="lbl">on fail</span>
    <span class="seg">
      {#each ON_FAIL as f (f)}
        <button type="button" class:on={onFail === f} on:click={() => onChangeOnFail(f)}>
          {f}
        </button>
      {/each}
    </span>
  </div>
  <div class="gseg-row last">
    <span class="lbl">budget</span>
    <span class="seg">
      {#each BUDGETS as b (b)}
        <button type="button" class:on={budget === b} on:click={() => onChangeBudget(b)}>
          {b}
        </button>
      {/each}
    </span>
  </div>
</div>
<div class="gfoot">
  <div class="maxiter">
    <span class="lbl">{iterLabel}</span>
    <span class="stepper">
      <button type="button" on:click={() => onStep(-1)} title="fewer iterations">−</button>
      <span class="v">{scope === 'stack' ? maxIterationsLabel(maxIterations) : cardIterationsLabel(maxIterations)}</span>
      <button type="button" on:click={() => onStep(1)} title="more iterations">+</button>
    </span>
  </div>
  <button class="apply" on:click={closePopover}>done</button>
</div>

<style>
  .gline {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 10px;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    color: rgba(245, 245, 245, 0.46);
  }
  .gline .lbl {
    color: var(--konjo-paper, #f5f5f5);
    width: 38px;
    flex: 0 0 auto;
  }
  .gline input {
    flex: 1;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 5px;
    padding: 4px 8px;
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    min-width: 0;
  }
  .gline input:disabled {
    opacity: 0.35;
  }
  .gseg-row {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 10px;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
  }
  .gseg-row.last {
    margin-bottom: 0;
  }
  .gseg-row .lbl {
    width: 52px;
    flex: 0 0 auto;
    text-transform: uppercase;
    font-size: 8.5px;
    letter-spacing: 0.06em;
    color: rgba(245, 245, 245, 0.66);
  }
  .seg {
    display: inline-flex;
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg button {
    padding: 4px 10px;
    font-size: 10px;
    color: rgba(245, 245, 245, 0.66);
    cursor: pointer;
    border: none;
    border-right: 1px solid rgba(255, 255, 255, 0.11);
    background: transparent;
    font-family: var(--font-mono, monospace);
  }
  .seg button:last-child {
    border-right: none;
  }
  .seg button.on {
    background: rgba(255, 204, 0, 0.16);
    color: var(--konjo-sun);
  }
  .gfoot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 10px 13px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .gfoot .maxiter {
    display: flex;
    align-items: center;
    gap: 9px;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
  }
  .gfoot .maxiter .lbl {
    text-transform: uppercase;
    font-size: 8.5px;
    letter-spacing: 0.06em;
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
    color: var(--konjo-sun);
    font-size: 14px;
    cursor: pointer;
  }
  .stepper .v {
    width: 34px;
    text-align: center;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    color: var(--konjo-paper, #f5f5f5);
    border-left: 1px solid rgba(255, 255, 255, 0.11);
    border-right: 1px solid rgba(255, 255, 255, 0.11);
    line-height: 25px;
  }
</style>
