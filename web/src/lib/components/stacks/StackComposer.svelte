<!--
  StackComposer — the fused creation flow (type-first + suggested chip +
  browse-presets grid + inline :alias/@repo/xN grammar), per
  docs/ui/lopi-creation-flow.html's "type-first, inline preset" door.

  UI-1 scope: the only interactive stack mutation this slice is "adding a
  card". Everything else (guardrails, evals editing, run) is later slices.
-->
<script lang="ts">
  import {
    PRESET_CATALOG,
    PRESET_KEYS,
    suggestPreset,
    parseComposerInput,
    buildCard,
    addToStack,
    type PresetKey
  } from '$lib/stores/stack';
  import { ICONS, PRESET_ICON, PRESET_ACCENT } from './icons';

  let value = '';
  /** Explicit door-B selection: a preset key, 'literal', or unset. */
  let selectedPreset: PresetKey | 'literal' | null = null;

  // The grid is the empty-composer default door; it collapses to the chip
  // row the moment typing starts.
  $: showGrid = value.trim().length === 0;
  // Highlight-only — never auto-attached, matches suggestPreset's contract.
  $: suggested = !selectedPreset && value.trim() ? suggestPreset(value) : null;
  $: parsed = value.trim() ? parseComposerInput(value) : null;
  $: parsedBits = parsed
    ? [
        parsed.alias && PRESET_CATALOG[parsed.alias as PresetKey] ? `:${parsed.alias}` : null,
        parsed.repo ? `@${parsed.repo}` : null,
        parsed.loopN ? `x${parsed.loopN}` : null
      ].filter((s): s is string => !!s)
    : [];

  function pickPreset(key: PresetKey | 'literal') {
    selectedPreset = selectedPreset === key ? null : key;
  }

  function submit() {
    const text = value.trim();
    if (!text) return;
    const explicit =
      selectedPreset && selectedPreset !== 'literal' ? (selectedPreset as PresetKey) : undefined;
    addToStack(buildCard(text, explicit));
    value = '';
    selectedPreset = null;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      submit();
    }
  }
</script>

<div class="composer-wrap">
  <div class="composer">
    <span class="pr">&gt;</span>
    <input
      bind:value
      on:keydown={onKeydown}
      placeholder="type a goal or :alias… (adds to top of the stack)"
      spellcheck="false"
    />
    <button class="add" on:click={submit} disabled={!value.trim()} title="add to stack">
      {@html ICONS.plus}
    </button>
  </div>

  {#if parsedBits.length}
    <div class="parsed">
      <span class="lbl">parsed</span>
      {#each parsedBits as bit}<span class="bit">{bit}</span>{/each}
    </div>
  {/if}

  <div class="chiprow">
    <span class="chiplabel">as a</span>
    {#each PRESET_KEYS as key (key)}
      <button
        type="button"
        class="pchip"
        class:on={selectedPreset === key}
        class:suggested={!selectedPreset && suggested === key}
        style="--accent:{PRESET_ACCENT[key]}"
        on:click={() => pickPreset(key)}
      >
        {@html PRESET_ICON[key]}{PRESET_CATALOG[key].label}
        {#if !selectedPreset && suggested === key}<span class="hint">· suggested</span>{/if}
      </button>
    {/each}
    <button
      type="button"
      class="pchip literal"
      class:on={selectedPreset === 'literal'}
      on:click={() => pickPreset('literal')}
    >
      literal
    </button>
  </div>

  {#if showGrid}
    <div class="grid">
      {#each PRESET_KEYS as key (key)}
        <button
          type="button"
          class="card"
          class:on={selectedPreset === key}
          style="--accent:{PRESET_ACCENT[key]}"
          on:click={() => pickPreset(key)}
        >
          <div class="pn">{@html PRESET_ICON[key]}{PRESET_CATALOG[key].label}</div>
          <div class="pev">
            {@html ICONS.checkbox}<span>{PRESET_CATALOG[key].evals.length} evals</span>
            <span class="n">· auto-attached</span>
          </div>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .composer-wrap {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .composer {
    display: flex;
    align-items: center;
    gap: 10px;
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 9px;
    padding: 11px 14px;
    background: var(--konjo-deep, #0e1214);
  }
  .pr {
    color: var(--konjo-flame);
    font-family: var(--font-mono, monospace);
    font-size: 16px;
    flex: 0 0 auto;
  }
  .composer input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    color: var(--konjo-paper);
    font-family: var(--font-mono, monospace);
    font-size: 13px;
  }
  .composer input::placeholder {
    opacity: 0.32;
  }
  .add {
    width: 32px;
    height: 32px;
    flex: 0 0 auto;
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 7px;
    background: transparent;
    color: rgba(245, 245, 245, 0.28);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .add :global(svg) {
    width: 15px;
    height: 15px;
  }
  .add:not(:disabled):hover {
    color: var(--konjo-flame);
    border-color: rgba(255, 149, 0, 0.4);
  }
  .add:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .parsed {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono, monospace);
    font-size: 9.5px;
    color: rgba(245, 245, 245, 0.4);
  }
  .parsed .lbl {
    text-transform: uppercase;
    letter-spacing: 0.1em;
    opacity: 0.6;
  }
  .parsed .bit {
    color: var(--konjo-jade);
    border: 1px solid rgba(0, 255, 157, 0.3);
    border-radius: 10px;
    padding: 1px 8px;
  }
  .chiprow {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: center;
  }
  .chiplabel {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: rgba(245, 245, 245, 0.28);
    margin-right: 2px;
  }
  .pchip {
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    color: rgba(245, 245, 245, 0.46);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 16px;
    padding: 5px 11px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: transparent;
    cursor: pointer;
    transition: 0.14s;
  }
  .pchip :global(svg) {
    width: 11px;
    height: 11px;
  }
  .pchip:hover {
    color: var(--konjo-paper);
  }
  .pchip.on {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .pchip.suggested {
    border-style: dashed;
    border-color: rgba(255, 149, 0, 0.45);
    color: var(--konjo-flame);
  }
  .pchip.literal {
    color: rgba(245, 245, 245, 0.46);
  }
  .pchip .hint {
    opacity: 0.75;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 9px;
  }
  .card {
    text-align: left;
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 9px;
    padding: 12px;
    cursor: pointer;
    background: transparent;
    transition: 0.14s;
    position: relative;
    overflow: hidden;
  }
  .card::before {
    content: '';
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    background: var(--accent);
  }
  .card:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .card.on {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }
  .pn {
    font-family: var(--font-mono, monospace);
    font-size: 13px;
    color: var(--accent);
    display: flex;
    align-items: center;
    gap: 7px;
    margin-bottom: 5px;
  }
  .pn :global(svg) {
    width: 14px;
    height: 14px;
  }
  .pev {
    font-family: var(--font-mono, monospace);
    font-size: 8.5px;
    color: var(--konjo-jade);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .pev :global(svg) {
    width: 10px;
    height: 10px;
  }
  .pev .n {
    color: rgba(245, 245, 245, 0.28);
  }
</style>
