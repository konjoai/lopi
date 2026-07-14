<script lang="ts">
  /**
   * Konjo dropdown — a custom, fully-styled select. Not a native `<select>`:
   * keyboard-navigable, animated, accent-aware, with an optional per-option
   * hint. Closes on outside-click or Escape.
   *
   * Options carrying a `group` render under section headers; a catalog where
   * nothing carries one — every field but `repo` — comes back as a single flat
   * list and renders exactly as it always has. Grouping keys off the data, not a
   * flag, so there is no mode to get wrong. See `stores/optionMenu.ts`.
   */
  import { createEventDispatcher, tick } from 'svelte';
  import type { Option } from '$lib/stores/controls';
  import { groupedMenu } from '$lib/stores/optionMenu';

  export let value: string;
  export let options: Option[] = [];
  export let label = '';
  /** Compact icon-style trigger (used in dense pane headers). */
  export let dense = false;
  /** Optional leading icon (raw SVG markup). In dense mode the icon + label
   *  sit *inside* the trigger, rendering a horizontal `icon · LABEL · value`
   *  chip (the config-drawer look) instead of the stacked label. */
  export let icon = '';
  /** Show a filter box above the list. Worth it past a few dozen options (the
   *  repo picker runs to the hundreds); noise below that. */
  export let searchable = false;

  const dispatch = createEventDispatcher<{ change: string }>();

  let open = false;
  let root: HTMLDivElement;
  let input: HTMLInputElement | undefined;
  let listEl: HTMLUListElement | undefined;
  let activeIndex = 0;
  let query = '';

  $: selected = options.find((o) => o.value === value) ?? options[0];
  $: selectedLabel = selected?.label ?? value;

  // `menu.flat` is what the cursor indexes — the FILTERED rows, in render order.
  // Indexing `options` instead would walk rows the query has hidden.
  $: menu = groupedMenu(options, searchable ? query : '');
  $: rows = menu.flat;
  // A shrinking result set must not strand the cursor past the end.
  $: if (activeIndex >= rows.length) activeIndex = Math.max(0, rows.length - 1);

  async function toggle() {
    open = !open;
    if (!open) return;
    query = '';
    activeIndex = Math.max(0, rows.findIndex((o) => o.value === value));
    await tick();
    input?.focus();
    scrollActiveIntoView();
  }

  function choose(opt: Option) {
    value = opt.value;
    open = false;
    query = '';
    dispatch('change', opt.value);
  }

  /** Keep the cursor visible — with hundreds of rows it leaves the viewport
   *  after a couple of keypresses. */
  async function scrollActiveIntoView() {
    await tick();
    listEl?.querySelector('.kdrop-item.active')?.scrollIntoView({ block: 'nearest' });
  }

  function step(delta: number) {
    if (!rows.length) return;
    activeIndex = (activeIndex + delta + rows.length) % rows.length;
    scrollActiveIntoView();
  }

  function onKeydown(e: KeyboardEvent) {
    if (!open) {
      if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown') {
        e.preventDefault();
        toggle();
      }
      return;
    }
    if (e.key === 'Escape') {
      // A live query is the first thing Escape should undo — closing outright
      // would throw away the filter *and* the menu on one keystroke.
      if (query) query = '';
      else open = false;
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      step(1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      step(-1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (rows[activeIndex]) choose(rows[activeIndex]);
    }
  }

  function onWindowClick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window on:click={onWindowClick} />

<div class="kdrop" class:dense bind:this={root}>
  {#if label && !(dense && (icon || label))}
    <span class="kdrop-label">{label}</span>
  {/if}
  <button
    type="button"
    class="kdrop-trigger"
    class:open
    class:chip={dense && (icon || label)}
    on:click|stopPropagation={toggle}
    on:keydown={onKeydown}
    aria-haspopup="listbox"
    aria-expanded={open}
  >
    {#if dense && icon}<span class="kdrop-icon">{@html icon}</span>{/if}
    {#if dense && label}<span class="kdrop-cl">{label}</span>{/if}
    <span class="kdrop-value">{selectedLabel}</span>
    <span class="kdrop-caret" class:open>⌄</span>
  </button>

  {#if open}
    <div class="kdrop-panel">
      {#if searchable}
        <!-- svelte-ignore a11y-autofocus -->
        <input
          class="kdrop-search"
          type="text"
          placeholder="search…"
          bind:this={input}
          bind:value={query}
          on:click|stopPropagation
          on:keydown={onKeydown}
        />
      {/if}
      <ul class="kdrop-menu" role="listbox" bind:this={listEl}>
        {#each menu.pinned as row (row.opt.value)}
          <li role="option" aria-selected={row.opt.value === value}>
            <button
              type="button"
              class="kdrop-item"
              class:stacked={searchable}
              class:active={row.index === activeIndex}
              class:selected={row.opt.value === value}
              on:click|stopPropagation={() => choose(row.opt)}
              on:mouseenter={() => (activeIndex = row.index)}
            >
              <span class="kdrop-item-label">{row.opt.label}</span>
              {#if row.opt.hint}<span class="kdrop-item-hint">{row.opt.hint}</span>{/if}
            </button>
          </li>
        {/each}

        {#each menu.groups as group (group.key)}
          <li class="kdrop-section" role="presentation">{group.key}</li>
          {#each group.rows as row (row.opt.value)}
            <li role="option" aria-selected={row.opt.value === value}>
              <button
                type="button"
                class="kdrop-item"
                class:stacked={searchable}
                class:active={row.index === activeIndex}
                class:selected={row.opt.value === value}
                on:click|stopPropagation={() => choose(row.opt)}
                on:mouseenter={() => (activeIndex = row.index)}
              >
                <span class="kdrop-item-label">{row.opt.label}</span>
                {#if row.opt.hint}<span class="kdrop-item-hint">{row.opt.hint}</span>{/if}
              </button>
            </li>
          {/each}
        {/each}

        {#if !rows.length}
          <li class="kdrop-empty" role="presentation">no match</li>
        {/if}
      </ul>
    </div>
  {/if}
</div>

<style>
  .kdrop {
    position: relative;
    display: inline-flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .kdrop-label {
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    opacity: 0.4;
    padding-left: 2px;
  }
  .kdrop-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 5px 8px;
    border-radius: 7px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.025);
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    cursor: pointer;
    transition:
      border-color 0.15s,
      background 0.15s;
    min-width: 0;
  }
  .dense .kdrop-trigger {
    padding: 3px 6px;
    font-size: 10px;
  }
  /* Horizontal config-drawer chip: [icon] LABEL value ⌄ (matches the mockup's
     .cfgchip). The parent sets --konjo-accent-rgb per field, colouring the
     leading icon exactly like the design's per-field accent. */
  .kdrop-trigger.chip {
    gap: 6px;
    padding: 7px 11px;
    font-size: 11px;
  }
  .kdrop-icon {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .kdrop-icon :global(svg) {
    width: 12px;
    height: 12px;
    color: rgb(var(--konjo-accent-rgb, 245 245 245));
  }
  .kdrop-cl {
    font-size: 8px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    opacity: 0.5;
    flex: 0 0 auto;
  }
  .kdrop-trigger:hover,
  .kdrop-trigger.open {
    border-color: rgb(var(--konjo-accent-rgb) / 0.5);
    background: rgb(var(--konjo-accent-rgb) / 0.06);
  }
  .kdrop-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    /* The ellipsis above is inert without a bound: a chip is `flex: 0 0 auto`
       inside the drawer, so it grows to whatever the label needs and hands the
       pane a horizontal scrollbar. Every real label but a disambiguated repo
       (`konjoai/squish · squish-w100-hf-url-normalize`) is far shorter. */
    max-width: 13rem;
  }
  .kdrop-caret {
    font-size: 10px;
    opacity: 0.5;
    transition: transform 0.18s ease;
    flex-shrink: 0;
  }
  .kdrop-caret.open {
    transform: rotate(180deg);
  }
  /* The floating panel: the (optional) search box pinned above a scrolling
     list, so filtering never scrolls the input out of reach. */
  .kdrop-panel {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 50;
    min-width: 100%;
    /* Cap the panel so a long row can't widen it past the pane and give the
       pane a horizontal scrollbar. Rows stack their hint under the label
       (`.kdrop-item.stacked`) precisely so this can stay narrow. */
    max-width: 20rem;
    border-radius: 9px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(8, 8, 10, 0.96);
    backdrop-filter: blur(12px);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.6);
    animation: kdrop-in 0.14s cubic-bezier(0.16, 1, 0.3, 1);
    overflow: hidden;
  }
  .kdrop-search {
    display: block;
    width: 100%;
    box-sizing: border-box;
    padding: 7px 10px;
    border: none;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.03);
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    outline: none;
  }
  .kdrop-search::placeholder {
    opacity: 0.35;
  }
  .kdrop-search:focus {
    border-bottom-color: rgb(var(--konjo-accent-rgb) / 0.5);
  }
  .kdrop-section {
    padding: 6px 8px 3px;
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgb(var(--konjo-accent-rgb, 245 245 245));
    opacity: 0.75;
  }
  .kdrop-empty {
    padding: 8px;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    opacity: 0.4;
  }
  .kdrop-menu {
    max-height: 240px;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 4px;
  }
  @keyframes kdrop-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .kdrop-item {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
  }
  .kdrop-item.active {
    background: rgb(var(--konjo-accent-rgb) / 0.12);
  }
  .kdrop-item.selected .kdrop-item-label {
    color: var(--konjo-accent);
  }
  .kdrop-item-label {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .kdrop-item-hint {
    font-size: 9px;
    opacity: 0.4;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Long-list rows (the repo picker) stack their hint under the label instead of
     beside it. A repo hint is a full path — inline, it would demand ~480px and
     hand the pane a horizontal scrollbar. Short catalogs keep the inline form,
     where the hint is a word or two ("deepest reasoning"). */
  .kdrop-item.stacked {
    flex-direction: column;
    align-items: stretch;
    gap: 1px;
  }
  /* No `direction: rtl` head-truncation trick here, tempting as it is: bidi
     reorders a path's neutral leading `/` to the end, so `/Users/w/x` *renders*
     as `Users/w/x/`. A path shown wrong is worse than one shown short — and the
     disambiguating detail lives in the label's `· dirname` suffix anyway, not
     here. Stacked rows leave room for ~55 characters before this matters. */
  .kdrop-item.stacked .kdrop-item-hint {
    text-align: left;
  }
</style>
