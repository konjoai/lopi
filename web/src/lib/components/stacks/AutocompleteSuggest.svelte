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
  clip the same way `Popover.svelte` does. Floats above every other control
  (`z-index` above `Popover.svelte`'s).

  Width is clamped to the anchor's own enclosing `.pane` (falling back to the
  viewport when no `.pane` ancestor exists), not the full window width — an
  earlier version spanned to the viewport edge regardless of which pane
  triggered it, so the left pane's suggestion list visibly overlapped the
  entire right pane in a multi-stack board. Still expands past the anchor's
  own (often narrow) width when the pane has room, just never past the
  pane's own right edge.

  Rows use `on:mousedown|preventDefault` rather than `on:click` so selecting a
  suggestion never first fires the input's `blur` — the standard trick for
  keeping focus in a text field across an autocomplete click.

  Each row carries an optional `kind` — the same facet identity `ChipInput`'s
  `CHIP_CLASS` and `StackCard.svelte`'s `.gchip` buttons already color by
  (`alias` teal, `repo`/`model` ice, `effort` ember, `branch` jade, `autonomy`
  violet, `eval` mint, `guard` sun, `schedule` plasma, `maxx`/`goal` flame,
  `claude` rose) — so a suggestion row reads as the same color its resolved
  chip will render in once picked, instead of every row rendering identically
  regardless of whether it's an alias, a repo, or a `;command` value.
-->
<script lang="ts">
  import { onMount, tick } from 'svelte';

  export let items: Array<{ value: string; label: string; hint?: string; kind?: string }>;
  export let activeIndex: number;
  export let onSelect: (value: string) => void;
  /** The input/bar this list hangs below — its real screen rect drives
   *  `top`/`left`, since `position: fixed` has no positioning context of
   *  its own to inherit from a relatively-positioned parent. */
  export let anchor: HTMLElement | null | undefined = null;

  /** Facet → accent, mirroring `ChipInput.svelte`'s `CHIP_CLASS` palette
   *  (kept as literal colors here rather than a shared import — each
   *  composer-grammar surface, `.gchip` included, already owns its palette
   *  as component-local literals, and this one has no chip class to reuse
   *  since a suggestion row isn't a chip yet). */
  const KIND_COLOR: Record<string, string> = {
    alias: '#00ffd4',
    repo: '#00d4ff',
    model: '#00d4ff',
    effort: '#ff4500',
    branch: '#00ff9d',
    autonomy: '#b79bff',
    eval: '#3be6c8',
    guard: '#ffcc00',
    schedule: '#5ee6ff',
    maxx: '#ff9500',
    goal: '#ff9500',
    loop: '#ffcc00',
    claude: '#ff0066'
  };

  function accentFor(kind: string | undefined): string {
    return KIND_COLOR[kind ?? ''] ?? '#00ffd4';
  }

  let left = 0;
  let top = 0;
  let width = 320;

  async function computePosition() {
    await tick();
    if (!anchor) return;
    const r = anchor.getBoundingClientRect();
    // Clamp to the enclosing pane, not the viewport — see the file doc on
    // why (a multi-pane board otherwise lets one pane's list cover another).
    const paneEl = anchor.closest('.pane');
    const boundRight = paneEl ? paneEl.getBoundingClientRect().right - 10 : window.innerWidth - 10;
    left = Math.max(10, r.left);
    top = r.bottom + 4;
    width = Math.max(r.width, boundRight - left);
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
    <button
      type="button"
      class="asrow"
      class:active={i === activeIndex}
      style="--row-accent: {accentFor(item.kind)}"
      on:mousedown|preventDefault={() => onSelect(item.value)}
    >
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
    background: color-mix(in srgb, var(--row-accent, #00ffd4) 9%, transparent);
  }
  .aname {
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    font-weight: 700;
    color: var(--row-accent, #00ffd4);
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
