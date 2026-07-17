<!--
  AutocompleteSuggest — a filtered suggestion list shown under the goal input
  (or the stack dock's `@org/repo /command` bar) while it's typing a
  `:alias`/`@repo`/`/command` token. Generic over the suggestion source (alias
  preset, repo, or stack command) — the caller maps its own domain type onto
  this shared `{value, label, hint}` row shape.

  `position: fixed`, computed from `anchor`'s real bounding rect — NOT
  absolutely positioned inside its trigger's container. The stack dock's
  `.dockbody` clips to `max-height: 0`→`420px` for its open/close animation,
  and `StackPane.svelte`'s `.pane` clips with `overflow: hidden` too; an
  absolutely-positioned child of either gets silently cut off the instant it
  extends past those bounds — this is what "the autocomplete is hard to see,
  it cuts off" meant in practice. `position: fixed` sidesteps every ancestor
  clip the same way `Popover.svelte` does. Spans (most of) the viewport width
  rather than the anchor's own width, and floats above every other control
  (`z-index` above `Popover.svelte`'s), matching the ask to extend the full
  length of the screen and hover over everything else.

  Rows use `on:mousedown|preventDefault` rather than `on:click` so selecting a
  suggestion never first fires the input's `blur` — the standard trick for
  keeping focus in a text field across an autocomplete click.
-->
<script lang="ts">
  import { onMount, tick } from 'svelte';

  export let items: Array<{ value: string; label: string; hint?: string }>;
  export let activeIndex: number;
  export let onSelect: (value: string) => void;
  /** The input/bar this list hangs below — its real screen rect drives
   *  `top`/`left`, since `position: fixed` has no positioning context of
   *  its own to inherit from a relatively-positioned parent. */
  export let anchor: HTMLElement | null | undefined = null;

  let left = 0;
  let top = 0;
  let width = 320;

  async function computePosition() {
    await tick();
    if (!anchor) return;
    const r = anchor.getBoundingClientRect();
    left = Math.max(10, r.left);
    top = r.bottom + 4;
    // "Extend the full length of the screen" — not clamped to the anchor's
    // own (often narrow, e.g. the stack dock's) width.
    width = Math.max(r.width, window.innerWidth - left - 10);
  }

  // Re-run whenever the result set changes (typing narrows/widens matches)
  // in addition to mount and window resize — the anchor itself doesn't move,
  // but this keeps the computed rect fresh if it does (e.g. a layout shift
  // above it).
  $: if (items) computePosition();
  onMount(computePosition);
</script>

<svelte:window on:resize={computePosition} />

<div
  class="autosuggest"
  role="listbox"
  style="left:{left}px; top:{top}px; width:{width}px;"
>
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
    position: fixed;
    z-index: 70;
    background: var(--konjo-panel, #0a0d0f);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 8px;
    box-shadow: 0 14px 34px rgba(0, 0, 0, 0.6);
    overflow: hidden;
    padding: 4px;
    max-height: 56vh;
    overflow-y: auto;
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
