<!--
  StackTemplatesMenu — the STACK-scope templates control (Stack-Templates-1
  §2b), in the dock's button row, icon-only, violet-accented, immediately
  left of duplicate. Menu contents are stack scope ONLY:

    1. stack templates (violet) — drop the whole chain into this pane
    2. saved stacks           — the other panes currently open ($panes);
                                 picking one copies its cards into this pane
    3. save this stack as template… (disabled when the pane has no cards)

  No presets, no prompt templates — those live on each card
  (`TemplatesMenu.svelte`). "Saved stacks" is deliberately thin: nothing
  persists a stack yet (that's `Persistence-1`), so this only ever lists
  panes that are open right now, in memory, in this browser tab.

  Positioned `fixed` off the button's own bounding rect (mirrors
  `Popover.svelte` and `TemplatesMenu.svelte`) rather than `absolute` — the
  dock's `.dockbody{overflow:hidden}` collapse animation would otherwise clip
  an `absolute` menu the moment it grows taller than the collapsed strip.
-->
<script lang="ts">
  import { tick } from 'svelte';
  import {
    type StackCard,
    type StackTemplate,
    panes,
    applyStackTemplateToPane,
    stackTemplateFromCards,
    loadStackCardsIntoPane
  } from '$lib/stores/stack';
  import { templates, saveStackTemplate } from '$lib/stores/templates';
  import { ICONS } from './icons';

  export let paneKey: string;
  export let cards: StackCard[] = [];

  let open = false;
  let btnEl: HTMLButtonElement | undefined;
  let menuEl: HTMLDivElement | undefined;
  let left = 0;
  let top = 0;
  let positioned = false;

  $: hasCards = cards.length > 0;
  $: otherPanes = $panes.filter((p) => p.key !== paneKey);

  /** Right-aligns under the button (it sits near the dock's right edge),
   *  clamped into the viewport — same shape as `TemplatesMenu`'s. Flips
   *  above the button when it doesn't fit below (the dock sits at the
   *  bottom of the pane, so "below" often means "off-screen"), mirroring
   *  `Popover.svelte`'s flip. */
  async function computePosition() {
    await tick();
    if (!btnEl || !menuEl) {
      positioned = true;
      return;
    }
    const r = btnEl.getBoundingClientRect();
    const mw = menuEl.offsetWidth;
    const mh = menuEl.offsetHeight;
    left = Math.min(Math.max(10, r.right - mw), window.innerWidth - mw - 10);
    top = r.bottom + 7 + mh > window.innerHeight - 10 ? Math.max(10, r.top - mh - 7) : r.bottom + 7;
    positioned = true;
  }

  function toggle() {
    open = !open;
    if (open) {
      positioned = false;
      computePosition();
    }
  }
  function close() {
    open = false;
  }

  function pickStack(tpl: StackTemplate) {
    applyStackTemplateToPane(paneKey, tpl);
    close();
  }
  function openSavedStack(sourceKey: string) {
    loadStackCardsIntoPane(paneKey, sourceKey);
    close();
  }
  function saveStack() {
    if (!hasCards) return;
    const name = window.prompt('Name this stack template');
    if (name && name.trim()) saveStackTemplate(stackTemplateFromCards(cards, name.trim()));
    close();
  }

  function onWindowClick(e: MouseEvent) {
    if (!open) return;
    const target = e.target as Node;
    if (btnEl?.contains(target)) return;
    if (menuEl?.contains(target)) return;
    close();
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) close();
  }
</script>

<svelte:window on:mousedown|capture={onWindowClick} on:keydown={onKeydown} on:resize={computePosition} on:scroll|capture={close} />

<div class="stroot">
  <button
    type="button"
    class="ib tplib"
    class:on={open}
    bind:this={btnEl}
    aria-haspopup="menu"
    aria-expanded={open}
    title="stack templates"
    on:click={toggle}
  >
    {@html ICONS.book}
  </button>

  {#if open}
    <div class="stmenu" class:positioned bind:this={menuEl} style="left:{left}px;top:{top}px" role="menu">
      <div class="sec stacks">
        <div class="sechd">{@html ICONS.layers}stack templates</div>
        {#if $templates.stacks.length === 0}
          <div class="empty">none saved yet</div>
        {:else}
          {#each $templates.stacks as tpl (tpl.id)}
            <button type="button" class="row" role="menuitem" on:click={() => pickStack(tpl)}>
              <span class="nm">{tpl.name}</span>
              <span class="ds">{tpl.loops.length} loop{tpl.loops.length === 1 ? '' : 's'}</span>
            </button>
          {/each}
        {/if}
      </div>

      <div class="sec saved">
        <div class="sechd">{@html ICONS.grid}saved stacks</div>
        {#if otherPanes.length === 0}
          <div class="empty">no other open stacks</div>
        {:else}
          {#each otherPanes as p (p.key)}
            <button type="button" class="row" role="menuitem" on:click={() => openSavedStack(p.key)}>
              <span class="nm">{p.title}</span>
              <span class="ds">{p.cards.length} loop{p.cards.length === 1 ? '' : 's'}</span>
            </button>
          {/each}
        {/if}
      </div>

      <div class="sec save">
        <div class="sechd">{@html ICONS.save}save</div>
        <button type="button" class="row" role="menuitem" disabled={!hasCards} on:click={saveStack}>
          <span class="nm">save this stack…</span>
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .stroot {
    position: relative;
    display: inline-flex;
  }
  .ib.tplib {
    position: relative;
    height: 29px;
    min-width: 29px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid rgba(183, 155, 255, 0.45);
    background: rgba(183, 155, 255, 0.1);
    color: var(--stack-violet, #b79bff);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    transition: 0.12s;
  }
  .ib.tplib :global(svg) {
    width: 14px;
    height: 14px;
  }
  .ib.tplib:hover,
  .ib.tplib.on {
    border-color: rgba(183, 155, 255, 0.85);
    background: rgba(183, 155, 255, 0.2);
  }
  .stmenu {
    position: fixed;
    z-index: 40;
    width: 288px;
    max-width: min(288px, calc(100vw - 24px));
    max-height: 60vh;
    overflow-y: auto;
    background: var(--konjo-panel, #0a0d0f);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 10px;
    box-shadow: 0 18px 50px rgba(0, 0, 0, 0.75);
    padding: 6px;
    visibility: hidden;
  }
  .stmenu.positioned {
    visibility: visible;
  }
  .sec {
    padding: 4px 0;
  }
  .sec + .sec {
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    margin-top: 2px;
  }
  .sechd {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono, monospace);
    font-size: 8.5px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    padding: 5px 8px;
  }
  .sechd :global(svg) {
    width: 11px;
    height: 11px;
  }
  .sec.stacks .sechd {
    color: var(--stack-violet, #b79bff);
  }
  .sec.saved .sechd {
    color: rgba(245, 245, 245, 0.66);
  }
  .sec.save .sechd {
    color: rgba(245, 245, 245, 0.46);
  }
  .row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: 6px;
    padding: 6px 8px;
    cursor: pointer;
    transition: background 0.12s;
  }
  .row:hover:not(:disabled),
  .row:focus-visible {
    background: rgba(255, 255, 255, 0.04);
    outline: none;
  }
  .row:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .row .nm {
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    color: var(--konjo-paper, #f5f5f5);
  }
  .row .ds {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: rgba(245, 245, 245, 0.46);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .sec.stacks .row .nm {
    color: var(--stack-violet, #b79bff);
  }
  .empty {
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    color: rgba(245, 245, 245, 0.28);
    padding: 4px 8px;
  }
</style>
