<!--
  TemplatesMenu — the PROMPT-scope templates control (Stack-Templates-1 §2a).
  Every card gets one: the draft renders it labeled (book icon + the word
  `templates`) in its spec row — the teaching surface; a committed card
  renders it icon-only, sun-accented, in its cardbar immediately left of
  duplicate. Same component, `labeled` just swaps the chrome.

  Menu contents are prompt scope ONLY — presets (teal) + prompt templates
  (sun) + save this prompt. Stack templates and "saved stacks" moved to the
  stack-scope menu (`StackTemplatesMenu.svelte`, in the dock) — a prompt menu
  never offers a stack action.

  Deliberately a plain menu, not a second Popover fork (the brief allows this
  when Popover doesn't fit without contortion — an anchored inline menu with
  three sections is exactly that case). Closes on outside click / Esc /
  selection; the list scrolls when long; every row is a real <button> so it is
  keyboard reachable.

  Positioned `fixed` off the trigger's own bounding rect — same escape hatch
  `Popover.svelte` uses — rather than `absolute` inside an inline wrapper. A
  plain `absolute` menu is clipped by any scrolling/animated ancestor (the
  pane's `.panestack{overflow-y:auto}`, the dock's `.dockbody{overflow:hidden}`
  collapse animation); `fixed` escapes both.
-->
<script lang="ts">
  import { tick } from 'svelte';
  import {
    type StackCard,
    PRESET_KEYS,
    PRESET_CATALOG,
    PRESET_DESCRIPTIONS,
    applyPreset,
    applyPromptTemplate,
    promptTemplateFromCard,
    draftIsHot,
    updateDraftInPane,
    updateCardInPane
  } from '$lib/stores/stack';
  import { templates, savePromptTemplate } from '$lib/stores/templates';
  import { ICONS } from './icons';

  export let card: StackCard;
  export let paneKey: string;
  /** True for the draft's labeled, teaching-surface rendering; false (the
   *  default) for a committed card's icon-only rendering in the cardbar. */
  export let labeled = false;

  let open = false;
  let btnEl: HTMLButtonElement | undefined;
  let menuEl: HTMLDivElement | undefined;
  let left = 0;
  let top = 0;
  let positioned = false;

  // A committed card always has a preset/goal already, so it's always "hot"
  // for the purposes of enabling "save this prompt…"; the draft is only hot
  // once it carries enough to commit.
  $: hot = labeled ? draftIsHot(card) : true;

  /** The draft's labeled button left-aligns its menu; a committed card's
   *  icon-only button sits near the cardbar's right edge, so its menu
   *  right-aligns to avoid overflowing past the card. Flips above the
   *  button when it doesn't fit below, mirroring `Popover.svelte`'s flip —
   *  a card near the bottom of a scrolled pane would otherwise drop the
   *  menu straight off the viewport. */
  async function computePosition() {
    await tick();
    if (!btnEl || !menuEl) {
      positioned = true;
      return;
    }
    const r = btnEl.getBoundingClientRect();
    const mw = menuEl.offsetWidth;
    const mh = menuEl.offsetHeight;
    left = labeled ? r.left : r.right - mw;
    left = Math.min(Math.max(10, left), window.innerWidth - mw - 10);
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

  /** Route the patch to the right store op: the draft edits the pane's
   *  `draft`; a committed card edits itself in `pane.cards`. */
  function writePatch(patch: Partial<StackCard>) {
    if (labeled) updateDraftInPane(paneKey, patch);
    else updateCardInPane(paneKey, card.id, patch);
  }

  function pickPreset(key: (typeof PRESET_KEYS)[number]) {
    writePatch(applyPreset(card, key));
    close();
  }
  function pickPrompt(tpl: (typeof $templates)['prompts'][number]) {
    writePatch(applyPromptTemplate(card, tpl));
    close();
  }
  function savePrompt() {
    if (!hot) return;
    const name = window.prompt('Name this prompt template');
    if (name && name.trim()) savePromptTemplate(promptTemplateFromCard(card, name.trim()));
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

<div class="tmroot">
  {#if labeled}
    <button
      type="button"
      class="tmbtn labeled"
      class:open
      bind:this={btnEl}
      aria-haspopup="menu"
      aria-expanded={open}
      title="templates"
      on:click={toggle}
    >
      {@html ICONS.book}<span class="lbl">templates</span>{@html ICONS.chevdown}
    </button>
  {:else}
    <button
      type="button"
      class="ib tplib"
      class:on={open}
      bind:this={btnEl}
      aria-haspopup="menu"
      aria-expanded={open}
      title="templates"
      on:click={toggle}
    >
      {@html ICONS.book}
    </button>
  {/if}

  {#if open}
    <div class="tmmenu" class:positioned bind:this={menuEl} style="left:{left}px;top:{top}px" role="menu">
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

      <div class="sec save">
        <div class="sechd">{@html ICONS.save}save</div>
        <button type="button" class="row" role="menuitem" disabled={!hot} on:click={savePrompt}>
          <span class="nm">save this prompt…</span>
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
  .tmbtn.labeled {
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
  .tmbtn.labeled:hover,
  .tmbtn.labeled.open {
    color: var(--konjo-paper, #f5f5f5);
    border-color: rgba(245, 245, 245, 0.46);
  }
  .tmbtn.labeled :global(svg) {
    width: 13px;
    height: 13px;
  }
  .tmbtn.labeled .lbl {
    letter-spacing: 0.02em;
  }
  /* Icon-only, committed-card rendering: matches StackCard's `.ib` sizing
     exactly but always carries the sun accent (Stack-Templates-1 §2a) — not
     conditional on `open`, unlike sched/guard/eval which are dim until active. */
  .ib.tplib {
    position: relative;
    height: 29px;
    min-width: 29px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid rgba(255, 204, 0, 0.45);
    background: rgba(255, 204, 0, 0.08);
    color: var(--konjo-sun, #ffcc00);
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
    border-color: rgba(255, 204, 0, 0.85);
    background: rgba(255, 204, 0, 0.18);
  }
  /* `fixed`, not `absolute` — escapes the pane's `.panestack{overflow-y:auto}`
     and the dock's `.dockbody{overflow:hidden}` collapse animation, exactly
     like `Popover.svelte`. `visibility:hidden` until positioned avoids a
     one-frame flash at (0,0) before the rect computation lands. */
  .tmmenu {
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
  .tmmenu.positioned {
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
  .sec.presets .sechd {
    color: var(--stack-teal, #00ffd4);
  }
  .sec.prompts .sechd {
    color: var(--konjo-sun, #ffcc00);
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
