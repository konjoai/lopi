<script lang="ts">
  /**
   * Selector row for launching tasks — model, effort, priority dropdowns plus
   * repo and branch inputs. Bound to the shared `launchControls` store so the
   * setup persists and is identical across every empty pane.
   */
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import {
    launchControls,
    MODEL_OPTIONS,
    EFFORT_OPTIONS,
    PRIORITY_OPTIONS
  } from '$lib/stores/controls';

  /** Compact mode trims labels for dense pane footers. */
  export let dense = false;
</script>

<div class="controls" class:dense>
  <Dropdown
    {dense}
    label={dense ? '' : 'model'}
    bind:value={$launchControls.model}
    options={MODEL_OPTIONS}
  />
  <Dropdown
    {dense}
    label={dense ? '' : 'effort'}
    bind:value={$launchControls.effort}
    options={EFFORT_OPTIONS}
  />
  <Dropdown
    {dense}
    label={dense ? '' : 'priority'}
    bind:value={$launchControls.priority}
    options={PRIORITY_OPTIONS}
  />
  <label class="field">
    {#if !dense}<span class="field-label">repo</span>{/if}
    <input
      type="text"
      placeholder="./path or owner/repo"
      bind:value={$launchControls.repo}
      spellcheck="false"
    />
  </label>
  <label class="field">
    {#if !dense}<span class="field-label">branch</span>{/if}
    <input type="text" placeholder="auto" bind:value={$launchControls.branch} spellcheck="false" />
  </label>
</div>

<style>
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px;
  }
  .field {
    display: inline-flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .field-label {
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    opacity: 0.4;
    padding-left: 2px;
  }
  .field input {
    padding: 5px 8px;
    border-radius: 7px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.025);
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    outline: none;
    min-width: 0;
    width: 130px;
    transition: border-color 0.15s;
  }
  .dense .field input {
    padding: 3px 6px;
    font-size: 10px;
    width: 96px;
  }
  .field input:focus {
    border-color: rgb(var(--konjo-accent-rgb) / 0.5);
  }
  .field input::placeholder {
    opacity: 0.3;
  }
</style>
