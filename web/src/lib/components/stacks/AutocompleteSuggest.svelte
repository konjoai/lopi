<!--
  AutocompleteSuggest — a filtered suggestion list shown under the goal input
  while it's typing a `:alias` or `@repo` token (see `StackCard.svelte`'s
  alias and repo autocomplete). Deliberately lighter than
  `Popover.svelte`/`Dropdown.svelte`: those are click-to-open selects, this
  reacts to live typing and has no anchor-flip/outside-click machinery of its
  own — it lives and dies with the `.goalwrap` it's absolutely positioned
  inside. Generic over the suggestion source (alias preset or repo) — the
  caller maps its own domain type (`AliasSuggestion`/`RepoSuggestion`) onto
  this shared `{value, label, hint}` row shape.

  Rows use `on:mousedown|preventDefault` rather than `on:click` so selecting a
  suggestion never first fires the input's `blur` — the standard trick for
  keeping focus in a text field across an autocomplete click.
-->
<script lang="ts">
  export let items: Array<{ value: string; label: string; hint?: string }>;
  export let activeIndex: number;
  export let onSelect: (value: string) => void;
</script>

<div class="autosuggest" role="listbox">
  {#each items as item, i (item.value)}
    <button type="button" class="asrow" class:active={i === activeIndex} on:mousedown|preventDefault={() => onSelect(item.value)}>
      <span class="aname">{item.value}</span>
      <span class="alabel">{item.label}</span>
      {#if item.hint}<span class="ahint">{item.hint}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .autosuggest {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 30;
    background: var(--konjo-panel, #0a0d0f);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 8px;
    box-shadow: 0 14px 34px rgba(0, 0, 0, 0.6);
    overflow: hidden;
    padding: 4px;
  }
  .asrow {
    display: flex;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: 5px;
    background: transparent;
    cursor: pointer;
    text-align: left;
  }
  .asrow.active,
  .asrow:hover {
    background: rgba(0, 255, 212, 0.09);
  }
  .aname {
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    font-weight: 700;
    color: var(--stack-teal, #00ffd4);
  }
  .alabel {
    font-family: var(--font-sans, sans-serif);
    font-size: 11px;
    color: var(--konjo-paper, #f5f5f5);
  }
  .ahint {
    margin-left: auto;
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: rgba(245, 245, 245, 0.4);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
