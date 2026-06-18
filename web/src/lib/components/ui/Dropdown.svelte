<script lang="ts">
  /**
   * Konjo dropdown — a custom, fully-styled select. Not a native `<select>`:
   * keyboard-navigable, animated, accent-aware, with an optional per-option
   * hint. Closes on outside-click or Escape.
   */
  import { createEventDispatcher, tick } from 'svelte';
  import type { Option } from '$lib/stores/controls';

  export let value: string;
  export let options: Option[] = [];
  export let label = '';
  /** Compact icon-style trigger (used in dense pane headers). */
  export let dense = false;

  const dispatch = createEventDispatcher<{ change: string }>();

  let open = false;
  let root: HTMLDivElement;
  let activeIndex = 0;

  $: selected = options.find((o) => o.value === value) ?? options[0];
  $: selectedLabel = selected?.label ?? value;

  async function toggle() {
    open = !open;
    if (open) {
      activeIndex = Math.max(0, options.findIndex((o) => o.value === value));
      await tick();
    }
  }

  function choose(opt: Option) {
    value = opt.value;
    open = false;
    dispatch('change', opt.value);
  }

  function onKeydown(e: KeyboardEvent) {
    if (!open) {
      if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown') {
        e.preventDefault();
        toggle();
      }
      return;
    }
    if (e.key === 'Escape') open = false;
    else if (e.key === 'ArrowDown') {
      e.preventDefault();
      activeIndex = (activeIndex + 1) % options.length;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      activeIndex = (activeIndex - 1 + options.length) % options.length;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      choose(options[activeIndex]);
    }
  }

  function onWindowClick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window on:click={onWindowClick} />

<div class="kdrop" class:dense bind:this={root}>
  {#if label}
    <span class="kdrop-label">{label}</span>
  {/if}
  <button
    type="button"
    class="kdrop-trigger"
    class:open
    on:click|stopPropagation={toggle}
    on:keydown={onKeydown}
    aria-haspopup="listbox"
    aria-expanded={open}
  >
    <span class="kdrop-value">{selectedLabel}</span>
    <span class="kdrop-caret" class:open>⌄</span>
  </button>

  {#if open}
    <ul class="kdrop-menu" role="listbox">
      {#each options as opt, i (opt.value)}
        <li role="option" aria-selected={opt.value === value}>
          <button
            type="button"
            class="kdrop-item"
            class:active={i === activeIndex}
            class:selected={opt.value === value}
            on:click|stopPropagation={() => choose(opt)}
            on:mouseenter={() => (activeIndex = i)}
          >
            <span class="kdrop-item-label">{opt.label}</span>
            {#if opt.hint}<span class="kdrop-item-hint">{opt.hint}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
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
  .kdrop-trigger:hover,
  .kdrop-trigger.open {
    border-color: rgb(var(--konjo-accent-rgb) / 0.5);
    background: rgb(var(--konjo-accent-rgb) / 0.06);
  }
  .kdrop-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .kdrop-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 50;
    min-width: 100%;
    max-height: 240px;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 4px;
    border-radius: 9px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(8, 8, 10, 0.96);
    backdrop-filter: blur(12px);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.6);
    animation: kdrop-in 0.14s cubic-bezier(0.16, 1, 0.3, 1);
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
  .kdrop-item-hint {
    font-size: 9px;
    opacity: 0.4;
  }
</style>
