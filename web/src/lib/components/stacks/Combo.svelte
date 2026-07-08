<!--
  Combo — small type-or-pick numeric input for the schedule popover's
  hour/minute fields. `Dropdown.svelte` can't do free typing (pre-flight
  gate item 2), so this is the narrow addition: a text input clamped to
  `[min, max]` plus a chevron that opens a short pick list of common values.
-->
<script lang="ts">
  import { ICONS } from './icons';

  export let value: number;
  export let options: number[];
  export let min: number;
  export let max: number;
  export let onChange: (n: number) => void;
  export let pad = true;

  let open = false;
  let root: HTMLElement | undefined;

  $: display = pad ? String(value).padStart(2, '0') : String(value);

  function clamp(n: number): number {
    return Math.min(max, Math.max(min, n));
  }

  function onInput(e: Event) {
    const raw = (e.target as HTMLInputElement).value.replace(/[^0-9]/g, '');
    (e.target as HTMLInputElement).value = raw;
    const n = parseInt(raw, 10);
    if (!isNaN(n)) onChange(clamp(n));
  }

  function onBlur(e: Event) {
    const raw = (e.target as HTMLInputElement).value;
    const n = parseInt(raw, 10);
    onChange(isNaN(n) ? value : clamp(n));
  }

  function choose(n: number) {
    onChange(n);
    open = false;
  }

  function onWindowClick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window on:click={onWindowClick} />

<span class="combo" bind:this={root}>
  <input class="cin" value={display} maxlength="2" inputmode="numeric" on:input={onInput} on:blur={onBlur} />
  <button type="button" class="cdd" class:open on:click={() => (open = !open)} aria-label="pick a value">
    {@html ICONS.chevdown}
  </button>
  {#if open}
    <div class="dmenu">
      {#each options as o (o)}
        <button type="button" class="dopt" class:on={o === value} on:click={() => choose(o)}>
          {pad ? String(o).padStart(2, '0') : o}
        </button>
      {/each}
    </div>
  {/if}
</span>

<style>
  .combo {
    position: relative;
    display: inline-flex;
    align-items: center;
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 5px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.04);
  }
  .combo:focus-within {
    border-color: rgba(0, 212, 255, 0.55);
    background: rgba(0, 212, 255, 0.05);
  }
  .cin {
    width: 26px;
    background: transparent;
    border: none;
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    text-align: center;
    padding: 5px 3px;
    outline: none;
  }
  .cdd {
    padding: 5px 6px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    border-left: 1px solid rgba(255, 255, 255, 0.11);
    background: transparent;
    border-top: none;
    border-right: none;
    border-bottom: none;
  }
  .cdd :global(svg) {
    width: 9px;
    height: 9px;
    color: var(--konjo-ice);
  }
  .cdd:hover,
  .cdd.open {
    background: rgba(0, 212, 255, 0.1);
  }
  .dmenu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    background: var(--konjo-panel, #0a0d0f);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 6px;
    box-shadow: 0 12px 34px rgba(0, 0, 0, 0.72);
    z-index: 80;
    max-height: 180px;
    overflow-y: auto;
    padding: 3px;
    display: flex;
    flex-direction: column;
  }
  .dopt {
    padding: 6px 13px;
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    color: rgba(245, 245, 245, 0.46);
    cursor: pointer;
    border-radius: 4px;
    white-space: nowrap;
    background: transparent;
    border: none;
    text-align: left;
  }
  .dopt:hover {
    background: rgba(0, 212, 255, 0.1);
    color: var(--konjo-ice);
  }
  .dopt.on {
    color: var(--konjo-ice);
    background: rgba(0, 212, 255, 0.16);
  }
</style>
