<!--
  EvalsPopover — content rendered inside `Popover` for the cardbar's jade
  evals button. Client-only intent (§3 of the UI-2 brief, unchanged by
  Stack-1) — eval execution doesn't exist server-side yet at either scope,
  so this is purely an editor of which checks *would* run, never a
  pass/fail state. Baseline is locked-on. Generalized (Stack-1) to an
  `evals`/`onChange` value pair instead of `card`/`paneKey`, so the same
  component mounts scoped to one loop or to the whole stack ("chain
  acceptance" checks in the purple control dock).
-->
<script lang="ts">
  import { type EvalRef, EVAL_CATALOG, EVAL_SUITES, BASELINE_EVAL, toggleEval, applySuite } from '$lib/stores/stack';
  import { closePopover } from './Popover.svelte';
  import { ICONS } from './icons';

  export let evals: EvalRef[];
  export let onChange: (evals: EvalRef[]) => void;
  export let heading = 'loop validation';

  const SUITE_KEYS = Object.keys(EVAL_SUITES);

  function isOn(name: string): boolean {
    return evals.some((e) => e.name === name);
  }
  function toggle(name: string) {
    if (name === BASELINE_EVAL.name) return;
    onChange(toggleEval(evals, name));
  }
  function suite(key: string) {
    onChange(applySuite(evals, EVAL_SUITES[key]));
  }
</script>

<div class="ph">{@html ICONS.checkbox}evals · {heading}</div>
<div class="pbody">
  {#each EVAL_CATALOG as e (e.name)}
    {@const locked = e.name === BASELINE_EVAL.name}
    <button
      type="button"
      class="echk"
      class:on={isOn(e.name)}
      class:locked
      disabled={locked}
      on:click={() => toggle(e.name)}
    >
      <span class="box">{@html ICONS.check}</span>
      <span class="en">{e.name}</span>
      <span class="tier tier-{e.tier}">{e.tier}</span>
    </button>
  {/each}
</div>
<div class="evalfoot">
  <div class="suitecol">
    <span class="nrl">suite:</span>
    <div class="sgroup">
      {#each SUITE_KEYS as key (key)}
        <button type="button" class="sbtn" class:kcqf={key === 'kcqf'} on:click={() => suite(key)}>{key}</button>
      {/each}
    </div>
  </div>
  <button class="apply" on:click={closePopover}>done</button>
</div>

<style>
  .echk {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 6px 7px;
    border-radius: 6px;
    cursor: pointer;
    transition: 0.12s;
    width: 100%;
    background: transparent;
    border: none;
    text-align: left;
  }
  .echk:hover {
    background: rgba(255, 255, 255, 0.02);
  }
  .echk .box {
    width: 16px;
    height: 16px;
    border-radius: 4px;
    border: 1.5px solid rgba(255, 255, 255, 0.11);
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 16px;
  }
  .echk .box :global(svg) {
    width: 11px;
    height: 11px;
    color: #04120c;
    opacity: 0;
  }
  .echk.on .box {
    background: var(--konjo-jade);
    border-color: var(--konjo-jade);
  }
  .echk.on .box :global(svg) {
    opacity: 1;
  }
  .echk .en {
    flex: 1;
    font-family: var(--font-mono, monospace);
    font-size: 11.5px;
    color: rgba(245, 245, 245, 0.46);
  }
  .echk.on .en {
    color: var(--konjo-paper, #f5f5f5);
  }
  .echk .tier {
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    border: 1px solid;
    border-radius: 10px;
    padding: 1px 7px;
    flex: 0 0 auto;
  }
  .tier-base {
    color: var(--konjo-jade);
    border-color: rgba(0, 255, 157, 0.3);
  }
  .tier-test {
    color: var(--konjo-ice);
    border-color: rgba(0, 212, 255, 0.3);
  }
  .tier-judge {
    color: var(--stack-violet, #b79bff);
    border-color: rgba(183, 155, 255, 0.3);
  }
  .tier-suite {
    color: var(--konjo-sun);
    border-color: rgba(255, 204, 0, 0.3);
  }
  .echk.locked {
    opacity: 0.6;
    cursor: default;
  }
  .evalfoot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 14px;
    padding: 10px 13px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .suitecol {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: rgba(245, 245, 245, 0.66);
    min-width: 0;
  }
  .suitecol .nrl {
    padding-top: 5px;
  }
  .sgroup {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
  }
  .sbtn {
    border: 1px dashed rgba(255, 255, 255, 0.28);
    border-radius: 11px;
    padding: 3px 10px;
    color: var(--konjo-paper, #f5f5f5);
    cursor: pointer;
    transition: 0.12s;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    background: transparent;
  }
  .sbtn:hover {
    color: var(--konjo-jade);
    border-color: rgba(0, 255, 157, 0.55);
    background: rgba(0, 255, 157, 0.06);
  }
  .sbtn.kcqf {
    color: var(--konjo-sun);
    border-color: rgba(255, 204, 0, 0.4);
  }
</style>
