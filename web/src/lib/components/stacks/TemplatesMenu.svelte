<!--
  TemplatesMenu — the draft card's single sectioned templates control
  (Creation-Flow-1 §5). One button (book icon + the word `templates` + a
  chevron, no other label) opens one color-coded menu with four sections:

    1. presets (teal)          — the five PRESET_CATALOG presets
    2. prompt templates (sun)  — fill the draft (preset + goal + provenance)
    3. stack templates (violet)— drop the whole chain into the pane at once
    4. save (dim)              — save this prompt… / save this stack…

  Deliberately a plain menu, not a second Popover fork (the brief allows this
  when Popover doesn't fit without contortion — an anchored inline menu with
  four sections is exactly that case). Closes on outside click / Esc /
  selection; the list scrolls when long; every row is a real <button> so it is
  keyboard reachable.
-->
<script lang="ts">
  import {
    type StackCard,
    PRESET_KEYS,
    PRESET_CATALOG,
    PRESET_DESCRIPTIONS,
    applyPreset,
    applyPromptTemplate,
    promptTemplateFromCard,
    stackTemplateFromCards,
    draftIsHot,
    updateDraftInPane,
    applyStackTemplateToPane
  } from '$lib/stores/stack';
  import { templates, savePromptTemplate, saveStackTemplate } from '$lib/stores/templates';
  import { ICONS } from './icons';

  export let draft: StackCard;
  export let paneKey: string;
  export let paneCards: StackCard[] = [];

  let open = false;
  let rootEl: HTMLDivElement | undefined;

  $: hot = draftIsHot(draft);
  $: hasCards = paneCards.length > 0;

  function toggle() {
    open = !open;
  }
  function close() {
    open = false;
  }

  function pickPreset(key: (typeof PRESET_KEYS)[number]) {
    updateDraftInPane(paneKey, applyPreset(draft, key));
    close();
  }
  function pickPrompt(tpl: (typeof $templates)['prompts'][number]) {
    updateDraftInPane(paneKey, applyPromptTemplate(draft, tpl));
    close();
  }
  function pickStack(tpl: (typeof $templates)['stacks'][number]) {
    applyStackTemplateToPane(paneKey, tpl);
    close();
  }
  function savePrompt() {
    if (!hot) return;
    const name = window.prompt('Name this prompt template');
    if (name && name.trim()) savePromptTemplate(promptTemplateFromCard(draft, name.trim()));
    close();
  }
  function saveStack() {
    if (!hasCards) return;
    const name = window.prompt('Name this stack template');
    if (name && name.trim()) saveStackTemplate(stackTemplateFromCards(paneCards, name.trim()));
    close();
  }

  function onWindowClick(e: MouseEvent) {
    if (!open) return;
    if (rootEl && !rootEl.contains(e.target as Node)) close();
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) close();
  }
</script>

<svelte:window on:mousedown|capture={onWindowClick} on:keydown={onKeydown} />

<div class="tmroot" bind:this={rootEl}>
  <button
    type="button"
    class="tmbtn"
    class:open
    aria-haspopup="menu"
    aria-expanded={open}
    title="templates"
    on:click={toggle}
  >
    {@html ICONS.book}<span class="lbl">templates</span>{@html ICONS.chevdown}
  </button>

  {#if open}
    <div class="tmmenu" role="menu">
      <div class="sec presets">
        <div class="sechd">{@html ICONS.loop}presets</div>
        {#each PRESET_KEYS as key (key)}
          <button type="button" class="row" role="menuitem" on:click={() => pickPreset(key)}>
            <span class="nm">:{PRESET_CATALOG[key].label}</span>
            <span class="ds">{PRESET_DESCRIPTIONS[key]}</span>
          </button>
        {/each}
      </div>

      <div class="sec prompts">
        <div class="sechd">{@html ICONS.file}prompt templates</div>
        {#if $templates.prompts.length === 0}
          <div class="empty">none saved yet</div>
        {:else}
          {#each $templates.prompts as tpl (tpl.id)}
            <button type="button" class="row" role="menuitem" on:click={() => pickPrompt(tpl)}>
              <span class="nm">{tpl.name}</span>
              <span class="ds">{tpl.goal}</span>
            </button>
          {/each}
        {/if}
      </div>

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

      <div class="sec save">
        <div class="sechd">{@html ICONS.save}save</div>
        <button type="button" class="row" role="menuitem" disabled={!hot} on:click={savePrompt}>
          <span class="nm">save this prompt…</span>
        </button>
        <button type="button" class="row" role="menuitem" disabled={!hasCards} on:click={saveStack}>
          <span class="nm">save this stack…</span>
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .tmroot {
    position: relative;
    display: inline-flex;
  }
  .tmbtn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 29px;
    padding: 0 10px;
    border-radius: 7px;
    border: 1px solid rgba(255, 255, 255, 0.11);
    background: transparent;
    color: rgba(245, 245, 245, 0.66);
    cursor: pointer;
    font-family: var(--font-mono, monospace);
    font-size: 11.5px;
    transition: 0.12s;
  }
  .tmbtn:hover,
  .tmbtn.open {
    color: var(--konjo-paper, #f5f5f5);
    border-color: rgba(245, 245, 245, 0.46);
  }
  .tmbtn :global(svg) {
    width: 13px;
    height: 13px;
  }
  .tmbtn .lbl {
    letter-spacing: 0.02em;
  }
  .tmmenu {
    position: absolute;
    top: calc(100% + 7px);
    left: 0;
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
  .sec.presets .sechd {
    color: var(--stack-teal, #00ffd4);
  }
  .sec.prompts .sechd {
    color: var(--konjo-sun, #ffcc00);
  }
  .sec.stacks .sechd {
    color: var(--stack-violet, #b79bff);
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
  .sec.presets .row .nm {
    color: var(--stack-teal, #00ffd4);
  }
  .empty {
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    color: rgba(245, 245, 245, 0.28);
    padding: 4px 8px;
  }
</style>
