<!--
  StackCard — one loop in the stack, rendered statically.

  UI-1 scope: display only. The card-bar buttons (loop pill, cron, shield,
  evals, duplicate, drag, delete) render but are disabled placeholders —
  UI-2 wires their popovers and array-op handlers. The two summary lines are
  read-only derived text, not live state.
-->
<script lang="ts">
  import type { StackCard as StackCardT } from '$lib/stores/stack';
  import { guardrailsSummary, evalsSummary } from '$lib/stores/stack';
  import { ICONS, PRESET_ICON, PRESET_ACCENT } from './icons';

  export let card: StackCardT;
  /** The bottom-most card in the stack — the next one that would run. */
  export let isNext = false;

  $: accent = card.preset ? PRESET_ACCENT[card.preset] : 'var(--konjo-dim2, rgba(245,245,245,.28))';
  $: presetIcon = card.preset ? PRESET_ICON[card.preset] : '';
</script>

<div class="pc" class:next={isNext} style="--accent:{accent}">
  {#if isNext}<span class="nexttag">next</span>{/if}

  {#if card.preset}
    <span class="preset" style="color:{accent};border-color:{accent}">
      {@html presetIcon}{card.preset}
    </span>
  {/if}

  <div class="spec">
    {#if card.alias}
      <span class="al">:{card.alias}</span>
    {/if}
    {#if card.literal}
      <span class="str">"{card.goal}"</span>
    {:else if card.goal}
      <span class="md">"{card.goal}"</span>
    {/if}
    {#if card.repo}<span class="rp">@{card.repo}</span>{/if}
    {#if card.loopN}<span class="md">x{card.loopN}</span>{/if}
  </div>

  <div class="cardbar">
    <span class="ib iter" class:on={!!card.loopN} title="loop count (UI-2)">
      {@html ICONS.loop}{#if card.loopN}<span class="val">×{card.loopN}</span>{/if}
    </span>
    <button class="ib" disabled title="schedule (UI-2)">{@html ICONS.cron}</button>
    <button class="ib" disabled title="guardrails (UI-2)">{@html ICONS.shield}</button>
    <button class="ib" disabled title="evals (UI-2)">{@html ICONS.checkbox}</button>
    <span class="sp"></span>
    <button class="ib" disabled title="duplicate (UI-2)">{@html ICONS.dup}</button>
    <button class="ib grip" disabled title="reorder (UI-2)">{@html ICONS.drag}</button>
    <button class="ib danger" disabled title="delete (UI-2)">{@html ICONS.trash}</button>
  </div>

  <div class="sumln guard">
    <span class="rl">{@html ICONS.shield}guards</span>
    <span class="txt">{guardrailsSummary(card)}</span>
  </div>
  <div class="sumln eval">
    <span class="rl">{@html ICONS.checkbox}evals</span>
    <span class="txt">{evalsSummary(card)}</span>
  </div>
</div>

<style>
  .pc {
    position: relative;
    background: var(--konjo-deep, #0e1214);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 8px;
    padding: 12px 13px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
  }
  .pc.next {
    border-color: color-mix(in srgb, var(--konjo-flame) 55%, transparent);
  }
  .nexttag {
    position: absolute;
    top: -8px;
    right: 10px;
    font-size: 8.5px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: #160500;
    background: var(--konjo-flame);
    padding: 1px 6px;
    border-radius: 2px;
    font-weight: 700;
  }
  .preset {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 8.5px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    border: 1px solid;
    border-radius: 12px;
    padding: 2px 8px;
    margin-bottom: 9px;
  }
  .preset :global(svg) {
    width: 10px;
    height: 10px;
  }
  .spec {
    font-size: 12.5px;
    line-height: 1.5;
    word-break: break-word;
  }
  .spec .al {
    color: var(--konjo-ice);
  }
  .spec .str {
    color: var(--konjo-paper);
  }
  .spec .md {
    color: rgba(245, 245, 245, 0.46);
  }
  .spec .rp {
    color: var(--konjo-sun);
  }
  .spec > :global(span + span) {
    margin-left: 6px;
  }
  .cardbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 10px;
    padding-top: 9px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .ib {
    height: 27px;
    min-width: 27px;
    padding: 0 6px;
    border-radius: 5px;
    border: 1px solid rgba(255, 255, 255, 0.11);
    background: transparent;
    color: rgba(245, 245, 245, 0.28);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    font-size: 10.5px;
    cursor: not-allowed;
  }
  .ib :global(svg) {
    width: 13px;
    height: 13px;
  }
  .ib.iter.on {
    border-color: rgba(255, 149, 0, 0.5);
    background: rgba(255, 69, 0, 0.09);
    color: var(--konjo-flame);
  }
  .ib.iter .val {
    font-weight: 700;
  }
  .sp {
    flex: 1;
  }
  .sumln {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 8px;
    font-size: 9.5px;
    min-width: 0;
  }
  .sumln .rl {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    font-size: 8px;
    flex: 0 0 auto;
    width: 52px;
  }
  .sumln .rl :global(svg) {
    width: 11px;
    height: 11px;
  }
  .sumln.guard .rl {
    color: var(--konjo-sun);
  }
  .sumln.eval .rl {
    color: var(--konjo-jade);
  }
  .sumln .txt {
    color: rgba(245, 245, 245, 0.46);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
</style>
