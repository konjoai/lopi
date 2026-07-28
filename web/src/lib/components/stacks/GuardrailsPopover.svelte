<!--
  GuardrailsPopover — content rendered inside `Popover` for the cardbar's
  sun guardrails button. At loop scope every field is WIRED: `gate`/`until`/
  `onFail` map onto the real `CreateTaskOptions.gate` / `.until` /
  `.on_fail` fields (landed PR #62), and the max-iter stepper edits the same
  `maxIterations` the cardbar's iteration pill does. `budget` (the legacy
  token-cap enum) maps to `.budget_tokens`; `budgetPreset`/`budgetUsd` (the
  web-composer loop.toml sprint) map to `.budget_override` — the real preset
  system that also governs the sub-agent fan-out tool list. `isolation` and
  `noProgressLimit` (same sprint) map to `.isolation`/`.no_progress_limit`.

  Generalized (Stack-1) to value + callback props instead of `card`/
  `paneKey`, and a `scope` prop that hides the gate/until/budget/
  budgetPreset/isolation/noProgressLimit rows at stack scope — there is no
  server-side "whole chain" for a shell precondition/exit-condition, token
  cap, budget preset, isolation mode, or no-progress ceiling to apply to (a
  chain is N independent task creations; see `stores/stack.ts::
  StackGuardrails`'s doc comment), so showing those there would be exactly
  the "inert control that looks enforced" the brief rules out — the legacy
  `budget` row used to render at both scopes despite reaching nothing at
  stack scope; Phase 3 (web-composer loop.toml sprint) confined it to loop
  scope like every other per-task-only field. `onFail` alone stays wired at
  both scopes — it drives the chain sequencer's on-fail policy
  (`stores/stackRun.ts`) at stack scope instead of one task's retry pacing.
-->
<script lang="ts">
  import {
    type OnFail,
    type Budget,
    type BudgetPresetChoice,
    type IsolationChoice,
    maxIterationsLabel,
    cardIterationsLabel
  } from '$lib/stores/stack';
  import { closePopover } from './Popover.svelte';
  import Toggle from './Toggle.svelte';
  import { ICONS } from './icons';
  import { autoGrow } from './autoGrow';

  export let scope: 'loop' | 'stack' = 'loop';
  export let gate = false;
  export let gateCmd = '';
  export let until = false;
  export let untilCmd = '';
  export let onFail: OnFail;
  /** Loop-scope only — see the component doc comment on why this no longer
   *  renders at stack scope. Defaulted so a stack-scope caller need not pass it. */
  export let budget: Budget = 'auto';
  export let budgetPreset: BudgetPresetChoice = 'inherit';
  export let budgetUsd: number | undefined = undefined;
  export let isolation: IsolationChoice = 'inherit';
  export let noProgressLimit: number | undefined = undefined;
  export let onChangeGate: (patch: { gate?: boolean; gateCmd?: string }) => void = () => {};
  export let onChangeUntil: (patch: { until?: boolean; untilCmd?: string }) => void = () => {};
  export let onChangeOnFail: (value: OnFail) => void;
  export let onChangeBudget: (value: Budget) => void = () => {};
  export let onChangeBudgetPreset: (value: BudgetPresetChoice) => void = () => {};
  export let onChangeBudgetUsd: (value: number | undefined) => void = () => {};
  export let onChangeIsolation: (value: IsolationChoice) => void = () => {};
  export let onChangeNoProgressLimit: (value: number | undefined) => void = () => {};
  /** Max-iter stepper — the same field the cardbar's iteration pill edits at
   *  loop scope, or the chain loop-count at stack scope. `label` lets the
   *  stack scope call it "loop stacks" instead of "max iter". */
  export let maxIterations: number;
  export let onStep: (delta: number) => void;
  export let iterLabel = 'max iter';

  const ON_FAIL: OnFail[] = ['stop', 'continue', 'backoff'];
  const BUDGETS: Budget[] = ['auto', '200k', 'none'];
  const BUDGET_PRESETS: BudgetPresetChoice[] = ['inherit', 'quick', 'standard', 'deep', 'unlimited'];
  const ISOLATIONS: IsolationChoice[] = ['inherit', 'branch', 'worktree'];

  function onGateInput(e: Event) {
    onChangeGate({ gateCmd: (e.target as HTMLTextAreaElement).value });
  }
  function onUntilInput(e: Event) {
    onChangeUntil({ untilCmd: (e.target as HTMLTextAreaElement).value });
  }
  function onBudgetUsdInput(e: Event) {
    const raw = (e.target as HTMLInputElement).value.trim();
    onChangeBudgetUsd(raw === '' ? undefined : Number(raw));
  }
  function onNoProgressLimitInput(e: Event) {
    const raw = (e.target as HTMLInputElement).value.trim();
    onChangeNoProgressLimit(raw === '' ? undefined : Number(raw));
  }

  /** Rounds away binary float noise (`0.1 + 0.25` etc.) — these are the
   *  chevron buttons' click handlers, not the free-typed input path above. */
  function stepBudgetUsd(delta: number): void {
    onChangeBudgetUsd(Math.max(0, Number(((budgetUsd ?? 0) + delta).toFixed(2))));
  }
  function stepNoProgressLimit(delta: number): void {
    onChangeNoProgressLimit(Math.max(0, (noProgressLimit ?? 0) + delta));
  }
</script>

<div class="ph">{@html ICONS.shield}guardrails · {scope === 'stack' ? 'chain limits' : 'run limits'}</div>
<div class="pbody">
  {#if scope === 'loop'}
    <div class="gline">
      <Toggle on={gate} onToggle={() => onChangeGate({ gate: !gate })} accent="sun" />
      <span class="lbl">gate</span>
      <textarea
        value={gateCmd}
        disabled={!gate}
        placeholder="shell cmd, must pass first"
        on:input={onGateInput}
        use:autoGrow
        rows="1"
      ></textarea>
    </div>
    <div class="gline">
      <Toggle on={until} onToggle={() => onChangeUntil({ until: !until })} accent="sun" />
      <span class="lbl">until</span>
      <textarea
        value={untilCmd}
        disabled={!until}
        placeholder="loop until exit 0"
        on:input={onUntilInput}
        use:autoGrow
        rows="1"
      ></textarea>
    </div>
  {/if}
  <div class="gseg-row" class:last={scope !== 'loop'}>
    <span class="lbl">on fail</span>
    <span class="seg">
      {#each ON_FAIL as f (f)}
        <button type="button" class:on={onFail === f} on:click={() => onChangeOnFail(f)}>
          {f}
        </button>
      {/each}
    </span>
  </div>
  {#if scope === 'loop'}
    <div class="gseg-row">
      <span class="lbl">budget</span>
      <span class="seg">
        {#each BUDGETS as b (b)}
          <button type="button" class:on={budget === b} on:click={() => onChangeBudget(b)}>
            {b}
          </button>
        {/each}
      </span>
    </div>
    <div class="gseg-row">
      <span class="lbl">preset</span>
      <span class="seg">
        {#each BUDGET_PRESETS as p (p)}
          <button type="button" class:on={budgetPreset === p} on:click={() => onChangeBudgetPreset(p)}>
            {p}
          </button>
        {/each}
      </span>
    </div>
    <div class="gline">
      <span class="lbl">usd</span>
      <span class="numstep">
        <span class="prefix">$</span>
        <input
          class="numfield"
          type="number"
          min="0"
          step="0.25"
          value={budgetUsd ?? ''}
          placeholder="inherit"
          on:input={onBudgetUsdInput}
        />
        <span class="chevs">
          <button type="button" on:click={() => stepBudgetUsd(0.25)} title="increase" aria-label="increase usd">{@html ICONS.chevup}</button>
          <button type="button" on:click={() => stepBudgetUsd(-0.25)} title="decrease" aria-label="decrease usd">{@html ICONS.chevdown}</button>
        </span>
      </span>
    </div>
    <div class="gseg-row">
      <span class="lbl">isolation</span>
      <span class="seg">
        {#each ISOLATIONS as i (i)}
          <button type="button" class:on={isolation === i} on:click={() => onChangeIsolation(i)}>
            {i}
          </button>
        {/each}
      </span>
    </div>
    <div class="gline">
      <span class="lbl">stall limit</span>
      <span class="numstep">
        <input
          class="numfield"
          type="number"
          min="0"
          step="1"
          value={noProgressLimit ?? ''}
          placeholder="inherit"
          on:input={onNoProgressLimitInput}
        />
        <span class="chevs">
          <button type="button" on:click={() => stepNoProgressLimit(1)} title="increase" aria-label="increase stall limit">{@html ICONS.chevup}</button>
          <button type="button" on:click={() => stepNoProgressLimit(-1)} title="decrease" aria-label="decrease stall limit">{@html ICONS.chevdown}</button>
        </span>
      </span>
    </div>
    <p class="explain last">stop after this many consecutive runs with no score improvement; 0 (or "inherit") never stops on stall alone.</p>
  {/if}
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
    width: 62px;
    flex: 0 0 auto;
  }
  .gline textarea {
    display: block;
    flex: 1;
    resize: none;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 5px;
    padding: 4px 8px;
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    line-height: 1.5;
    min-width: 0;
    outline: none;
  }
  .gline textarea:disabled {
    opacity: 0.35;
  }
  /* Bordered pill wrapping the bare input, colored chevrons instead of the
     browser's own (inconsistently-styled across browsers, unstyleable to
     match the rest of the popover) native number spinner — same bordered-
     pill-plus-accent-icon language as `Combo.svelte`'s hour/minute fields. */
  .numstep {
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 0;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 5px;
    overflow: hidden;
  }
  .numstep:focus-within {
    border-color: rgba(255, 204, 0, 0.55);
    background: rgba(255, 204, 0, 0.05);
  }
  .numstep .prefix {
    padding-left: 8px;
    color: rgba(245, 245, 245, 0.46);
    font-family: var(--font-mono, monospace);
    font-size: 10px;
  }
  .gline .numfield {
    display: block;
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    padding: 4px 4px 4px 8px;
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    outline: none;
  }
  .numstep .prefix + .numfield {
    padding-left: 2px;
  }
  .gline .numfield::-webkit-outer-spin-button,
  .gline .numfield::-webkit-inner-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }
  .gline .numfield[type='number'] {
    appearance: textfield;
    -moz-appearance: textfield;
  }
  .numstep .chevs {
    display: flex;
    flex-direction: column;
    flex: 0 0 auto;
    border-left: 1px solid rgba(255, 255, 255, 0.11);
    align-self: stretch;
  }
  .numstep .chevs button {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1 1 0;
    width: 20px;
    border: none;
    background: transparent;
    cursor: pointer;
    padding: 0;
  }
  .numstep .chevs button:first-child {
    border-bottom: 1px solid rgba(255, 255, 255, 0.11);
  }
  .numstep .chevs button:hover {
    background: rgba(255, 204, 0, 0.12);
  }
  .numstep .chevs button :global(svg) {
    width: 8px;
    height: 8px;
    color: var(--konjo-sun);
  }
  .explain {
    margin: -3px 0 10px;
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    line-height: 1.5;
    color: rgba(245, 245, 245, 0.4);
  }
  .explain.last {
    margin-bottom: 0;
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
